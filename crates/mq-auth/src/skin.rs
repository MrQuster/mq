//! 2D Minecraft skin face/hat compositor and deterministic offline pixel avatars.
//!
//! Decodes `64x64` and legacy `64x32` Minecraft skin PNGs, composites the `8x8`
//! base face `(8..16, 8..16)` with the `8x8` outer hat/helmet layer `(40..48, 8..16)`,
//! and provides a crisp symmetric `8x8` geometric avatar for offline or skinless profiles.

use image::ImageFormat;
use md5::{Digest, Md5};
use mq_core::{MqError, MqResult};
use reqwest::StatusCode;

use crate::elyby::ELYBY_SKINS_BASE_URL;
use crate::models::SkinHeadAvatar;

const FACE_X: u32 = 8;
const FACE_Y: u32 = 8;
const HAT_X: u32 = 40;
const HAT_Y: u32 = 8;
const HEAD_SIZE: u32 = SkinHeadAvatar::SIZE;

/// Slate background tones for deterministic offline avatars.
const SLATE_BACKGROUNDS: [[u8; 4]; 4] = [
    [20, 27, 38, 255],
    [25, 34, 48, 255],
    [18, 31, 37, 255],
    [28, 33, 46, 255],
];

/// Primary emerald/cobalt/teal foreground palette for deterministic offline avatars.
const PRIMARY_PALETTE: [[u8; 4]; 8] = [
    [16, 185, 129, 255], // Emerald 500
    [52, 211, 153, 255], // Emerald 400
    [59, 130, 246, 255], // Cobalt / Blue 500
    [37, 99, 235, 255],  // Royal Cobalt / Blue 600
    [20, 184, 166, 255], // Teal 500
    [56, 189, 248, 255], // Sky Cobalt 400
    [45, 212, 191, 255], // Seafoam Teal 400
    [96, 165, 250, 255], // Soft Cobalt 400
];

/// Crisp highlight accents paired with the emerald/cobalt/slate theme.
const ACCENT_PALETTE: [[u8; 4]; 4] = [
    [167, 243, 208, 255], // Light Emerald
    [147, 197, 253, 255], // Light Cobalt
    [148, 163, 184, 255], // Cool Slate 400
    [204, 251, 241, 255], // Ice Mint
];

/// Decodes a `64x64` or `64x32` Minecraft skin PNG and extracts the composited `8x8` head avatar.
///
/// Alpha-blends the outer hat/helmet layer `(40..48, 8..16)` over the base face `(8..16, 8..16)`
/// using straight-alpha `out = hat * alpha + base * (1 - alpha)` ready for Nearest-Neighbor UI scaling.
pub fn extract_head_avatar_from_png(png_bytes: &[u8]) -> MqResult<SkinHeadAvatar> {
    let decoded = image::load_from_memory_with_format(png_bytes, ImageFormat::Png)
        .map_err(|err| MqError::Metadata(format!("failed to decode skin PNG: {err}")))?;

    let rgba_img = decoded.to_rgba8();
    let (width, height) = rgba_img.dimensions();
    if width != 64 || (height != 64 && height != 32) {
        return Err(MqError::Metadata(format!(
            "unsupported skin PNG dimensions {width}x{height} (expected 64x64 or 64x32)"
        )));
    }

    let mut rgba = Vec::with_capacity(SkinHeadAvatar::BYTE_LEN);

    for dy in 0..HEAD_SIZE {
        for dx in 0..HEAD_SIZE {
            let base = rgba_img.get_pixel(FACE_X + dx, FACE_Y + dy).0;
            let hat = rgba_img.get_pixel(HAT_X + dx, HAT_Y + dy).0;
            let blended = blend_hat_over_face(base, hat);
            rgba.extend_from_slice(&blended);
        }
    }

    Ok(SkinHeadAvatar {
        width: HEAD_SIZE,
        height: HEAD_SIZE,
        rgba,
    })
}

/// Alpha-blends a single hat pixel over a base face pixel (`out = hat * alpha + base * (1 - alpha)`).
#[inline]
fn blend_hat_over_face(base: [u8; 4], hat: [u8; 4]) -> [u8; 4] {
    let alpha = u32::from(hat[3]);
    if alpha == 0 {
        return base;
    }
    if alpha == 255 {
        return hat;
    }

    let inv_alpha = 255 - alpha;
    let blend_channel = |fg: u8, bg: u8| -> u8 {
        ((u32::from(fg) * alpha + u32::from(bg) * inv_alpha + 127) / 255) as u8
    };

    let out_r = blend_channel(hat[0], base[0]);
    let out_g = blend_channel(hat[1], base[1]);
    let out_b = blend_channel(hat[2], base[2]);
    let out_a = ((alpha * 255 + u32::from(base[3]) * inv_alpha + 127) / 255) as u8;

    [out_r, out_g, out_b, out_a]
}

/// Generates a deterministic, horizontally symmetric `8x8` pixel avatar from the MD5 of `username`
/// using an emerald/cobalt/slate palette.
pub fn generate_offline_geometric_avatar(username: &str) -> SkinHeadAvatar {
    let digest: [u8; 16] = Md5::digest(username.as_bytes()).into();

    let bg = SLATE_BACKGROUNDS[(digest[0] as usize) % SLATE_BACKGROUNDS.len()];
    let primary = PRIMARY_PALETTE[(digest[1] as usize) % PRIMARY_PALETTE.len()];
    let accent = ACCENT_PALETTE[(digest[2] as usize) % ACCENT_PALETTE.len()];

    let mut grid = [[bg; 8]; 8];
    let mut lit_half_cells = 0usize;

    for (y, row) in grid.iter_mut().enumerate() {
        // Pull 8 bits per row from the remaining MD5 bytes so all 8 rows have rich entropy.
        let row_byte = digest[3 + (y % 12)];
        let style_byte = digest[15 - (y % 12)];

        for x in 0..4usize {
            // Keep outer corner pixels as background slate for a clean rounded-frame silhouette.
            if (x == 0) && (y == 0 || y == 7) {
                continue;
            }

            let bit_on = ((row_byte >> (x * 2)) & 0x01) == 1
                || (((row_byte >> (x * 2 + 1)) & 0x01) == 1 && (x >= 1 && (1..=6).contains(&y)));

            if bit_on {
                let use_accent = ((style_byte >> x) & 0x03) == 0;
                let color = if use_accent { accent } else { primary };
                row[x] = color;
                row[7 - x] = color;
                lit_half_cells += 1;
            }
        }
    }

    // Guarantee a crisp visible center motif even for rare low-density MD5 bit patterns.
    if lit_half_cells < 6 {
        grid[2][2] = primary;
        grid[2][5] = primary;
        grid[3][1] = accent;
        grid[3][6] = accent;
        grid[5][2] = primary;
        grid[5][3] = primary;
        grid[5][4] = primary;
        grid[5][5] = primary;
    }

    let mut rgba = Vec::with_capacity(SkinHeadAvatar::BYTE_LEN);
    for row in &grid {
        for px in row {
            rgba.extend_from_slice(px);
        }
    }

    SkinHeadAvatar {
        width: HEAD_SIZE,
        height: HEAD_SIZE,
        rgba,
    }
}

/// Fetches `{base}/{username}.png` from Ely.by's skin system (or a custom override base URL)
/// and decodes the `8x8` head avatar, falling back to [`generate_offline_geometric_avatar`] on `404`.
pub async fn fetch_elyby_skin_avatar(
    client: &reqwest::Client,
    skins_base_url: Option<&str>,
    username: &str,
) -> MqResult<SkinHeadAvatar> {
    let base = skins_base_url
        .unwrap_or(ELYBY_SKINS_BASE_URL)
        .trim_end_matches('/');
    let url = format!("{base}/{username}.png");

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|err| MqError::Network(format!("skin request failed for {url}: {err}")))?;

    let status = resp.status();
    if status == StatusCode::NOT_FOUND {
        return Ok(generate_offline_geometric_avatar(username));
    }

    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(MqError::HttpStatus {
            status: status.as_u16(),
            url,
            body,
        });
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|err| MqError::Network(format!("failed reading skin body from {url}: {err}")))?;

    extract_head_avatar_from_png(&bytes)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::{Rgba, RgbaImage};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    fn encode_png(img: &RgbaImage) -> Vec<u8> {
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
            .expect("encode synthetic PNG");
        buf
    }

    #[test]
    fn extract_head_avatar_composites_64x64_face_and_semi_transparent_hat() {
        let mut img = RgbaImage::from_pixel(64, 64, Rgba([0, 0, 0, 0]));

        // Fill base 8x8 face at (8..16, 8..16) with solid cobalt [40, 100, 200, 255].
        for y in 8..16 {
            for x in 8..16 {
                img.put_pixel(x, y, Rgba([40, 100, 200, 255]));
            }
        }

        // Top row of hat (40..48, y=8): fully opaque white [255, 255, 255, 255].
        for x in 40..48 {
            img.put_pixel(x, 8, Rgba([255, 255, 255, 255]));
        }

        // Second row of hat (40..48, y=9): 50%-ish semi-transparent red [200, 20, 40, 128].
        for x in 40..48 {
            img.put_pixel(x, 9, Rgba([200, 20, 40, 128]));
        }

        // Remaining rows (y in 10..16): transparent hat [0, 0, 0, 0] -> base face untouched.
        let png_bytes = encode_png(&img);
        let avatar = extract_head_avatar_from_png(&png_bytes).expect("extract 64x64 head");

        assert_eq!(avatar.width, 8);
        assert_eq!(avatar.height, 8);
        assert_eq!(avatar.rgba.len(), 256);

        // Row 0 (dx=0, dy=0): opaque hat completely replaces base.
        assert_eq!(&avatar.rgba[0..4], &[255, 255, 255, 255]);

        // Row 1 (dx=0, dy=1): alpha-blended (alpha = 128/255):
        // R = (200*128 + 40*127 + 127) / 255 = (25600 + 5080 + 127) / 255 = 120
        // G = (20*128 + 100*127 + 127) / 255 = (2560 + 12700 + 127) / 255 = 60
        // B = (40*128 + 200*127 + 127) / 255 = (5120 + 25400 + 127) / 255 = 120
        let row1_px0 = &avatar.rgba[8 * 4..8 * 4 + 4];
        assert_eq!(row1_px0, &[120, 60, 120, 255]);

        // Row 2 (dx=0, dy=2): transparent hat leaves base face untouched.
        let row2_px0 = &avatar.rgba[16 * 4..16 * 4 + 4];
        assert_eq!(row2_px0, &[40, 100, 200, 255]);
    }

    #[test]
    fn extract_head_avatar_supports_64x32_and_rejects_invalid_dimensions() {
        let mut legacy = RgbaImage::from_pixel(64, 32, Rgba([0, 0, 0, 0]));
        for y in 8..16 {
            for x in 8..16 {
                legacy.put_pixel(x, y, Rgba([16, 185, 129, 255]));
            }
        }
        let avatar =
            extract_head_avatar_from_png(&encode_png(&legacy)).expect("extract 64x32 legacy skin");
        assert_eq!(avatar.rgba.len(), 256);
        assert_eq!(&avatar.rgba[0..4], &[16, 185, 129, 255]);

        let invalid = RgbaImage::from_pixel(32, 32, Rgba([0, 0, 0, 255]));
        let err = extract_head_avatar_from_png(&encode_png(&invalid)).unwrap_err();
        assert!(matches!(err, MqError::Metadata(_)));
    }

    #[test]
    fn offline_geometric_avatar_is_deterministic_opaque_and_horizontally_symmetric() {
        let a1 = generate_offline_geometric_avatar("Notch");
        let a2 = generate_offline_geometric_avatar("Notch");
        let other = generate_offline_geometric_avatar("Mr_Quster");

        assert_eq!(a1, a2);
        assert_ne!(a1, other);
        assert_eq!(a1.width, 8);
        assert_eq!(a1.height, 8);
        assert_eq!(a1.rgba.len(), 256);

        for y in 0..8usize {
            for x in 0..4usize {
                let left_idx = (y * 8 + x) * 4;
                let right_idx = (y * 8 + (7 - x)) * 4;
                assert_eq!(
                    &a1.rgba[left_idx..left_idx + 4],
                    &a1.rgba[right_idx..right_idx + 4],
                    "avatar must be symmetric at row {y}, col {x}"
                );
                assert_eq!(a1.rgba[left_idx + 3], 255, "pixels must be fully opaque");
            }
        }
    }

    #[tokio::test]
    async fn fetch_elyby_skin_avatar_decodes_png_and_falls_back_on_404() {
        let mut skin_img = RgbaImage::from_pixel(64, 64, Rgba([0, 0, 0, 0]));
        for y in 8..16 {
            for x in 8..16 {
                skin_img.put_pixel(x, y, Rgba([12, 140, 220, 255]));
            }
        }
        let png_payload = encode_png(&skin_img);

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind local test listener");
        let addr = listener.local_addr().expect("listener addr");

        let server_png = png_payload.clone();
        let server_task = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().await.expect("accept request");
                let mut req_buf = [0u8; 1024];
                let n = stream.read(&mut req_buf).await.expect("read request");
                let req_str = String::from_utf8_lossy(&req_buf[..n]);

                if req_str.starts_with("GET /skins/Steve.png ") {
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        server_png.len()
                    );
                    stream.write_all(header.as_bytes()).await.unwrap();
                    stream.write_all(&server_png).await.unwrap();
                } else {
                    let resp =
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                    stream.write_all(resp).await.unwrap();
                }
            }
        });

        let client = reqwest::Client::new();
        let base_url = format!("http://{addr}/skins");

        let steve_avatar = fetch_elyby_skin_avatar(&client, Some(&base_url), "Steve")
            .await
            .expect("fetch 200 skin");
        assert_eq!(&steve_avatar.rgba[0..4], &[12, 140, 220, 255]);

        let missing_avatar = fetch_elyby_skin_avatar(&client, Some(&base_url), "NoSuchSkin")
            .await
            .expect("fallback on 404 skin");
        assert_eq!(
            missing_avatar,
            generate_offline_geometric_avatar("NoSuchSkin")
        );

        server_task.await.expect("server task clean exit");
    }
}
