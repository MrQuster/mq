//! Deterministic, low-memory synthetic payload, JAR/ZIP, PNG, and malicious archive generators.

use std::io::{Cursor, Write};

use bytes::Bytes;
use image::{ImageFormat, Rgba, RgbaImage};
use serde_json::json;
use sha1::{Digest as _, Sha1};
use sha2::{Sha256, Sha512};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

/// Domain discriminator mixed into the PRNG seed so payloads across categories never collide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u64)]
pub enum PayloadKind {
    Asset = 1,
    LibraryClass = 2,
    NativeSharedObject = 3,
    ClientClass = 4,
    ModClass = 5,
    AuthlibClass = 6,
}

/// Generates a deterministic, high-entropy byte payload of exact `size` on the fly.
///
/// Uses SplitMix64 seeded from `(kind, index, size)` so:
/// - DEFLATE compression cannot trivially collapse the buffer,
/// - SHA-1 / SHA-256 / SHA-512 hashing performs genuine CPU work,
/// - Generating an 8 KiB..32 KiB buffer takes ~1 microsecond with zero persistent RAM cost.
pub fn payload_for_seed(kind: PayloadKind, index: usize, size: usize) -> Bytes {
    let mut buf = vec![0u8; size];
    fill_pseudo_random(&mut buf, kind as u64, index as u64, size as u64);
    Bytes::from(buf)
}

#[inline]
fn fill_pseudo_random(buf: &mut [u8], domain: u64, index: u64, extra: u64) {
    let mut state = 0x9E37_79B9_7F4A_7C15_u64
        ^ domain.wrapping_mul(0xBF58_476D_1CE4_E5B9)
        ^ index.wrapping_mul(0x94D0_49BB_1331_11EB)
        ^ extra.rotate_left(17);

    let (chunks, rem) = buf.as_chunks_mut::<8>();
    for chunk in chunks {
        state = splitmix64(state);
        *chunk = state.to_le_bytes();
    }
    if !rem.is_empty() {
        state = splitmix64(state);
        let tail = state.to_le_bytes();
        rem.copy_from_slice(&tail[..rem.len()]);
    }
}

#[inline]
fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Computes raw 20-byte SHA-1 digest.
pub fn sha1_bytes(data: &[u8]) -> [u8; 20] {
    let mut hasher = Sha1::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// Formats a 20-byte SHA-1 digest as lowercase 40-char hex.
pub fn sha1_hex(digest: &[u8; 20]) -> String {
    hex::encode(digest)
}

/// Computes raw 32-byte SHA-256 digest.
pub fn sha256_bytes(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// Formats a 32-byte SHA-256 digest as lowercase 64-char hex.
pub fn sha256_hex(digest: &[u8; 32]) -> String {
    hex::encode(digest)
}

/// Computes raw 64-byte SHA-512 digest.
pub fn sha512_bytes(data: &[u8]) -> [u8; 64] {
    let mut hasher = Sha512::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// Formats a 64-byte SHA-512 digest as lowercase 128-char hex.
pub fn sha512_hex(digest: &[u8; 64]) -> String {
    hex::encode(digest)
}

/// Metadata descriptor format embedded inside a synthetic mod `.jar`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModMetadataFormat {
    /// Writes `fabric.mod.json` at the root of the JAR.
    Fabric,
    /// Writes `quilt.mod.json` at the root of the JAR.
    Quilt,
    /// Writes `META-INF/mods.toml` for Forge / NeoForge TOML scanners.
    ForgeModsToml,
}

/// Specification for deterministically generating a valid synthetic mod `.jar`.
#[derive(Debug, Clone)]
pub struct SyntheticModSpec {
    pub id: String,
    pub name: String,
    pub version: String,
    pub authors: Vec<String>,
    pub depends: Vec<(String, String)>,
    pub format: ModMetadataFormat,
    pub seed_index: usize,
    pub class_payload_size: usize,
}

impl SyntheticModSpec {
    /// Creates a standard indexed mod spec (`16 KiB`..`32 KiB` binary class payload).
    pub fn indexed(index: usize) -> Self {
        let format = match index % 20 {
            9 => ModMetadataFormat::Quilt,
            19 => ModMetadataFormat::ForgeModsToml,
            _ => ModMetadataFormat::Fabric,
        };
        let class_payload_size = 16 * 1024 + (index % 17) * 1024;
        Self {
            id: format!("mq_mod_{index}"),
            name: format!("MQ Synthetic Mod #{index}"),
            version: format!("1.{}.0", index % 50),
            authors: vec!["MQBench".to_string()],
            depends: vec![
                ("minecraft".to_string(), "1.21.1".to_string()),
                ("fabricloader".to_string(), ">=0.16.0".to_string()),
            ],
            format,
            seed_index: index,
            class_payload_size,
        }
    }
}

fn fixed_stored_options() -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(DateTime::default())
}

fn fixed_deflated_options() -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default())
}

/// Builds a deterministic, valid mod `.jar` archive containing mod metadata and a binary `.class`
/// payload (`16 KiB`–`32 KiB` by default) so parallel `.jar` scanning and SHA-1/SHA-512 hashing do
/// genuine multi-core work.
pub fn build_synthetic_mod_jar(spec: &SyntheticModSpec) -> Vec<u8> {
    let mut out = Vec::with_capacity(spec.class_payload_size + 2048);
    {
        let mut zip = ZipWriter::new(Cursor::new(&mut out));
        let manifest = format!(
            "Manifest-Version: 1.0\r\nImplementation-Title: {}\r\nImplementation-Version: {}\r\n\r\n",
            spec.name, spec.version
        );
        zip.start_file("META-INF/MANIFEST.MF", fixed_deflated_options())
            .expect("write MANIFEST.MF");
        zip.write_all(manifest.as_bytes()).expect("manifest bytes");

        match spec.format {
            ModMetadataFormat::Fabric => {
                let mut depends_map = serde_json::Map::new();
                for (k, v) in &spec.depends {
                    depends_map.insert(k.clone(), json!(v));
                }
                let meta = json!({
                    "schemaVersion": 1,
                    "id": spec.id,
                    "version": spec.version,
                    "name": spec.name,
                    "description": format!("Deterministic synthetic Fabric mod {}", spec.id),
                    "authors": spec.authors,
                    "license": "MIT",
                    "environment": "*",
                    "entrypoints": {
                        "main": [format!("org.mqmock.{}.ModInit", sanitize_ident(&spec.id))]
                    },
                    "depends": depends_map,
                });
                let meta_bytes =
                    serde_json::to_vec_pretty(&meta).expect("serialize fabric.mod.json");
                zip.start_file("fabric.mod.json", fixed_deflated_options())
                    .expect("start fabric.mod.json");
                zip.write_all(&meta_bytes).expect("write fabric.mod.json");
            }
            ModMetadataFormat::Quilt => {
                let depends: Vec<serde_json::Value> = spec
                    .depends
                    .iter()
                    .map(|(id, versions)| json!({ "id": id, "versions": versions }))
                    .collect();
                let meta = json!({
                    "schema_version": 1,
                    "quilt_loader": {
                        "group": "org.mqmock",
                        "id": spec.id,
                        "version": spec.version,
                        "metadata": {
                            "name": spec.name,
                            "description": format!("Deterministic synthetic Quilt mod {}", spec.id),
                            "contributors": {
                                spec.authors.first().cloned().unwrap_or_else(|| "MQBench".into()): "Owner"
                            },
                            "license": "MIT"
                        },
                        "intermediate_mappings": "net.fabricmc:intermediary",
                        "depends": depends,
                    }
                });
                let meta_bytes =
                    serde_json::to_vec_pretty(&meta).expect("serialize quilt.mod.json");
                zip.start_file("quilt.mod.json", fixed_deflated_options())
                    .expect("start quilt.mod.json");
                zip.write_all(&meta_bytes).expect("write quilt.mod.json");
            }
            ModMetadataFormat::ForgeModsToml => {
                let authors_joined = spec.authors.join(", ");
                let mut toml_text = format!(
                    "modLoader=\"javafml\"\nloaderVersion=\"[47,)\"\nlicense=\"MIT\"\n\n[[mods]]\nmodId=\"{}\"\nversion=\"{}\"\ndisplayName=\"{}\"\nauthors=\"{}\"\ndescription='''Deterministic synthetic mod {}'''\n",
                    spec.id, spec.version, spec.name, authors_joined, spec.id
                );
                for (dep_id, version_range) in &spec.depends {
                    toml_text.push_str(&format!(
                        "\n[[dependencies.{}]]\nmodId=\"{}\"\nmandatory=true\nversionRange=\"{}\"\nordering=\"NONE\"\nside=\"BOTH\"\n",
                        spec.id, dep_id, version_range
                    ));
                }
                zip.start_file("META-INF/mods.toml", fixed_deflated_options())
                    .expect("start META-INF/mods.toml");
                zip.write_all(toml_text.as_bytes())
                    .expect("write META-INF/mods.toml");
            }
        }

        let class_size = spec.class_payload_size.max(16);
        let mut class_bytes = vec![0u8; class_size];
        // Valid Java class header magic (0xCAFEBABE, major version 65 = Java 21).
        class_bytes[..8].copy_from_slice(&[0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x41]);
        fill_pseudo_random(
            &mut class_bytes[8..],
            PayloadKind::ModClass as u64,
            spec.seed_index as u64,
            class_size as u64,
        );

        let class_entry = format!("org/mqmock/{}/ModInit.class", sanitize_ident(&spec.id));
        zip.start_file(class_entry, fixed_stored_options())
            .expect("start ModInit.class");
        zip.write_all(&class_bytes).expect("write ModInit.class");
        zip.finish().expect("finish mod jar");
    }
    out
}

fn sanitize_ident(raw: &str) -> String {
    raw.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// Builds a deterministic synthetic Java library `.jar` for library index `index`.
pub fn build_library_jar(index: usize, version: &str) -> Bytes {
    let payload_len = 8 * 1024 + (index % 9) * 1024;
    let mut out = Vec::with_capacity(payload_len + 512);
    {
        let mut zip = ZipWriter::new(Cursor::new(&mut out));
        let manifest = format!(
            "Manifest-Version: 1.0\r\nImplementation-Title: org.mqmock.lib-{index}\r\nImplementation-Version: {version}\r\n\r\n"
        );
        zip.start_file("META-INF/MANIFEST.MF", fixed_deflated_options())
            .expect("start MANIFEST.MF");
        zip.write_all(manifest.as_bytes())
            .expect("write MANIFEST.MF");

        let mut class_bytes = vec![0u8; payload_len];
        class_bytes[..8].copy_from_slice(&[0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x3D]);
        fill_pseudo_random(
            &mut class_bytes[8..],
            PayloadKind::LibraryClass as u64,
            index as u64,
            payload_len as u64,
        );

        let entry_path = format!("org/mqmock/lib_{index}/Lib{index}.class");
        zip.start_file(entry_path, fixed_stored_options())
            .expect("start library class");
        zip.write_all(&class_bytes).expect("write library class");
        zip.finish().expect("finish library jar");
    }
    Bytes::from(out)
}

/// Builds a deterministic Linux native `.jar` containing `libmocknative.so` and `META-INF/MANIFEST.MF`.
pub fn build_native_linux_jar(index: usize) -> Bytes {
    let so_len = 12 * 1024;
    let mut out = Vec::with_capacity(so_len + 1024);
    {
        let mut zip = ZipWriter::new(Cursor::new(&mut out));
        zip.start_file("META-INF/MANIFEST.MF", fixed_deflated_options())
            .expect("start MANIFEST.MF");
        zip.write_all(b"Manifest-Version: 1.0\r\nCreated-By: mq-testsupport\r\n\r\n")
            .expect("write MANIFEST.MF");

        let mut so_bytes = vec![0u8; so_len];
        // ELF64 little-endian shared object header prefix.
        so_bytes[..8].copy_from_slice(&[0x7F, b'E', b'L', b'F', 0x02, 0x01, 0x01, 0x00]);
        fill_pseudo_random(
            &mut so_bytes[8..],
            PayloadKind::NativeSharedObject as u64,
            index as u64,
            so_len as u64,
        );
        zip.start_file("libmocknative.so", fixed_stored_options())
            .expect("start libmocknative.so");
        zip.write_all(&so_bytes).expect("write libmocknative.so");
        zip.finish().expect("finish native linux jar");
    }
    Bytes::from(out)
}

/// Builds a deterministic loader or Maven override `.jar` identified by Maven coordinate.
pub fn build_named_maven_jar(coord: &str, seed: usize) -> Bytes {
    let payload_len = 10 * 1024 + (seed % 5) * 1024;
    let mut out = Vec::with_capacity(payload_len + 512);
    {
        let mut zip = ZipWriter::new(Cursor::new(&mut out));
        let manifest = format!("Manifest-Version: 1.0\r\nMaven-Coordinate: {coord}\r\n\r\n");
        zip.start_file("META-INF/MANIFEST.MF", fixed_deflated_options())
            .expect("start MANIFEST.MF");
        zip.write_all(manifest.as_bytes())
            .expect("write MANIFEST.MF");

        let mut class_bytes = vec![0u8; payload_len];
        class_bytes[..8].copy_from_slice(&[0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x41]);
        fill_pseudo_random(
            &mut class_bytes[8..],
            PayloadKind::LibraryClass as u64,
            (seed as u64).wrapping_add(1_000_000),
            coord.len() as u64,
        );
        let entry = format!("org/mqmock/maven/{}/Artifact.class", sanitize_ident(coord));
        zip.start_file(entry, fixed_stored_options())
            .expect("start maven class");
        zip.write_all(&class_bytes).expect("write maven class");
        zip.finish().expect("finish maven jar");
    }
    Bytes::from(out)
}

/// Builds a deterministic Minecraft `client.jar` for `mc_version`.
pub fn build_client_jar(mc_version: &str) -> Bytes {
    let seed: usize = mc_version.bytes().fold(0usize, |acc, b| {
        acc.wrapping_mul(131).wrapping_add(b as usize)
    });
    let payload_len = 48 * 1024;
    let mut out = Vec::with_capacity(payload_len + 1024);
    {
        let mut zip = ZipWriter::new(Cursor::new(&mut out));
        let manifest = format!(
            "Manifest-Version: 1.0\r\nMain-Class: net.minecraft.client.main.Main\r\nImplementation-Version: {mc_version}\r\n\r\n"
        );
        zip.start_file("META-INF/MANIFEST.MF", fixed_deflated_options())
            .expect("start MANIFEST.MF");
        zip.write_all(manifest.as_bytes())
            .expect("write MANIFEST.MF");

        let mut class_bytes = vec![0u8; payload_len];
        class_bytes[..8].copy_from_slice(&[0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x41]);
        fill_pseudo_random(
            &mut class_bytes[8..],
            PayloadKind::ClientClass as u64,
            seed as u64,
            payload_len as u64,
        );
        zip.start_file(
            "net/minecraft/client/main/Main.class",
            fixed_stored_options(),
        )
        .expect("start Main.class");
        zip.write_all(&class_bytes).expect("write Main.class");
        zip.finish().expect("finish client jar");
    }
    Bytes::from(out)
}

/// Builds a deterministic `authlib-injector.jar` containing valid Java agent manifest headers.
pub fn build_authlib_injector_jar() -> Bytes {
    let payload_len = 16 * 1024;
    let mut out = Vec::with_capacity(payload_len + 1024);
    {
        let mut zip = ZipWriter::new(Cursor::new(&mut out));
        let manifest = concat!(
            "Manifest-Version: 1.0\r\n",
            "Premain-Class: moe.yushi.authlibinjector.Premain\r\n",
            "Agent-Class: moe.yushi.authlibinjector.Premain\r\n",
            "Can-Redefine-Classes: true\r\n",
            "Can-Retransform-Classes: true\r\n",
            "Implementation-Version: 1.2.5\r\n",
            "Build-Number: 53\r\n\r\n"
        );
        zip.start_file("META-INF/MANIFEST.MF", fixed_deflated_options())
            .expect("start MANIFEST.MF");
        zip.write_all(manifest.as_bytes())
            .expect("write MANIFEST.MF");

        let mut class_bytes = vec![0u8; payload_len];
        class_bytes[..8].copy_from_slice(&[0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34]);
        fill_pseudo_random(
            &mut class_bytes[8..],
            PayloadKind::AuthlibClass as u64,
            53,
            payload_len as u64,
        );
        zip.start_file(
            "moe/yushi/authlibinjector/Premain.class",
            fixed_stored_options(),
        )
        .expect("start Premain.class");
        zip.write_all(&class_bytes).expect("write Premain.class");
        zip.finish().expect("finish authlib-injector.jar");
    }
    Bytes::from(out)
}

/// Generates a deterministic 64x64 RGBA Minecraft skin PNG with distinct colorful face
/// `(8..16, 8..16)` and hat overlay `(40..48, 8..16)` regions.
pub fn build_skin_png_bytes(username: &str) -> Vec<u8> {
    let hash = sha256_bytes(username.as_bytes());
    let base_r = 60u8.saturating_add(hash[0] % 160);
    let base_g = 60u8.saturating_add(hash[1] % 160);
    let base_b = 60u8.saturating_add(hash[2] % 160);

    let hat_r = 255u8.wrapping_sub(base_r / 2);
    let hat_g = 40u8.saturating_add(hash[3] % 180);
    let hat_b = 200u8.saturating_sub(hash[4] % 120);

    let mut img = RgbaImage::from_pixel(64, 64, Rgba([0, 0, 0, 0]));

    // Face region: x in 8..16, y in 8..16 (fully opaque, colorful, with distinct eyes).
    for y in 8..16 {
        for x in 8..16 {
            let is_eye = y == 12 && (x == 9 || x == 10 || x == 13 || x == 14);
            let px = if is_eye {
                Rgba([24, 32, 220, 255])
            } else {
                Rgba([
                    base_r.wrapping_add(((x - 8) * 7) as u8),
                    base_g.wrapping_add(((y - 8) * 5) as u8),
                    base_b,
                    255,
                ])
            };
            img.put_pixel(x, y, px);
        }
    }

    // Hat / outer head layer: x in 40..48, y in 8..16 (distinct colorful pixels).
    for y in 8..16 {
        for x in 40..48 {
            let alpha = if y < 12 || x == 40 || x == 47 {
                255
            } else {
                160
            };
            let px = Rgba([
                hat_r,
                hat_g.wrapping_add(((x - 40) * 9) as u8),
                hat_b.wrapping_add(((y - 8) * 6) as u8),
                alpha,
            ]);
            img.put_pixel(x, y, px);
        }
    }

    // Torso & limbs UV regions so full-skin parsers see opaque body pixels too.
    for y in 20..32 {
        for x in 20..28 {
            img.put_pixel(x, y, Rgba([base_b, base_r, base_g, 255]));
        }
    }

    let mut png_bytes = Vec::with_capacity(2048);
    img.write_to(&mut Cursor::new(&mut png_bytes), ImageFormat::Png)
        .expect("encode 64x64 RGBA PNG");
    png_bytes
}

/// Builds a malicious Modrinth `.mrpack` containing `overrides/../../etc/passwd` to test
/// Zip-Slip path traversal defenses.
pub fn build_malicious_zip_slip_mrpack() -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut zip = ZipWriter::new(Cursor::new(&mut out));
        let index = json!({
            "formatVersion": 1,
            "game": "minecraft",
            "versionId": "1.0.0-evil",
            "name": "Malicious Zip-Slip Pack",
            "dependencies": {
                "minecraft": "1.21.1"
            },
            "files": []
        });
        zip.start_file("modrinth.index.json", fixed_deflated_options())
            .expect("start modrinth.index.json");
        zip.write_all(serde_json::to_string_pretty(&index).unwrap().as_bytes())
            .expect("write modrinth.index.json");

        zip.start_file("overrides/../../etc/passwd", fixed_stored_options())
            .expect("start malicious traversal entry");
        zip.write_all(b"root:x:0:0:malicious:/root:/bin/sh\n")
            .expect("write malicious traversal entry");
        zip.finish().expect("finish malicious mrpack");
    }
    out
}

/// Builds a `.mrpack` where `modrinth.index.json` declares an intentionally mismatched SHA-512
/// hash for `mods/mod-0.jar` to test hash verification rejection and rollback.
pub fn build_corrupted_hash_mrpack(base_url: &str) -> Vec<u8> {
    let spec = SyntheticModSpec::indexed(0);
    let mod_bytes = build_synthetic_mod_jar(&spec);
    let good_sha1 = sha1_hex(&sha1_bytes(&mod_bytes));
    let bad_sha512 = "f".repeat(128);

    let mut out = Vec::new();
    {
        let mut zip = ZipWriter::new(Cursor::new(&mut out));
        let index = json!({
            "formatVersion": 1,
            "game": "minecraft",
            "versionId": "1.0.0-corrupt",
            "name": "Corrupted Hash Pack",
            "dependencies": {
                "minecraft": "1.21.1",
                "fabric-loader": "0.16.9"
            },
            "files": [
                {
                    "path": "mods/mod-0.jar",
                    "hashes": {
                        "sha1": good_sha1,
                        "sha512": bad_sha512
                    },
                    "env": {
                        "client": "required",
                        "server": "optional"
                    },
                    "downloads": [
                        format!("{base_url}/download/mod/0.jar")
                    ],
                    "fileSize": mod_bytes.len()
                }
            ]
        });
        zip.start_file("modrinth.index.json", fixed_deflated_options())
            .expect("start modrinth.index.json");
        zip.write_all(serde_json::to_string_pretty(&index).unwrap().as_bytes())
            .expect("write modrinth.index.json");
        zip.finish().expect("finish corrupted hash mrpack");
    }
    out
}

/// Builds a compact `.mrpack` containing a highly compressible `overrides/bomb.bin` entry that
/// expands to `uncompressed_mib` MiB of zeros, testing streaming extraction quota guards.
pub fn build_zip_bomb_mrpack(uncompressed_mib: usize) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut zip = ZipWriter::new(Cursor::new(&mut out));
        let index = json!({
            "formatVersion": 1,
            "game": "minecraft",
            "versionId": "1.0.0-bomb",
            "name": "Zip Bomb Pack",
            "dependencies": {
                "minecraft": "1.21.1"
            },
            "files": []
        });
        zip.start_file("modrinth.index.json", fixed_deflated_options())
            .expect("start modrinth.index.json");
        zip.write_all(serde_json::to_string_pretty(&index).unwrap().as_bytes())
            .expect("write modrinth.index.json");

        zip.start_file("overrides/bomb.bin", fixed_deflated_options())
            .expect("start overrides/bomb.bin");
        let zero_chunk = [0u8; 64 * 1024];
        let chunks_per_mib = (1024 * 1024) / zero_chunk.len();
        for _ in 0..(uncompressed_mib * chunks_per_mib) {
            zip.write_all(&zero_chunk).expect("write zero chunk");
        }
        zip.finish().expect("finish zip bomb mrpack");
    }
    out
}
