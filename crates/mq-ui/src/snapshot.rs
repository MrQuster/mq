use anyhow::{Context, Result};
use egui::Context as EguiContext;
use std::path::Path;

use crate::app::MqApp;
use crate::fonts::configure_system_fonts;
use crate::theme::{apply_theme, Tokens};

/// Renders a selected screen of `MqApp` into a high-resolution PNG file completely offscreen via wgpu.
pub async fn capture_app_snapshot(
    app: &mut MqApp,
    width: u32,
    height: u32,
    out_path: &Path,
) -> Result<()> {
    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await
        .context("No suitable GPU or software adapter found for offscreen rendering")?;

    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: Some("MQ Snapshot Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        )
        .await
        .context("Failed to create wgpu device for snapshot")?;

    let target_format = wgpu::TextureFormat::Rgba8Unorm;
    let texture_desc = wgpu::TextureDescriptor {
        label: Some("MQ Snapshot Target Texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: target_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    };
    let target_texture = device.create_texture(&texture_desc);
    let target_view = target_texture.create_view(&wgpu::TextureViewDescriptor::default());

    let mut renderer = egui_wgpu::Renderer::new(&device, target_format, None, 1, false);

    let egui_ctx = EguiContext::default();
    apply_theme(&egui_ctx, &app.state.config.theme);
    configure_system_fonts(&egui_ctx);

    let raw_input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(width as f32, height as f32),
        )),
        ..Default::default()
    };

    // Warm-up pass so egui can measure text and allocate layout geometry
    let warm_output = egui_ctx.run(raw_input.clone(), |ctx| {
        app.render_frame_for_snapshot(ctx);
    });
    for (id, delta) in &warm_output.textures_delta.set {
        renderer.update_texture(&device, &queue, *id, delta);
    }
    for id in &warm_output.textures_delta.free {
        renderer.free_texture(id);
    }

    // Run the final UI pass
    let full_output = egui_ctx.run(raw_input, |ctx| {
        app.render_frame_for_snapshot(ctx);
    });

    let clipped_primitives = egui_ctx.tessellate(full_output.shapes, 1.0);

    for (id, delta) in &full_output.textures_delta.set {
        renderer.update_texture(&device, &queue, *id, delta);
    }

    let screen_descriptor = egui_wgpu::ScreenDescriptor {
        size_in_pixels: [width, height],
        pixels_per_point: 1.0,
    };

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("MQ Snapshot Encoder"),
    });

    renderer.update_buffers(
        &device,
        &queue,
        &mut encoder,
        &clipped_primitives,
        &screen_descriptor,
    );

    let tokens = Tokens::from_config(&app.state.config.theme);
    let bg = tokens.bg_app;

    {
        let mut rpass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("MQ Snapshot Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: bg.r() as f64 / 255.0,
                            g: bg.g() as f64 / 255.0,
                            b: bg.b() as f64 / 255.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            })
            .forget_lifetime();

        renderer.render(&mut rpass, &clipped_primitives, &screen_descriptor);
    }

    // Free textures
    for id in &full_output.textures_delta.free {
        renderer.free_texture(id);
    }

    // Read back texture pixels aligned to 256 bytes per row
    let bytes_per_pixel = 4u32;
    let unpadded_bytes_per_row = width * bytes_per_pixel;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
    let buffer_size = (padded_bytes_per_row * height) as u64;

    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("MQ Snapshot Readback Buffer"),
        size: buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &target_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &output_buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    queue.submit(std::iter::once(encoder.finish()));

    let slice = output_buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });

    device.poll(wgpu::Maintain::Wait);
    rx.recv()
        .context("Channel dropped before buffer mapped")?
        .context("GPU buffer map error")?;

    let mapped_view = slice.get_mapped_range();
    let mut image_bytes = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        let start = (row * padded_bytes_per_row) as usize;
        let end = start + unpadded_bytes_per_row as usize;
        image_bytes.extend_from_slice(&mapped_view[start..end]);
    }
    drop(mapped_view);
    output_buffer.unmap();

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    image::save_buffer(
        out_path,
        &image_bytes,
        width,
        height,
        image::ExtendedColorType::Rgba8,
    )
    .context("Failed to write snapshot PNG file")?;

    Ok(())
}
