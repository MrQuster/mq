//! Compact precomputed metadata and deterministic manifest/archive builders (`MockWorld`).

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::{Cursor, Write};

use bytes::Bytes;
use serde_json::json;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

use crate::payload::{
    build_authlib_injector_jar, build_client_jar, build_library_jar, build_named_maven_jar,
    build_native_linux_jar, build_synthetic_mod_jar, payload_for_seed, sha1_bytes, sha1_hex,
    sha256_bytes, sha256_hex, sha512_bytes, sha512_hex, ModMetadataFormat, PayloadKind,
    SyntheticModSpec,
};

/// Supported Minecraft versions emulated by the mock Mojang repository.
pub const SUPPORTED_MC_VERSIONS: [&str; 2] = ["1.21.1", "1.20.4"];
/// Default Fabric loader version served by the mock Fabric meta endpoints.
pub const DEFAULT_FABRIC_LOADER_VERSION: &str = "0.16.9";
/// Default Quilt loader version served by the mock Quilt meta endpoints.
pub const DEFAULT_QUILT_LOADER_VERSION: &str = "0.26.4";

/// Configuration controlling synthetic dataset scale and transient fault injection.
#[derive(Debug, Clone)]
pub struct MockConfig {
    /// Number of distinct synthetic asset objects in `asset_index.json` (default: `3200`).
    pub asset_count: usize,
    /// Number of vanilla libraries in `version.json` (default: `110`).
    pub library_count: usize,
    /// Number of required client mod `.jar` files in `mq-ultra-pack.mrpack` (default: `220`).
    pub modpack_mod_count: usize,
    /// Number of initial HTTP requests to fail with `503 Service Unavailable` (default: `0`).
    pub fail_first_n_requests: usize,
}

impl Default for MockConfig {
    fn default() -> Self {
        Self {
            asset_count: 3200,
            library_count: 110,
            modpack_mod_count: 220,
            fail_first_n_requests: 0,
        }
    }
}

impl MockConfig {
    /// Compact dataset configuration suitable for fast unit and integration tests.
    pub fn small() -> Self {
        Self {
            asset_count: 64,
            library_count: 12,
            modpack_mod_count: 16,
            fail_first_n_requests: 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AssetMeta {
    pub size: u32,
    pub sha1: [u8; 20],
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BlobSha1Meta {
    pub size: u64,
    pub sha1: [u8; 20],
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BlobDualHashMeta {
    pub size: u64,
    pub sha1: [u8; 20],
    pub sha512: [u8; 64],
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LibraryMeta {
    pub artifact: BlobSha1Meta,
    pub is_macos_only: bool,
    pub native_linux: Option<BlobSha1Meta>,
}

#[derive(Debug, Clone)]
pub(crate) struct CatalogArtifacts {
    pub sodium: BlobDualHashMeta,
    pub fabric_api: BlobDualHashMeta,
    pub lithium: BlobDualHashMeta,
    pub jei: BlobDualHashMeta,
    pub server_only_mod: BlobDualHashMeta,
    pub authlib_sha256: [u8; 32],
}

/// Deterministic, memory-lean repository state shared across `axum` handlers.
///
/// Stores only compact hashes and byte sizes (`< 2 MiB` even for 3,200+ assets and 220+ mods) and
/// synthesizes actual binary payloads on the fly per request.
#[derive(Debug)]
pub struct MockWorld {
    config: MockConfig,
    base_url: String,
    assets: Vec<AssetMeta>,
    assets_by_sha1: HashMap<[u8; 20], (u32, u32)>,
    asset_index_sha1: [u8; 20],
    asset_index_size: u64,
    asset_total_size: u64,
    client_1211: BlobSha1Meta,
    client_1204: BlobSha1Meta,
    libraries: Vec<LibraryMeta>,
    mods: Vec<BlobDualHashMeta>,
    pub(crate) catalog: CatalogArtifacts,
    pub(crate) mrpack_meta: BlobDualHashMeta,
    pub(crate) cf_zip_meta: BlobDualHashMeta,
}

impl MockWorld {
    /// Initializes compact metadata for the configured synthetic world bound to `base_url`.
    pub fn new(config: MockConfig, base_url: impl Into<String>) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_string();

        // 1. Precompute compact asset metadata without retaining asset payloads in memory.
        let mut assets = Vec::with_capacity(config.asset_count);
        let mut assets_by_sha1 = HashMap::with_capacity(config.asset_count);
        let mut asset_total_size = 0u64;
        for i in 0..config.asset_count {
            let size = asset_size_for_index(i);
            let bytes = payload_for_seed(PayloadKind::Asset, i, size);
            let sha1 = sha1_bytes(&bytes);
            assets.push(AssetMeta {
                size: size as u32,
                sha1,
            });
            assets_by_sha1.insert(sha1, (i as u32, size as u32));
            asset_total_size += size as u64;
        }

        let rendered_asset_index = render_asset_index_string(&assets);
        let asset_index_sha1 = sha1_bytes(rendered_asset_index.as_bytes());
        let asset_index_size = rendered_asset_index.len() as u64;
        drop(rendered_asset_index);

        // 2. Precompute client.jar metadata for 1.21.1 and 1.20.4.
        let c1211 = build_client_jar("1.21.1");
        let client_1211 = BlobSha1Meta {
            size: c1211.len() as u64,
            sha1: sha1_bytes(&c1211),
        };
        drop(c1211);

        let c1204 = build_client_jar("1.20.4");
        let client_1204 = BlobSha1Meta {
            size: c1204.len() as u64,
            sha1: sha1_bytes(&c1204),
        };
        drop(c1204);

        // 3. Precompute library metadata (including macOS-only rule library and Linux native jar).
        let native_index = match config.library_count {
            0 => None,
            1 | 2 => Some(0),
            _ => Some(2),
        };
        let macos_only_index = if config.library_count >= 2 {
            Some(1)
        } else {
            None
        };

        let mut libraries = Vec::with_capacity(config.library_count);
        for i in 0..config.library_count {
            let ver = format!("1.0.{i}");
            let jar = build_library_jar(i, &ver);
            let artifact = BlobSha1Meta {
                size: jar.len() as u64,
                sha1: sha1_bytes(&jar),
            };
            drop(jar);

            let native_linux = if native_index == Some(i) {
                let njar = build_native_linux_jar(i);
                let meta = BlobSha1Meta {
                    size: njar.len() as u64,
                    sha1: sha1_bytes(&njar),
                };
                drop(njar);
                Some(meta)
            } else {
                None
            };

            libraries.push(LibraryMeta {
                artifact,
                is_macos_only: macos_only_index == Some(i),
                native_linux,
            });
        }

        // 4. Precompute modpack mod metadata (`0..modpack_mod_count`).
        let mut mods = Vec::with_capacity(config.modpack_mod_count);
        for i in 0..config.modpack_mod_count {
            let spec = SyntheticModSpec::indexed(i);
            let jar = build_synthetic_mod_jar(&spec);
            mods.push(dual_hash_meta(&jar));
        }

        // 5. Precompute catalog & authlib-injector compact metadata.
        let sodium = dual_hash_meta(&build_synthetic_mod_jar(&sodium_spec()));
        let fabric_api = dual_hash_meta(&build_synthetic_mod_jar(&fabric_api_spec()));
        let lithium = dual_hash_meta(&build_synthetic_mod_jar(&lithium_spec()));
        let jei = dual_hash_meta(&build_synthetic_mod_jar(&jei_spec()));
        let server_only_mod = dual_hash_meta(&build_synthetic_mod_jar(&server_only_spec()));

        let authlib_jar = build_authlib_injector_jar();
        let authlib_sha256 = sha256_bytes(&authlib_jar);
        drop(authlib_jar);

        let catalog = CatalogArtifacts {
            sodium,
            fabric_api,
            lithium,
            jei,
            server_only_mod,
            authlib_sha256,
        };

        let mut world = Self {
            config,
            base_url,
            assets,
            assets_by_sha1,
            asset_index_sha1,
            asset_index_size,
            asset_total_size,
            client_1211,
            client_1204,
            libraries,
            mods,
            catalog,
            mrpack_meta: BlobDualHashMeta {
                size: 0,
                sha1: [0; 20],
                sha512: [0; 64],
            },
            cf_zip_meta: BlobDualHashMeta {
                size: 0,
                sha1: [0; 20],
                sha512: [0; 64],
            },
        };

        let mrpack_bytes = world.build_mrpack_bytes();
        world.mrpack_meta = dual_hash_meta(&mrpack_bytes);
        drop(mrpack_bytes);

        let cf_zip_bytes = world.build_curseforge_zip_bytes();
        world.cf_zip_meta = dual_hash_meta(&cf_zip_bytes);
        drop(cf_zip_bytes);

        world
    }

    /// Returns the active [`MockConfig`].
    pub fn config(&self) -> &MockConfig {
        &self.config
    }

    /// Returns the base URL embedded into generated manifests.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Returns the exact SHA-256 hex digest of the synthetic `authlib-injector.jar`.
    pub fn authlib_injector_sha256_hex(&self) -> String {
        sha256_hex(&self.catalog.authlib_sha256)
    }

    /// Renders Mojang `version_manifest_v2.json` pointing to `{base_url}/mc/meta/{ver}/version.json`.
    pub fn version_manifest_v2_json(&self) -> Bytes {
        let v1211 = self.version_json("1.21.1").expect("1.21.1 version.json");
        let v1204 = self.version_json("1.20.4").expect("1.20.4 version.json");

        let doc = json!({
            "latest": {
                "release": "1.21.1",
                "snapshot": "1.21.1"
            },
            "versions": [
                {
                    "id": "1.21.1",
                    "type": "release",
                    "url": format!("{}/mc/meta/1.21.1/version.json", self.base_url),
                    "time": "2024-08-08T12:24:05+00:00",
                    "releaseTime": "2024-08-08T12:24:05+00:00",
                    "sha1": sha1_hex(&sha1_bytes(&v1211)),
                    "complianceLevel": 1
                },
                {
                    "id": "1.20.4",
                    "type": "release",
                    "url": format!("{}/mc/meta/1.20.4/version.json", self.base_url),
                    "time": "2023-12-07T12:56:20+00:00",
                    "releaseTime": "2023-12-07T12:56:20+00:00",
                    "sha1": sha1_hex(&sha1_bytes(&v1204)),
                    "complianceLevel": 1
                }
            ]
        });
        Bytes::from(serde_json::to_vec_pretty(&doc).expect("serialize version_manifest_v2.json"))
    }

    /// Renders Mojang `version.json` for `"1.21.1"` or `"1.20.4"`.
    pub fn version_json(&self, mc_ver: &str) -> Option<Bytes> {
        let (java_major, java_component, client_meta, release_time) = match mc_ver {
            "1.21.1" => (
                21,
                "java-runtime-delta",
                self.client_1211,
                "2024-08-08T12:24:05+00:00",
            ),
            "1.20.4" => (
                17,
                "java-runtime-gamma",
                self.client_1204,
                "2023-12-07T12:56:20+00:00",
            ),
            _ => return None,
        };

        let mut libs_json = Vec::with_capacity(self.libraries.len());
        for (i, lib) in self.libraries.iter().enumerate() {
            let ver = format!("1.0.{i}");
            let name = format!("org.mqmock:lib-{i}:{ver}");
            let rel_path = format!("org/mqmock/lib-{i}/{ver}/lib-{i}-{ver}.jar");
            let url = format!("{}/download/libraries/{rel_path}", self.base_url);

            let mut downloads = serde_json::Map::new();
            downloads.insert(
                "artifact".to_string(),
                json!({
                    "path": rel_path,
                    "sha1": sha1_hex(&lib.artifact.sha1),
                    "size": lib.artifact.size,
                    "url": url,
                }),
            );

            let mut entry = serde_json::Map::new();
            entry.insert("name".to_string(), json!(name));

            if let Some(native_meta) = lib.native_linux {
                let native_rel =
                    format!("org/mqmock/lib-{i}/{ver}/lib-{i}-{ver}-natives-linux.jar");
                let native_url = format!("{}/download/libraries/{native_rel}", self.base_url);
                downloads.insert(
                    "classifiers".to_string(),
                    json!({
                        "natives-linux": {
                            "path": native_rel,
                            "sha1": sha1_hex(&native_meta.sha1),
                            "size": native_meta.size,
                            "url": native_url,
                        }
                    }),
                );
                entry.insert(
                    "natives".to_string(),
                    json!({
                        "linux": "natives-linux"
                    }),
                );
                entry.insert(
                    "extract".to_string(),
                    json!({
                        "exclude": ["META-INF/"]
                    }),
                );
                entry.insert(
                    "rules".to_string(),
                    json!([
                        {
                            "action": "allow",
                            "os": { "name": "linux" }
                        }
                    ]),
                );
            } else if lib.is_macos_only {
                entry.insert(
                    "rules".to_string(),
                    json!([
                        {
                            "action": "allow",
                            "os": { "name": "osx" }
                        }
                    ]),
                );
            }

            entry.insert(
                "downloads".to_string(),
                serde_json::Value::Object(downloads),
            );
            libs_json.push(serde_json::Value::Object(entry));
        }

        let doc = json!({
            "id": mc_ver,
            "type": "release",
            "mainClass": "net.minecraft.client.main.Main",
            "minimumLauncherVersion": 21,
            "time": release_time,
            "releaseTime": release_time,
            "complianceLevel": 1,
            "javaVersion": {
                "component": java_component,
                "majorVersion": java_major
            },
            "arguments": {
                "jvm": [
                    {
                        "rules": [
                            { "action": "allow", "os": { "name": "osx" } }
                        ],
                        "value": ["-XstartOnFirstThread"]
                    },
                    "-Djava.library.path=${natives_directory}",
                    "-Djna.tmpdir=${natives_directory}",
                    "-Dminecraft.launcher.brand=${launcher_name}",
                    "-Dminecraft.launcher.version=${launcher_version}",
                    "-cp",
                    "${classpath}"
                ],
                "game": [
                    "--username",
                    "${auth_player_name}",
                    "--version",
                    "${version_name}",
                    "--gameDir",
                    "${game_directory}",
                    "--assetsDir",
                    "${assets_root}",
                    "--assetIndex",
                    "${assets_index_name}",
                    "--uuid",
                    "${auth_uuid}",
                    "--accessToken",
                    "${auth_access_token}",
                    "--userType",
                    "${user_type}",
                    "--versionType",
                    "${version_type}",
                    {
                        "rules": [
                            { "action": "allow", "features": { "is_demo_user": true } }
                        ],
                        "value": "--demo"
                    }
                ]
            },
            "assetIndex": {
                "id": mc_ver,
                "sha1": sha1_hex(&self.asset_index_sha1),
                "size": self.asset_index_size,
                "totalSize": self.asset_total_size,
                "url": format!("{}/mc/assets/indexes/{mc_ver}.json", self.base_url)
            },
            "downloads": {
                "client": {
                    "sha1": sha1_hex(&client_meta.sha1),
                    "size": client_meta.size,
                    "url": format!("{}/download/client/{mc_ver}/client.jar", self.base_url)
                }
            },
            "libraries": libs_json
        });

        Some(Bytes::from(
            serde_json::to_vec_pretty(&doc).expect("serialize version.json"),
        ))
    }

    /// Renders `asset_index.json` containing `config.asset_count` distinct synthetic sound objects.
    pub fn asset_index_json(&self, mc_ver: &str) -> Option<Bytes> {
        if !SUPPORTED_MC_VERSIONS.contains(&mc_ver) {
            return None;
        }
        Some(Bytes::from(
            render_asset_index_string(&self.assets).into_bytes(),
        ))
    }

    /// Generates an asset payload on the fly by verifying `(prefix, sha1_hex)` in O(1).
    pub fn asset_payload_by_hash(&self, prefix: &str, hash_hex: &str) -> Option<Bytes> {
        if prefix.len() != 2 || hash_hex.len() != 40 {
            return None;
        }
        if !hash_hex[..2].eq_ignore_ascii_case(prefix) {
            return None;
        }
        let mut raw = [0u8; 20];
        hex::decode_to_slice(hash_hex, &mut raw).ok()?;
        let &(index, size) = self.assets_by_sha1.get(&raw)?;
        Some(payload_for_seed(
            PayloadKind::Asset,
            index as usize,
            size as usize,
        ))
    }

    /// Renders Fabric Meta `/fabric/v2/versions/loader/{mc_ver}` response.
    pub fn fabric_loader_list_json(&self, mc_ver: &str) -> Option<Bytes> {
        if !SUPPORTED_MC_VERSIONS.contains(&mc_ver) {
            return None;
        }
        let min_java = if mc_ver == "1.21.1" { 21 } else { 17 };
        let asm_entry = self.maven_library_json("org.ow2.asm:asm:9.7.1");
        let override_entry = self.maven_library_json("org.mqmock:lib-0:2.0.0");
        let doc = json!([
            {
                "loader": {
                    "separator": ".",
                    "build": 9,
                    "maven": format!("net.fabricmc:fabric-loader:{DEFAULT_FABRIC_LOADER_VERSION}"),
                    "version": DEFAULT_FABRIC_LOADER_VERSION,
                    "stable": true
                },
                "intermediary": {
                    "maven": format!("net.fabricmc:intermediary:{mc_ver}"),
                    "version": mc_ver,
                    "stable": true
                },
                "launcherMeta": {
                    "version": 1,
                    "min_java_version": min_java,
                    "libraries": {
                        "client": [],
                        "common": [asm_entry, override_entry],
                        "server": []
                    },
                    "mainClass": {
                        "client": "net.fabricmc.loader.impl.launch.knot.KnotClient",
                        "server": "net.fabricmc.loader.impl.launch.knot.KnotServer"
                    }
                }
            }
        ]);
        Some(Bytes::from(
            serde_json::to_vec_pretty(&doc).expect("serialize fabric loader list"),
        ))
    }

    /// Renders Fabric Meta `/fabric/v2/versions/loader/{mc_ver}/{loader_ver}/profile/json`.
    ///
    /// Includes `org.mqmock:lib-0:2.0.0` to deterministically override `org.mqmock:lib-0:1.0.0`
    /// from the base vanilla `version.json`.
    pub fn fabric_profile_json(&self, mc_ver: &str, loader_ver: &str) -> Option<Bytes> {
        if !SUPPORTED_MC_VERSIONS.contains(&mc_ver) || loader_ver.is_empty() {
            return None;
        }
        let libs = vec![
            self.maven_library_json(&format!("net.fabricmc:fabric-loader:{loader_ver}")),
            self.maven_library_json(&format!("net.fabricmc:intermediary:{mc_ver}")),
            self.maven_library_json("org.ow2.asm:asm:9.7.1"),
            self.maven_library_json("org.mqmock:lib-0:2.0.0"),
        ];
        let doc = json!({
            "id": format!("fabric-loader-{loader_ver}-{mc_ver}"),
            "inheritsFrom": mc_ver,
            "releaseTime": "2024-11-01T00:00:00+00:00",
            "time": "2024-11-01T00:00:00+00:00",
            "type": "release",
            "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
            "arguments": {
                "game": [],
                "jvm": [
                    "-DFabricMcEmu= net.minecraft.client.main.Main "
                ]
            },
            "libraries": libs
        });
        Some(Bytes::from(
            serde_json::to_vec_pretty(&doc).expect("serialize fabric profile json"),
        ))
    }

    /// Renders Quilt Meta `/quilt/v3/versions/loader/{mc_ver}` response.
    pub fn quilt_loader_list_json(&self, mc_ver: &str) -> Option<Bytes> {
        if !SUPPORTED_MC_VERSIONS.contains(&mc_ver) {
            return None;
        }
        let doc = json!([
            {
                "loader": {
                    "separator": ".",
                    "build": 4,
                    "maven": format!("org.quiltmc:quilt-loader:{DEFAULT_QUILT_LOADER_VERSION}"),
                    "version": DEFAULT_QUILT_LOADER_VERSION
                },
                "hashed": {
                    "maven": format!("org.quiltmc:hashed:{mc_ver}"),
                    "version": mc_ver
                },
                "intermediary": {
                    "maven": format!("net.fabricmc:intermediary:{mc_ver}"),
                    "version": mc_ver
                },
                "launcherMeta": {
                    "version": 1,
                    "libraries": {
                        "client": [],
                        "common": [
                            self.maven_library_json("org.mqmock:lib-0:2.0.0")
                        ],
                        "server": []
                    },
                    "mainClass": {
                        "client": "org.quiltmc.loader.impl.launch.knot.KnotClient",
                        "server": "org.quiltmc.loader.impl.launch.knot.KnotServer"
                    }
                }
            }
        ]);
        Some(Bytes::from(
            serde_json::to_vec_pretty(&doc).expect("serialize quilt loader list"),
        ))
    }

    /// Renders Quilt Meta `/quilt/v3/versions/loader/{mc_ver}/{loader_ver}/profile/json`.
    pub fn quilt_profile_json(&self, mc_ver: &str, loader_ver: &str) -> Option<Bytes> {
        if !SUPPORTED_MC_VERSIONS.contains(&mc_ver) || loader_ver.is_empty() {
            return None;
        }
        let libs = vec![
            self.maven_library_json(&format!("org.quiltmc:quilt-loader:{loader_ver}")),
            self.maven_library_json(&format!("org.quiltmc:hashed:{mc_ver}")),
            self.maven_library_json(&format!("net.fabricmc:intermediary:{mc_ver}")),
            self.maven_library_json("org.mqmock:lib-0:2.0.0"),
        ];
        let doc = json!({
            "id": format!("quilt-loader-{loader_ver}-{mc_ver}"),
            "inheritsFrom": mc_ver,
            "releaseTime": "2024-11-01T00:00:00+00:00",
            "time": "2024-11-01T00:00:00+00:00",
            "type": "release",
            "mainClass": "org.quiltmc.loader.impl.launch.knot.KnotClient",
            "arguments": {
                "game": [],
                "jvm": [
                    "-DQuiltMcEmu= net.minecraft.client.main.Main "
                ]
            },
            "libraries": libs
        });
        Some(Bytes::from(
            serde_json::to_vec_pretty(&doc).expect("serialize quilt profile json"),
        ))
    }

    /// Builds a complete, deterministic Modrinth `.mrpack` archive containing:
    /// - `modrinth.index.json` (`modpack_mod_count` required client mods + 1 `client: unsupported` mod),
    /// - `overrides/config/mq-pack.toml`,
    /// - `overrides/options.txt` (overridden by `client-overrides/options.txt`),
    /// - `client-overrides/options.txt`.
    pub fn build_mrpack_bytes(&self) -> Vec<u8> {
        let mut files = Vec::with_capacity(self.mods.len() + 1);
        for (i, meta) in self.mods.iter().enumerate() {
            files.push(json!({
                "path": format!("mods/mod-{i}.jar"),
                "hashes": {
                    "sha1": sha1_hex(&meta.sha1),
                    "sha512": sha512_hex(&meta.sha512)
                },
                "env": {
                    "client": "required",
                    "server": "optional"
                },
                "downloads": [
                    format!("{}/download/mod/{i}.jar", self.base_url)
                ],
                "fileSize": meta.size
            }));
        }

        // Include 1 server-only ("client": "unsupported") entry that client installs MUST skip.
        files.push(json!({
            "path": "mods/server-only-mod.jar",
            "hashes": {
                "sha1": sha1_hex(&self.catalog.server_only_mod.sha1),
                "sha512": sha512_hex(&self.catalog.server_only_mod.sha512)
            },
            "env": {
                "client": "unsupported",
                "server": "required"
            },
            "downloads": [
                format!("{}/download/mod/server-only.jar", self.base_url)
            ],
            "fileSize": self.catalog.server_only_mod.size
        }));

        let index_doc = json!({
            "formatVersion": 1,
            "game": "minecraft",
            "versionId": "1.0.0",
            "name": "MQ Ultra Pack",
            "summary": "Deterministic synthetic Modrinth modpack for MQ benchmarks and tests",
            "dependencies": {
                "minecraft": "1.21.1",
                "fabric-loader": DEFAULT_FABRIC_LOADER_VERSION
            },
            "files": files
        });

        let opts = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .last_modified_time(DateTime::default());

        let mut out = Vec::with_capacity(32 * 1024);
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut out));
            zip.start_file("modrinth.index.json", opts)
                .expect("start modrinth.index.json");
            let index_bytes =
                serde_json::to_vec_pretty(&index_doc).expect("serialize modrinth.index.json");
            zip.write_all(&index_bytes)
                .expect("write modrinth.index.json");

            zip.start_file("overrides/config/mq-pack.toml", opts)
                .expect("start overrides/config/mq-pack.toml");
            zip.write_all(b"[pack]\nname = \"mq-ultra-pack\"\nversion = \"1.0.0\"\n")
                .expect("write overrides/config/mq-pack.toml");

            zip.start_file("overrides/options.txt", opts)
                .expect("start overrides/options.txt");
            zip.write_all(b"fov:0.0\ngamma:0.5\n")
                .expect("write overrides/options.txt");

            zip.start_file("client-overrides/options.txt", opts)
                .expect("start client-overrides/options.txt");
            zip.write_all(b"fov:0.75\ngamma:1.0\nmaxFps:240\n")
                .expect("write client-overrides/options.txt");

            zip.finish().expect("finish mrpack");
        }
        out
    }

    /// Builds a deterministic CurseForge `.zip` modpack archive (`manifest.json` + `overrides/`).
    pub fn build_curseforge_zip_bytes(&self) -> Vec<u8> {
        let manifest = json!({
            "minecraft": {
                "version": "1.21.1",
                "modLoaders": [
                    {
                        "id": format!("fabric-{DEFAULT_FABRIC_LOADER_VERSION}"),
                        "primary": true
                    }
                ]
            },
            "manifestType": "minecraftModpack",
            "manifestVersion": 1,
            "name": "MQ CurseForge Pack",
            "version": "1.0.0",
            "author": "MQ Engineering",
            "files": [
                { "projectID": 394468, "fileID": 5002, "required": true },
                { "projectID": 306612, "fileID": 5003, "required": true }
            ],
            "overrides": "overrides"
        });

        let opts = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .last_modified_time(DateTime::default());

        let mut out = Vec::with_capacity(8 * 1024);
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut out));
            zip.start_file("manifest.json", opts)
                .expect("start manifest.json");
            let manifest_bytes =
                serde_json::to_vec_pretty(&manifest).expect("serialize cf manifest.json");
            zip.write_all(&manifest_bytes)
                .expect("write cf manifest.json");

            zip.start_file("modlist.html", opts)
                .expect("start modlist.html");
            zip.write_all(b"<ul><li>Sodium</li><li>Fabric API</li></ul>\n")
                .expect("write modlist.html");

            zip.start_file("overrides/config/mq-cf.toml", opts)
                .expect("start overrides/config/mq-cf.toml");
            zip.write_all(b"[curseforge]\npack = \"mq-cf-pack\"\n")
                .expect("write overrides/config/mq-cf.toml");

            zip.start_file("overrides/options.txt", opts)
                .expect("start overrides/options.txt");
            zip.write_all(b"renderDistance:12\n")
                .expect("write overrides/options.txt");

            zip.finish().expect("finish curseforge zip");
        }
        out
    }

    /// Resolves any `/download/{*rel_path}` artifact on the fly into `(Bytes, content_type)`.
    pub fn download_payload(&self, rel_path: &str) -> Option<(Bytes, &'static str)> {
        let rel = rel_path.trim_start_matches('/');

        if rel == "authlib-injector.jar" {
            return Some((build_authlib_injector_jar(), "application/java-archive"));
        }
        if rel == "modpack/mq-ultra-pack.mrpack" {
            return Some((
                Bytes::from(self.build_mrpack_bytes()),
                "application/x-modrinth-modpack+zip",
            ));
        }
        if rel == "modpack/mq-cf-pack.zip" || rel == "curseforge/900001/5004/mq-cf-pack.zip" {
            return Some((
                Bytes::from(self.build_curseforge_zip_bytes()),
                "application/zip",
            ));
        }
        if let Some(rest) = rel.strip_prefix("client/") {
            if let Some(mc_ver) = rest.strip_suffix("/client.jar") {
                if SUPPORTED_MC_VERSIONS.contains(&mc_ver) {
                    return Some((build_client_jar(mc_ver), "application/java-archive"));
                }
            }
            return None;
        }
        if let Some(mod_tail) = rel.strip_prefix("mod/") {
            if mod_tail == "server-only.jar" {
                let jar = build_synthetic_mod_jar(&server_only_spec());
                return Some((Bytes::from(jar), "application/java-archive"));
            }
            if let Some(idx_str) = mod_tail.strip_suffix(".jar") {
                let idx: usize = idx_str.parse().ok()?;
                if idx < self.mods.len() {
                    let jar = build_synthetic_mod_jar(&SyntheticModSpec::indexed(idx));
                    return Some((Bytes::from(jar), "application/java-archive"));
                }
            }
            return None;
        }
        if rel == "catalog/sodium-0.6.0.jar" || rel == "curseforge/394468/5002/sodium-0.6.0.jar" {
            return Some((
                Bytes::from(build_synthetic_mod_jar(&sodium_spec())),
                "application/java-archive",
            ));
        }
        if rel == "catalog/fabric-api-0.102.0.jar"
            || rel == "curseforge/306612/5003/fabric-api-0.102.0.jar"
        {
            return Some((
                Bytes::from(build_synthetic_mod_jar(&fabric_api_spec())),
                "application/java-archive",
            ));
        }
        if rel == "catalog/lithium-0.13.0.jar" {
            return Some((
                Bytes::from(build_synthetic_mod_jar(&lithium_spec())),
                "application/java-archive",
            ));
        }
        if rel == "catalog/jei-1.21.1-19.8.0.jar"
            || rel == "curseforge/238222/5001/jei-1.21.1-19.8.0.jar"
        {
            return Some((
                Bytes::from(build_synthetic_mod_jar(&jei_spec())),
                "application/java-archive",
            ));
        }
        if let Some(lib_rel) = rel.strip_prefix("libraries/org/mqmock/") {
            return self
                .resolve_vanilla_library_download(lib_rel)
                .map(|b| (b, "application/java-archive"));
        }
        if let Some(maven_rel) = rel.strip_prefix("maven/") {
            let coord = parse_maven_path_to_coord(maven_rel)?;
            let jar = build_named_maven_jar(&coord, maven_seed(&coord));
            return Some((jar, "application/java-archive"));
        }
        None
    }

    fn resolve_vanilla_library_download(&self, lib_rel: &str) -> Option<Bytes> {
        // Expected: `lib-{i}/1.0.{i}/lib-{i}-1.0.{i}[-natives-linux].jar`
        let parts: Vec<&str> = lib_rel.split('/').collect();
        if parts.len() != 3 {
            return None;
        }
        let artifact = parts[0];
        let version = parts[1];
        let filename = parts[2];
        let idx: usize = artifact.strip_prefix("lib-")?.parse().ok()?;
        if idx >= self.libraries.len() || version != format!("1.0.{idx}") {
            return None;
        }
        let expected_std = format!("lib-{idx}-{version}.jar");
        let expected_native = format!("lib-{idx}-{version}-natives-linux.jar");
        if filename == expected_std {
            Some(build_library_jar(idx, version))
        } else if filename == expected_native && self.libraries[idx].native_linux.is_some() {
            Some(build_native_linux_jar(idx))
        } else {
            None
        }
    }

    fn maven_library_json(&self, coord: &str) -> serde_json::Value {
        let rel_path = coord_to_maven_path(coord);
        let jar = build_named_maven_jar(coord, maven_seed(coord));
        let sha1 = sha1_hex(&sha1_bytes(&jar));
        let size = jar.len() as u64;
        let maven_base = format!("{}/download/maven/", self.base_url);
        let full_url = format!("{maven_base}{rel_path}");
        json!({
            "name": coord,
            "url": maven_base,
            "sha1": sha1,
            "size": size,
            "downloads": {
                "artifact": {
                    "path": rel_path,
                    "sha1": sha1,
                    "size": size,
                    "url": full_url
                }
            }
        })
    }
}

#[inline]
fn asset_size_for_index(index: usize) -> usize {
    4096 + ((index.wrapping_mul(2_654_435_761)) % 8193)
}

fn render_asset_index_string(assets: &[AssetMeta]) -> String {
    let mut out = String::with_capacity(assets.len() * 96 + 32);
    out.push_str("{\"objects\":{");
    for (i, meta) in assets.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let hex_hash = sha1_hex(&meta.sha1);
        let key = match i {
            2 => "minecraft/lang/ru_ru.json".to_string(),
            3 => "pack.mcmeta".to_string(),
            4 => "minecraft/lang/en_us.json".to_string(),
            _ => format!("mq/sound_{i}.ogg"),
        };
        let _ = write!(
            out,
            "\"{key}\":{{\"hash\":\"{hex_hash}\",\"size\":{}}}",
            meta.size
        );
    }
    out.push_str("}}");
    out
}

fn dual_hash_meta(bytes: &[u8]) -> BlobDualHashMeta {
    BlobDualHashMeta {
        size: bytes.len() as u64,
        sha1: sha1_bytes(bytes),
        sha512: sha512_bytes(bytes),
    }
}

fn maven_seed(coord: &str) -> usize {
    coord.bytes().fold(5381usize, |acc, b| {
        acc.wrapping_mul(33).wrapping_add(b as usize)
    })
}

fn coord_to_maven_path(coord: &str) -> String {
    let mut sp = coord.splitn(3, ':');
    let group = sp.next().unwrap_or("org.mqmock").replace('.', "/");
    let artifact = sp.next().unwrap_or("artifact");
    let version = sp.next().unwrap_or("1.0.0");
    format!("{group}/{artifact}/{version}/{artifact}-{version}.jar")
}

fn parse_maven_path_to_coord(rel_path: &str) -> Option<String> {
    let parts: Vec<&str> = rel_path.trim_matches('/').split('/').collect();
    if parts.len() < 4 {
        return None;
    }
    let filename = parts[parts.len() - 1];
    let version = parts[parts.len() - 2];
    let artifact = parts[parts.len() - 3];
    let expected_file = format!("{artifact}-{version}.jar");
    if filename != expected_file {
        return None;
    }
    let group = parts[..parts.len() - 3].join(".");
    Some(format!("{group}:{artifact}:{version}"))
}

pub(crate) fn sodium_spec() -> SyntheticModSpec {
    SyntheticModSpec {
        id: "sodium".to_string(),
        name: "Sodium".to_string(),
        version: "0.6.0+mc1.21.1".to_string(),
        authors: vec!["JellySquid".to_string()],
        depends: vec![
            ("minecraft".to_string(), "1.21.1".to_string()),
            ("fabricloader".to_string(), ">=0.16.0".to_string()),
            ("fabric-api".to_string(), ">=0.100.0".to_string()),
        ],
        format: ModMetadataFormat::Fabric,
        seed_index: 50_001,
        class_payload_size: 24 * 1024,
    }
}

pub(crate) fn fabric_api_spec() -> SyntheticModSpec {
    SyntheticModSpec {
        id: "fabric-api".to_string(),
        name: "Fabric API".to_string(),
        version: "0.102.0+1.21.1".to_string(),
        authors: vec!["FabricMC".to_string()],
        depends: vec![
            ("minecraft".to_string(), "1.21.1".to_string()),
            ("fabricloader".to_string(), ">=0.16.0".to_string()),
        ],
        format: ModMetadataFormat::Fabric,
        seed_index: 50_002,
        class_payload_size: 28 * 1024,
    }
}

pub(crate) fn lithium_spec() -> SyntheticModSpec {
    SyntheticModSpec {
        id: "lithium".to_string(),
        name: "Lithium".to_string(),
        version: "0.13.0+mc1.21.1".to_string(),
        authors: vec!["JellySquid".to_string(), "2No2Name".to_string()],
        depends: vec![
            ("minecraft".to_string(), "1.21.1".to_string()),
            ("fabricloader".to_string(), ">=0.16.0".to_string()),
        ],
        format: ModMetadataFormat::Fabric,
        seed_index: 50_003,
        class_payload_size: 20 * 1024,
    }
}

pub(crate) fn jei_spec() -> SyntheticModSpec {
    SyntheticModSpec {
        id: "jei".to_string(),
        name: "Just Enough Items".to_string(),
        version: "19.8.0".to_string(),
        authors: vec!["mezz".to_string()],
        depends: vec![("minecraft".to_string(), "[1.21,1.22)".to_string())],
        format: ModMetadataFormat::ForgeModsToml,
        seed_index: 50_004,
        class_payload_size: 22 * 1024,
    }
}

pub(crate) fn server_only_spec() -> SyntheticModSpec {
    SyntheticModSpec {
        id: "mq_server_only".to_string(),
        name: "MQ Dedicated Server Utility".to_string(),
        version: "1.0.0".to_string(),
        authors: vec!["MQBench".to_string()],
        depends: vec![("minecraft".to_string(), "1.21.1".to_string())],
        format: ModMetadataFormat::Fabric,
        seed_index: 50_099,
        class_payload_size: 16 * 1024,
    }
}
