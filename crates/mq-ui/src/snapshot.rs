use anyhow::{Context, Result};
use egui::Context as EguiContext;
use std::path::Path;

use crate::app::MqApp;
use crate::fonts::configure_system_fonts;
use crate::theme::{apply_theme, Tokens};

/// Options controlling offscreen snapshot rendering (DPI scale, pointer hover, input events).
#[derive(Debug, Clone)]
pub struct SnapshotOptions {
    /// UI scale factor (`pixels_per_point`), e.g. `1.0`, `1.5`, `2.0`.
    /// Logical viewport is `width x height`; output PNG is `(width * scale) x (height * scale)`.
    pub scale: f32,
    /// Optional pointer position in logical coordinates to trigger hover states.
    pub pointer_pos: Option<egui::Pos2>,
    /// Extra egui events (e.g. `Tab` key press for focus ring) injected during frame passes.
    pub extra_events: Vec<egui::Event>,
}

impl Default for SnapshotOptions {
    fn default() -> Self {
        Self {
            scale: 1.0,
            pointer_pos: None,
            extra_events: Vec::new(),
        }
    }
}

/// Reusable offscreen wgpu session for fast batch snapshot rendering.
pub struct SnapshotSession {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl SnapshotSession {
    /// Initializes a headless wgpu device and queue once for multiple snapshots.
    pub async fn new() -> Result<Self> {
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

        Ok(Self { device, queue })
    }

    /// Renders a single frame of `MqApp` to `out_path` using the given logical viewport and options.
    pub fn capture(
        &self,
        app: &mut MqApp,
        width: u32,
        height: u32,
        out_path: &Path,
        opts: &SnapshotOptions,
    ) -> Result<()> {
        let scale = opts.scale.clamp(0.5, 4.0);
        let phys_width = ((width as f32) * scale).round().max(1.0) as u32;
        let phys_height = ((height as f32) * scale).round().max(1.0) as u32;

        let target_format = wgpu::TextureFormat::Rgba8Unorm;
        let texture_desc = wgpu::TextureDescriptor {
            label: Some("MQ Snapshot Target Texture"),
            size: wgpu::Extent3d {
                width: phys_width,
                height: phys_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: target_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        };
        let target_texture = self.device.create_texture(&texture_desc);
        let target_view = target_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut renderer = egui_wgpu::Renderer::new(&self.device, target_format, None, 1, false);

        let egui_ctx = EguiContext::default();
        apply_theme(&egui_ctx, &app.state.config.theme);
        configure_system_fonts(&egui_ctx);
        egui_ctx.set_pixels_per_point(scale);

        let mut events = Vec::new();
        if let Some(pos) = opts.pointer_pos {
            events.push(egui::Event::PointerMoved(pos));
        }
        events.extend(opts.extra_events.iter().cloned());

        // Warm-up passes so egui can measure text, settle Window anchors/animations, and register hover/focus
        for step in 0..4 {
            let warm_input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width as f32, height as f32),
                )),
                time: Some(step as f64 * 0.25),
                events: events.clone(),
                ..Default::default()
            };
            let warm_output = egui_ctx.run(warm_input, |ctx| {
                app.render_frame_for_snapshot(ctx);
            });
            for (id, delta) in &warm_output.textures_delta.set {
                renderer.update_texture(&self.device, &self.queue, *id, delta);
            }
            for id in &warm_output.textures_delta.free {
                renderer.free_texture(id);
            }
        }

        let final_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width as f32, height as f32),
            )),
            time: Some(1.0),
            events: opts
                .pointer_pos
                .map(egui::Event::PointerMoved)
                .into_iter()
                .collect(),
            ..Default::default()
        };

        // Run the final UI pass
        let full_output = egui_ctx.run(final_input, |ctx| {
            app.render_frame_for_snapshot(ctx);
        });

        let clipped_primitives = egui_ctx.tessellate(full_output.shapes, scale);

        for (id, delta) in &full_output.textures_delta.set {
            renderer.update_texture(&self.device, &self.queue, *id, delta);
        }

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [phys_width, phys_height],
            pixels_per_point: scale,
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("MQ Snapshot Encoder"),
            });

        renderer.update_buffers(
            &self.device,
            &self.queue,
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
        let unpadded_bytes_per_row = phys_width * bytes_per_pixel;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
        let buffer_size = (padded_bytes_per_row * phys_height) as u64;

        let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
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
                    rows_per_image: Some(phys_height),
                },
            },
            wgpu::Extent3d {
                width: phys_width,
                height: phys_height,
                depth_or_array_layers: 1,
            },
        );

        self.queue.submit(std::iter::once(encoder.finish()));

        let slice = output_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });

        self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .context("Channel dropped before buffer mapped")?
            .context("GPU buffer map error")?;

        let mapped_view = slice.get_mapped_range();
        let mut image_bytes = Vec::with_capacity((phys_width * phys_height * 4) as usize);
        for row in 0..phys_height {
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
            phys_width,
            phys_height,
            image::ExtendedColorType::Rgba8,
        )
        .context("Failed to write snapshot PNG file")?;

        Ok(())
    }
}

/// Renders a selected screen of `MqApp` into a high-resolution PNG file completely offscreen via wgpu.
pub async fn capture_app_snapshot(
    app: &mut MqApp,
    width: u32,
    height: u32,
    out_path: &Path,
) -> Result<()> {
    capture_app_snapshot_with_options(app, width, height, out_path, &SnapshotOptions::default())
        .await
}

/// Renders a selected screen of `MqApp` with explicit [`SnapshotOptions`] (scale, hover, events).
pub async fn capture_app_snapshot_with_options(
    app: &mut MqApp,
    width: u32,
    height: u32,
    out_path: &Path,
    opts: &SnapshotOptions,
) -> Result<()> {
    let session = SnapshotSession::new().await?;
    session.capture(app, width, height, out_path, opts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use mq_core::{AppConfig, InstanceId, ModLoaderKind, MqPaths, ThemePreset};
    use mq_instances::InstanceManifest;
    use mq_sched::ComputePool;

    use crate::accounts_view::AccountsView;
    use crate::catalog_demo::apply_demo_scene;
    use crate::catalog_view::CatalogView;
    use crate::instances_view::{InstanceViewMode, InstancesView};
    use crate::perf_view::PerfView;
    use crate::settings_view::{SettingsSection, SettingsView};
    use crate::sidebar::NavTab;
    use crate::state::UiState;

    fn build_demo_app_for_qa(
        root: &Path,
        tab: NavTab,
        preset: ThemePreset,
        scene: &str,
    ) -> Result<MqApp> {
        let paths = MqPaths::with_root(root.to_path_buf());
        std::fs::create_dir_all(paths.instances_dir())?;

        let mut config = AppConfig {
            launcher_locale: "ru".to_string(),
            ..Default::default()
        };
        config.theme.preset = preset;

        let pool = Arc::new(ComputePool::new(2));
        let mut state = UiState::new(paths.clone(), config, pool)?;

        let mut m1 = InstanceManifest::new("26.1 Vanilla", "26.1", ModLoaderKind::Vanilla, None);
        m1.id = InstanceId::new("vanilla-latest")?;
        let mut m2 = InstanceManifest::new(
            "Fabric Optimization",
            "26.1",
            ModLoaderKind::Fabric,
            Some("0.16.9".into()),
        );
        m2.id = InstanceId::new("fabric-optimization")?;
        let mut m3 = InstanceManifest::new(
            "NeoForge Adventures",
            "26.1",
            ModLoaderKind::NeoForge,
            Some("21.1.65".into()),
        );
        m3.id = InstanceId::new("neoforge-adventures")?;

        state.instances = vec![m1, m2, m3];
        state.selected_instance_id = Some(InstanceId::new("fabric-optimization")?);

        if let Ok(profile) = mq_auth::create_offline_account("Player") {
            state.accounts.upsert_account(profile, true);
        }

        let mut instances_view = InstancesView::default();
        if scene == "instances-list" {
            instances_view.view_mode = InstanceViewMode::List;
        } else {
            instances_view.view_mode = InstanceViewMode::Grid;
        }

        let settings_view = SettingsView {
            active_section: Some(SettingsSection::Customization),
            ..Default::default()
        };

        apply_demo_scene(&mut state, scene);

        Ok(MqApp {
            state,
            current_tab: tab,
            instances_view,
            catalog_view: CatalogView::default(),
            accounts_view: AccountsView::default(),
            perf_view: PerfView,
            settings_view,
            frame_crash_message: None,
            recovery_journal: None,
        })
    }

    #[test]
    fn snapshot_options_defaults() {
        let opts = SnapshotOptions::default();
        assert!((opts.scale - 1.0).abs() < f32::EPSILON);
        assert!(opts.pointer_pos.is_none());
        assert!(opts.extra_events.is_empty());
    }

    #[tokio::test]
    #[ignore = "renders full offscreen PNG QA matrix via wgpu"]
    async fn generate_qa_snapshots() -> Result<()> {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let qa_dir = manifest_dir.join("../../docs/assets/screenshots/qa");
        std::fs::create_dir_all(&qa_dir)?;

        let session = SnapshotSession::new().await?;
        let tmp = tempfile::tempdir()?;

        let cases: Vec<(&str, NavTab, ThemePreset, &str, u32, u32, SnapshotOptions)> = vec![
            (
                "01-instances-empty.png",
                NavTab::Instances,
                ThemePreset::MatteDark,
                "instances-empty",
                1180,
                740,
                SnapshotOptions::default(),
            ),
            (
                "02-instances-list.png",
                NavTab::Instances,
                ThemePreset::MatteDark,
                "instances-list",
                1180,
                740,
                SnapshotOptions::default(),
            ),
            (
                "03-instances-create-dialog.png",
                NavTab::Instances,
                ThemePreset::MatteDark,
                "instances-create",
                1180,
                740,
                SnapshotOptions::default(),
            ),
            (
                "04-sidebar-compact-960x600.png",
                NavTab::Instances,
                ThemePreset::MatteDark,
                "instances-grid",
                960,
                600,
                SnapshotOptions::default(),
            ),
            (
                "05-sidebar-hover.png",
                NavTab::Instances,
                ThemePreset::MatteDark,
                "instances-grid",
                1180,
                740,
                SnapshotOptions {
                    pointer_pos: Some(egui::Pos2::new(112.0, 275.0)),
                    ..Default::default()
                },
            ),
            (
                "06-sidebar-focus.png",
                NavTab::Instances,
                ThemePreset::MatteDark,
                "instances-grid",
                1180,
                740,
                SnapshotOptions {
                    extra_events: vec![egui::Event::Key {
                        key: egui::Key::Tab,
                        physical_key: Some(egui::Key::Tab),
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    ..Default::default()
                },
            ),
            (
                "07-sidebar-long-nick.png",
                NavTab::Instances,
                ThemePreset::MatteDark,
                "sidebar-long-nick",
                1180,
                740,
                SnapshotOptions::default(),
            ),
            (
                "08-sidebar-downloads.png",
                NavTab::Instances,
                ThemePreset::MatteDark,
                "sidebar-downloads",
                1180,
                740,
                SnapshotOptions::default(),
            ),
            (
                "09-viewport-1920x1080.png",
                NavTab::Instances,
                ThemePreset::MatteDark,
                "instances-grid",
                1920,
                1080,
                SnapshotOptions::default(),
            ),
            (
                "10-scale-1_5.png",
                NavTab::Instances,
                ThemePreset::MatteDark,
                "instances-grid",
                1180,
                740,
                SnapshotOptions {
                    scale: 1.5,
                    ..Default::default()
                },
            ),
            (
                "11-scale-2_0.png",
                NavTab::Instances,
                ThemePreset::MatteDark,
                "instances-grid",
                1180,
                740,
                SnapshotOptions {
                    scale: 2.0,
                    ..Default::default()
                },
            ),
        ];

        for (filename, tab, preset, scene, w, h, opts) in cases {
            let mut app = build_demo_app_for_qa(tmp.path(), tab, preset, scene)?;
            session.capture(&mut app, w, h, &qa_dir.join(filename), &opts)?;
        }

        for (idx, preset) in ThemePreset::ALL.iter().enumerate() {
            let filename = format!("theme-{:02}-{preset:?}.png", idx + 1);
            let mut app = build_demo_app_for_qa(tmp.path(), NavTab::Settings, *preset, "settings")?;
            session.capture(
                &mut app,
                1180,
                740,
                &qa_dir.join(filename),
                &SnapshotOptions::default(),
            )?;
        }

        Ok(())
    }
}
