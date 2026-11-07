//! `testsupport` — Deterministic, low-memory local `axum` HTTP mock server and synthetic
//! dataset generators for offline MQ Launcher unit tests, chaos tests, and `mq bench`.

pub mod catalog;
pub mod chaos;
pub mod ely;
pub mod faults;
pub mod payload;
pub mod server;
pub mod world;

pub use chaos::{
    assert_install_fails_on_readonly_dir, kill_installation_mid_flight, run_concurrent_installs,
    with_read_only_dir, ReadOnlyGuard,
};
pub use ely::MOCK_TOTP_CODE;
pub use faults::{FaultAction, FaultInjector, FaultRule};
pub use payload::{
    build_authlib_injector_jar, build_client_jar, build_corrupted_hash_mrpack, build_library_jar,
    build_malicious_zip_slip_mrpack, build_named_maven_jar, build_native_linux_jar,
    build_skin_png_bytes, build_synthetic_mod_jar, build_zip_bomb_mrpack, payload_for_seed,
    sha1_bytes, sha1_hex, sha256_bytes, sha256_hex, sha512_bytes, sha512_hex, ModMetadataFormat,
    PayloadKind, SyntheticModSpec,
};
pub use server::MockServer;
pub use world::{
    MockConfig, MockWorld, DEFAULT_FABRIC_LOADER_VERSION, DEFAULT_QUILT_LOADER_VERSION,
    SUPPORTED_MC_VERSIONS,
};

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::io::{Cursor, Read};
    use std::net::SocketAddr;

    use serde_json::{json, Value};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use zip::ZipArchive;

    use super::*;

    struct RawHttpResponse {
        status: u16,
        headers: HashMap<String, String>,
        body: Vec<u8>,
    }

    async fn raw_http_request(
        addr: SocketAddr,
        method: &str,
        path_and_query: &str,
        extra_headers: &[(&str, &str)],
        body: Option<&[u8]>,
    ) -> RawHttpResponse {
        let mut stream = TcpStream::connect(addr).await.expect("connect mock server");
        let body_slice = body.unwrap_or(&[]);

        let mut req =
            format!("{method} {path_and_query} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n");
        if body.is_some() {
            req.push_str(&format!("Content-Length: {}\r\n", body_slice.len()));
        }
        for (k, v) in extra_headers {
            req.push_str(&format!("{k}: {v}\r\n"));
        }
        req.push_str("\r\n");

        stream
            .write_all(req.as_bytes())
            .await
            .expect("write request headers");
        if !body_slice.is_empty() {
            stream
                .write_all(body_slice)
                .await
                .expect("write request body");
        }

        let mut raw = Vec::new();
        stream
            .read_to_end(&mut raw)
            .await
            .expect("read full HTTP response");

        let header_end = match raw.windows(4).position(|w| w == b"\r\n\r\n") {
            Some(pos) => pos,
            None => {
                panic!(
                    "HTTP header delimiter missing. raw len={}, raw={:?}",
                    raw.len(),
                    String::from_utf8_lossy(&raw)
                );
            }
        };
        let header_text = std::str::from_utf8(&raw[..header_end]).expect("UTF-8 HTTP headers");
        let raw_body = &raw[header_end + 4..];

        let mut lines = header_text.split("\r\n");
        let status_line = lines.next().expect("HTTP status line");
        let status: u16 = status_line
            .split_whitespace()
            .nth(1)
            .expect("HTTP status code token")
            .parse()
            .expect("numeric HTTP status");

        let mut headers = HashMap::new();
        for line in lines {
            if let Some((k, v)) = line.split_once(':') {
                headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
            }
        }

        let decoded_body = if headers
            .get("transfer-encoding")
            .map(|v| v.eq_ignore_ascii_case("chunked"))
            .unwrap_or(false)
        {
            decode_chunked(raw_body)
        } else {
            raw_body.to_vec()
        };

        RawHttpResponse {
            status,
            headers,
            body: decoded_body,
        }
    }

    fn decode_chunked(mut input: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        while let Some(line_end) = input.windows(2).position(|w| w == b"\r\n") {
            let size_hex = std::str::from_utf8(&input[..line_end])
                .expect("chunk size utf8")
                .split(';')
                .next()
                .unwrap()
                .trim();
            let chunk_len = usize::from_str_radix(size_hex, 16).expect("chunk size hex");
            input = &input[line_end + 2..];
            if chunk_len == 0 {
                break;
            }
            out.extend_from_slice(&input[..chunk_len]);
            input = &input[chunk_len + 2..];
        }
        out
    }

    fn path_from_url<'a>(url: &'a str, base_url: &str) -> &'a str {
        url.strip_prefix(base_url)
            .expect("URL must start with server base_url")
    }

    #[tokio::test]
    async fn mojang_manifest_version_libraries_and_3200_assets_end_to_end() {
        let server = MockServer::start(MockConfig::default()).await;
        let addr = server.addr();
        let base = server.base_url().to_string();

        // 1. Fetch version_manifest_v2.json
        let manifest_resp =
            raw_http_request(addr, "GET", "/mc/game/version_manifest_v2.json", &[], None).await;
        assert_eq!(manifest_resp.status, 200);
        let manifest: Value = serde_json::from_slice(&manifest_resp.body).unwrap();
        let versions = manifest["versions"].as_array().unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0]["id"], "1.21.1");
        assert_eq!(versions[1]["id"], "1.20.4");

        // Verify 1.20.4 Java 17 metadata
        let v1204_path = path_from_url(versions[1]["url"].as_str().unwrap(), &base);
        let v1204_resp = raw_http_request(addr, "GET", v1204_path, &[], None).await;
        assert_eq!(v1204_resp.status, 200);
        assert_eq!(
            sha1_hex(&sha1_bytes(&v1204_resp.body)),
            versions[1]["sha1"].as_str().unwrap()
        );
        let v1204_json: Value = serde_json::from_slice(&v1204_resp.body).unwrap();
        assert_eq!(v1204_json["javaVersion"]["majorVersion"], 17);

        // 2. Verify 1.21.1 version.json, arguments, libraries, client.jar, and 3,200 assets
        let v1211_path = path_from_url(versions[0]["url"].as_str().unwrap(), &base);
        let v1211_resp = raw_http_request(addr, "GET", v1211_path, &[], None).await;
        assert_eq!(v1211_resp.status, 200);
        assert_eq!(
            sha1_hex(&sha1_bytes(&v1211_resp.body)),
            versions[0]["sha1"].as_str().unwrap()
        );
        let v1211: Value = serde_json::from_slice(&v1211_resp.body).unwrap();
        assert_eq!(v1211["mainClass"], "net.minecraft.client.main.Main");
        assert_eq!(v1211["javaVersion"]["majorVersion"], 21);

        let libs = v1211["libraries"].as_array().unwrap();
        assert_eq!(libs.len(), 110);

        // Check macOS-only rule library exists
        let has_macos_only = libs.iter().any(|l| {
            l["rules"]
                .as_array()
                .map(|r| r.iter().any(|rule| rule["os"]["name"] == "osx"))
                .unwrap_or(false)
        });
        assert!(has_macos_only, "must include macOS-only library rule");

        // Check Linux native library jar and verify it contains libmocknative.so + MANIFEST.MF
        let native_lib = libs
            .iter()
            .find(|l| l["natives"]["linux"] == "natives-linux")
            .expect("must include Linux native library");
        let native_cls = &native_lib["downloads"]["classifiers"]["natives-linux"];
        let native_path = path_from_url(native_cls["url"].as_str().unwrap(), &base);
        let native_resp = raw_http_request(addr, "GET", native_path, &[], None).await;
        assert_eq!(native_resp.status, 200);
        assert_eq!(
            native_resp.body.len() as u64,
            native_cls["size"].as_u64().unwrap()
        );
        assert_eq!(
            sha1_hex(&sha1_bytes(&native_resp.body)),
            native_cls["sha1"].as_str().unwrap()
        );
        {
            let mut archive = ZipArchive::new(Cursor::new(&native_resp.body)).unwrap();
            assert!(archive.by_name("libmocknative.so").is_ok());
            assert!(archive.by_name("META-INF/MANIFEST.MF").is_ok());
        }

        // Verify client.jar SHA-1 and size
        let client_dl = &v1211["downloads"]["client"];
        let client_path = path_from_url(client_dl["url"].as_str().unwrap(), &base);
        let client_resp = raw_http_request(addr, "GET", client_path, &[], None).await;
        assert_eq!(client_resp.status, 200);
        assert_eq!(
            client_resp.body.len() as u64,
            client_dl["size"].as_u64().unwrap()
        );
        assert_eq!(
            sha1_hex(&sha1_bytes(&client_resp.body)),
            client_dl["sha1"].as_str().unwrap()
        );

        // Verify asset_index.json has 3,200 distinct hashes and spot-check served asset blobs
        let asset_idx_meta = &v1211["assetIndex"];
        let idx_path = path_from_url(asset_idx_meta["url"].as_str().unwrap(), &base);
        let idx_resp = raw_http_request(addr, "GET", idx_path, &[], None).await;
        assert_eq!(idx_resp.status, 200);
        assert_eq!(
            sha1_hex(&sha1_bytes(&idx_resp.body)),
            asset_idx_meta["sha1"].as_str().unwrap()
        );
        assert_eq!(
            idx_resp.body.len() as u64,
            asset_idx_meta["size"].as_u64().unwrap()
        );

        let idx_json: Value = serde_json::from_slice(&idx_resp.body).unwrap();
        let objects = idx_json["objects"].as_object().unwrap();
        assert_eq!(objects.len(), 3200);

        let mut distinct_hashes = HashSet::with_capacity(objects.len());
        let mut total_size = 0u64;
        for obj in objects.values() {
            distinct_hashes.insert(obj["hash"].as_str().unwrap().to_string());
            total_size += obj["size"].as_u64().unwrap();
        }
        assert_eq!(distinct_hashes.len(), 3200);
        assert_eq!(total_size, asset_idx_meta["totalSize"].as_u64().unwrap());

        for sample_idx in [0usize, 1, 1599, 3199] {
            let key = format!("mq/sound_{sample_idx}.ogg");
            let entry = &objects[&key];
            let hash = entry["hash"].as_str().unwrap();
            let size = entry["size"].as_u64().unwrap() as usize;
            let obj_path = format!("/mc/assets/objects/{}/{hash}", &hash[..2]);
            let obj_resp = raw_http_request(addr, "GET", &obj_path, &[], None).await;
            assert_eq!(obj_resp.status, 200);
            assert_eq!(obj_resp.body.len(), size);
            assert_eq!(sha1_hex(&sha1_bytes(&obj_resp.body)), hash);
        }

        assert!(server.request_count() >= 10);
        server.shutdown().await;
    }

    #[tokio::test]
    async fn fabric_quilt_profiles_modrinth_deps_and_mrpack_accuracy() {
        let mut cfg = MockConfig::small();
        cfg.modpack_mod_count = 220;
        let server = MockServer::start(cfg).await;
        let addr = server.addr();
        let base = server.base_url().to_string();

        // 1. Fabric & Quilt profiles
        let fab_resp = raw_http_request(
            addr,
            "GET",
            "/fabric/v2/versions/loader/1.21.1/0.16.9/profile/json",
            &[],
            None,
        )
        .await;
        assert_eq!(fab_resp.status, 200);
        let fab_json: Value = serde_json::from_slice(&fab_resp.body).unwrap();
        assert_eq!(fab_json["inheritsFrom"], "1.21.1");
        assert_eq!(
            fab_json["mainClass"],
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
        let fab_libs = fab_json["libraries"].as_array().unwrap();
        assert!(fab_libs
            .iter()
            .any(|l| l["name"] == "net.fabricmc:fabric-loader:0.16.9"));
        let override_lib = fab_libs
            .iter()
            .find(|l| l["name"] == "org.mqmock:lib-0:2.0.0")
            .expect("must override vanilla lib-0 coordinate");
        let ov_path = path_from_url(
            override_lib["downloads"]["artifact"]["url"]
                .as_str()
                .unwrap(),
            &base,
        );
        let ov_dl = raw_http_request(addr, "GET", ov_path, &[], None).await;
        assert_eq!(ov_dl.status, 200);
        assert_eq!(
            sha1_hex(&sha1_bytes(&ov_dl.body)),
            override_lib["sha1"].as_str().unwrap()
        );

        let quilt_resp = raw_http_request(
            addr,
            "GET",
            "/quilt/v3/versions/loader/1.21.1/0.26.4/profile/json",
            &[],
            None,
        )
        .await;
        assert_eq!(quilt_resp.status, 200);
        let quilt_json: Value = serde_json::from_slice(&quilt_resp.body).unwrap();
        assert_eq!(quilt_json["inheritsFrom"], "1.21.1");
        assert_eq!(
            quilt_json["mainClass"],
            "org.quiltmc.loader.impl.launch.knot.KnotClient"
        );

        // 2. Modrinth search & dependency chain (`sodium` -> required `fabric-api`)
        let search_resp = raw_http_request(
            addr,
            "GET",
            "/modrinth/v2/search?query=sodium&limit=5&offset=0",
            &[],
            None,
        )
        .await;
        assert_eq!(search_resp.status, 200);
        let search_json: Value = serde_json::from_slice(&search_resp.body).unwrap();
        assert_eq!(search_json["hits"][0]["slug"], "sodium");

        let sod_ver_resp = raw_http_request(
            addr,
            "GET",
            "/modrinth/v2/project/sodium/version",
            &[],
            None,
        )
        .await;
        assert_eq!(sod_ver_resp.status, 200);
        let sod_vers: Value = serde_json::from_slice(&sod_ver_resp.body).unwrap();
        let deps = sod_vers[0]["dependencies"].as_array().unwrap();
        assert_eq!(deps[0]["project_id"], "P7dR8mSH");
        assert_eq!(deps[0]["dependency_type"], "required");

        // Verify dependent project resolves to fabric-api
        let fapi_proj_resp =
            raw_http_request(addr, "GET", "/modrinth/v2/project/P7dR8mSH", &[], None).await;
        assert_eq!(fapi_proj_resp.status, 200);
        let fapi_proj: Value = serde_json::from_slice(&fapi_proj_resp.body).unwrap();
        assert_eq!(fapi_proj["slug"], "fabric-api");

        // 3. Download `.mrpack` and validate 220 client mods + 1 unsupported + overrides + hashes
        let mrpack_resp = raw_http_request(
            addr,
            "GET",
            "/download/modpack/mq-ultra-pack.mrpack",
            &[],
            None,
        )
        .await;
        assert_eq!(mrpack_resp.status, 200);
        assert_eq!(mrpack_resp.body, server.build_mrpack_bytes());

        let index_json: Value = {
            let mut zip = ZipArchive::new(Cursor::new(&mrpack_resp.body)).unwrap();
            assert!(zip.by_name("overrides/config/mq-pack.toml").is_ok());
            assert!(zip.by_name("client-overrides/options.txt").is_ok());
            let mut idx_entry = zip.by_name("modrinth.index.json").unwrap();
            let mut buf = Vec::new();
            idx_entry.read_to_end(&mut buf).unwrap();
            serde_json::from_slice(&buf).unwrap()
        };

        assert_eq!(index_json["formatVersion"], 1);
        assert_eq!(index_json["game"], "minecraft");
        assert_eq!(index_json["dependencies"]["minecraft"], "1.21.1");
        assert_eq!(index_json["dependencies"]["fabric-loader"], "0.16.9");

        let files = index_json["files"].as_array().unwrap();
        assert_eq!(files.len(), 221);
        let unsupported_count = files
            .iter()
            .filter(|f| f["env"]["client"] == "unsupported")
            .count();
        assert_eq!(unsupported_count, 1);

        // Spot-check mod-0 (Fabric), mod-9 (Quilt), mod-19 (Forge mods.toml), and mod-219
        for (mod_idx, expected_entry_name) in [
            (0usize, "fabric.mod.json"),
            (9usize, "quilt.mod.json"),
            (19usize, "META-INF/mods.toml"),
            (219usize, "META-INF/mods.toml"),
        ] {
            let f = &files[mod_idx];
            assert_eq!(f["path"], format!("mods/mod-{mod_idx}.jar"));
            let dl_url = f["downloads"][0].as_str().unwrap();
            let dl_path = path_from_url(dl_url, &base);
            let jar_resp = raw_http_request(addr, "GET", dl_path, &[], None).await;
            assert_eq!(jar_resp.status, 200);
            assert_eq!(jar_resp.body.len() as u64, f["fileSize"].as_u64().unwrap());
            assert_eq!(
                sha1_hex(&sha1_bytes(&jar_resp.body)),
                f["hashes"]["sha1"].as_str().unwrap()
            );
            assert_eq!(
                sha512_hex(&sha512_bytes(&jar_resp.body)),
                f["hashes"]["sha512"].as_str().unwrap()
            );

            let mut jar_zip = ZipArchive::new(Cursor::new(&jar_resp.body)).unwrap();
            assert!(
                jar_zip.by_name(expected_entry_name).is_ok(),
                "mod-{mod_idx}.jar must contain {expected_entry_name}"
            );
        }

        // 4. Verify Zip-Slip malicious mrpack contains traversal entry
        let evil = build_malicious_zip_slip_mrpack();
        let mut evil_zip = ZipArchive::new(Cursor::new(&evil)).unwrap();
        let mut found_slip = false;
        for i in 0..evil_zip.len() {
            let entry = evil_zip.by_index(i).unwrap();
            if entry.name() == "overrides/../../etc/passwd" {
                assert!(entry.enclosed_name().is_none());
                found_slip = true;
            }
        }
        assert!(
            found_slip,
            "malicious mrpack must contain overrides/../../etc/passwd"
        );

        server.shutdown().await;
    }

    #[tokio::test]
    async fn http_range_206_fault_injection_curseforge_and_ely_auth_skins() {
        let mut cfg = MockConfig::small();
        cfg.fail_first_n_requests = 2;
        let server = MockServer::start(cfg).await;
        let addr = server.addr();
        let base = server.base_url().to_string();

        // 1. First 2 requests must fail with 503 due to `fail_first_n_requests = 2`
        let fail1 = raw_http_request(
            addr,
            "GET",
            "/authlib-injector/artifact/latest.json",
            &[],
            None,
        )
        .await;
        let fail2 = raw_http_request(
            addr,
            "GET",
            "/authlib-injector/artifact/latest.json",
            &[],
            None,
        )
        .await;
        assert_eq!(fail1.status, 503);
        assert_eq!(fail2.status, 503);

        // 2. Third request succeeds: verify authlib-injector metadata + SHA-256 + HTTP Range 206
        let authlib_meta_resp = raw_http_request(
            addr,
            "GET",
            "/authlib-injector/artifact/latest.json",
            &[],
            None,
        )
        .await;
        assert_eq!(authlib_meta_resp.status, 200);
        let authlib_meta: Value = serde_json::from_slice(&authlib_meta_resp.body).unwrap();
        assert_eq!(authlib_meta["build_number"], 53);
        assert_eq!(authlib_meta["version"], "1.2.5");
        let expected_sha256 = authlib_meta["checksums"]["sha256"].as_str().unwrap();

        let jar_path = path_from_url(authlib_meta["download_url"].as_str().unwrap(), &base);
        let full_jar_resp = raw_http_request(addr, "GET", jar_path, &[], None).await;
        assert_eq!(full_jar_resp.status, 200);
        assert_eq!(
            sha256_hex(&sha256_bytes(&full_jar_resp.body)),
            expected_sha256
        );

        let split_at = 4096usize;
        let partial_resp = raw_http_request(
            addr,
            "GET",
            jar_path,
            &[("Range", &format!("bytes={split_at}-"))],
            None,
        )
        .await;
        assert_eq!(partial_resp.status, 206);
        let expected_cr = format!(
            "bytes {split_at}-{}/{}",
            full_jar_resp.body.len() - 1,
            full_jar_resp.body.len()
        );
        assert_eq!(
            partial_resp
                .headers
                .get("content-range")
                .map(String::as_str),
            Some(expected_cr.as_str())
        );
        assert_eq!(
            partial_resp.body.as_slice(),
            &full_jar_resp.body[split_at..]
        );

        // 3. CurseForge v1 API key check + mod files + CurseForge .zip modpack
        let cf_forbidden =
            raw_http_request(addr, "GET", "/curseforge/v1/mods/search", &[], None).await;
        assert_eq!(cf_forbidden.status, 403);

        let cf_ok = raw_http_request(
            addr,
            "GET",
            "/curseforge/v1/mods/394468/files",
            &[("x-api-key", "test-cf-key")],
            None,
        )
        .await;
        assert_eq!(cf_ok.status, 200);
        let cf_files: Value = serde_json::from_slice(&cf_ok.body).unwrap();
        assert_eq!(cf_files["data"][0]["dependencies"][0]["modId"], 306612);

        let cf_zip = server.build_curseforge_zip_bytes();
        let mut cf_archive = ZipArchive::new(Cursor::new(&cf_zip)).unwrap();
        assert!(cf_archive.by_name("manifest.json").is_ok());
        assert!(cf_archive.by_name("overrides/config/mq-cf.toml").is_ok());

        // 4. Ely.by authenticate (standard + 2FA), validate, refresh, and 64x64 RGBA skin PNG
        let bad_2fa_body = serde_json::to_vec(&json!({
            "username": "user2fa",
            "password": "correct_password",
            "requestUser": true
        }))
        .unwrap();
        let bad_2fa_resp = raw_http_request(
            addr,
            "POST",
            "/ely/auth/authenticate",
            &[("Content-Type", "application/json")],
            Some(&bad_2fa_body),
        )
        .await;
        assert_eq!(bad_2fa_resp.status, 401);

        let ok_2fa_body = serde_json::to_vec(&json!({
            "username": "user2fa",
            "password": "correct_password",
            "clientToken": "ct-123",
            "requestUser": true,
            "totp": MOCK_TOTP_CODE
        }))
        .unwrap();
        let ok_2fa_resp = raw_http_request(
            addr,
            "POST",
            "/ely/auth/authenticate",
            &[("Content-Type", "application/json")],
            Some(&ok_2fa_body),
        )
        .await;
        assert_eq!(ok_2fa_resp.status, 200);
        let auth_json: Value = serde_json::from_slice(&ok_2fa_resp.body).unwrap();
        let tok1 = auth_json["accessToken"].as_str().unwrap().to_string();
        assert_eq!(auth_json["clientToken"], "ct-123");
        assert_eq!(auth_json["selectedProfile"]["name"], "user2fa");
        assert!(auth_json["user"].is_object());

        let val_body = serde_json::to_vec(&json!({
            "accessToken": tok1,
            "clientToken": "ct-123"
        }))
        .unwrap();
        let val_resp = raw_http_request(
            addr,
            "POST",
            "/ely/auth/validate",
            &[("Content-Type", "application/json")],
            Some(&val_body),
        )
        .await;
        assert_eq!(val_resp.status, 204);

        let ref_body = serde_json::to_vec(&json!({
            "accessToken": tok1,
            "clientToken": "ct-123",
            "requestUser": true
        }))
        .unwrap();
        let ref_resp = raw_http_request(
            addr,
            "POST",
            "/ely/auth/refresh",
            &[("Content-Type", "application/json")],
            Some(&ref_body),
        )
        .await;
        assert_eq!(ref_resp.status, 200);
        let ref_json: Value = serde_json::from_slice(&ref_resp.body).unwrap();
        let tok2 = ref_json["accessToken"].as_str().unwrap();
        assert_ne!(tok1, tok2);

        // Old token must now be invalid
        let old_val_resp = raw_http_request(
            addr,
            "POST",
            "/ely/auth/validate",
            &[("Content-Type", "application/json")],
            Some(&val_body),
        )
        .await;
        assert_eq!(old_val_resp.status, 403);

        // Verify 64x64 RGBA skin PNG face (8..16, 8..16) and hat (40..48, 8..16)
        let skin_resp = raw_http_request(addr, "GET", "/ely/skins/SteveMQ.png", &[], None).await;
        assert_eq!(skin_resp.status, 200);
        assert_eq!(
            skin_resp.headers.get("content-type").map(String::as_str),
            Some("image/png")
        );
        let decoded = image::load_from_memory(&skin_resp.body)
            .expect("decode PNG")
            .to_rgba8();
        assert_eq!(decoded.dimensions(), (64, 64));

        // Background corner (0, 0) is transparent, face & hat are colorful and distinct.
        assert_eq!(decoded.get_pixel(0, 0).0, [0, 0, 0, 0]);
        for y in 8..16 {
            for x in 8..16 {
                let face_px = decoded.get_pixel(x, y).0;
                let hat_px = decoded.get_pixel(x + 32, y).0;
                assert_eq!(face_px[3], 255, "face pixel must be opaque");
                assert!(hat_px[3] > 0, "hat pixel must have non-zero alpha");
                assert_ne!(
                    &face_px[..3],
                    &hat_px[..3],
                    "face and hat pixels must be visually distinct"
                );
            }
        }

        server.shutdown().await;
    }

    #[test]
    fn fault_archives_launch_placeholders_and_memory_footprint_budget() {
        fn current_rss_kib() -> Option<u64> {
            let status = std::fs::read_to_string("/proc/self/status").ok()?;
            for line in status.lines() {
                if let Some(rest) = line.strip_prefix("VmRSS:") {
                    return rest.split_whitespace().next()?.parse().ok();
                }
            }
            None
        }

        let rss_before = current_rss_kib().unwrap_or(0);
        let world = MockWorld::new(MockConfig::default(), "http://127.0.0.1:9999");
        let rss_after = current_rss_kib().unwrap_or(0);
        let rss_delta_kib = rss_after.saturating_sub(rss_before);
        assert!(
            rss_delta_kib < 15 * 1024,
            "MockWorld RSS delta ({rss_delta_kib} KiB) must stay well under 15 MiB"
        );

        let v1211: Value = serde_json::from_slice(&world.version_json("1.21.1").unwrap()).unwrap();
        let jvm_args_str = serde_json::to_string(&v1211["arguments"]["jvm"]).unwrap();
        assert!(jvm_args_str.contains("-Djava.library.path=${natives_directory}"));
        assert!(jvm_args_str.contains("\"-cp\""));
        assert!(jvm_args_str.contains("\"${classpath}\""));

        let game_args_str = serde_json::to_string(&v1211["arguments"]["game"]).unwrap();
        for required_tok in [
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
        ] {
            assert!(
                game_args_str.contains(required_tok),
                "missing game argument token {required_tok}"
            );
        }

        let corrupt = build_corrupted_hash_mrpack("http://127.0.0.1:9999");
        let mut corrupt_zip = ZipArchive::new(Cursor::new(&corrupt)).unwrap();
        assert!(corrupt_zip.by_name("modrinth.index.json").is_ok());

        let bomb = build_zip_bomb_mrpack(8);
        assert!(
            bomb.len() < 128 * 1024,
            "compressed 8 MiB bomb should stay tiny"
        );
        let mut bomb_zip = ZipArchive::new(Cursor::new(&bomb)).unwrap();
        let bomb_entry = bomb_zip.by_name("overrides/bomb.bin").unwrap();
        assert_eq!(bomb_entry.size(), 8 * 1024 * 1024);
    }

    #[tokio::test]
    async fn test_language_assets_and_options_txt_patching() {
        let config = MockConfig {
            asset_count: 50,
            ..Default::default()
        };
        let server = MockServer::start(config).await;
        let addr = server.addr();

        // 1. Fetch asset index and check ru_ru.json and pack.mcmeta are present
        let resp = raw_http_request(addr, "GET", "/mc/assets/indexes/1.21.1.json", &[], None).await;
        assert_eq!(resp.status, 200);
        let index_json: serde_json::Value = serde_json::from_slice(&resp.body).expect("json");
        let objects = index_json["objects"].as_object().expect("objects");

        assert!(objects.contains_key("minecraft/lang/ru_ru.json"));
        assert!(objects.contains_key("pack.mcmeta"));

        let ru_hash = objects["minecraft/lang/ru_ru.json"]["hash"]
            .as_str()
            .unwrap();
        let ru_path = format!("/mc/assets/objects/{}/{ru_hash}", &ru_hash[..2]);
        let ru_resp = raw_http_request(addr, "GET", &ru_path, &[], None).await;
        assert_eq!(ru_resp.status, 200);

        // 2. Test options.txt patching
        let tmp = tempfile::tempdir().expect("tempdir");
        let inst_dir = tmp.path();

        // Fresh instance: creates options.txt with lang:ru_ru
        let initialized =
            mq_launch::patch_minecraft_options_lang(inst_dir, "1.21.1", false).expect("patch");
        assert!(initialized);
        let opts = std::fs::read_to_string(inst_dir.join("options.txt")).expect("read options");
        assert_eq!(opts.trim(), "lang:ru_ru");

        // Legacy instance (1.7.10): creates options.txt with uppercase lang:ru_RU
        let tmp_legacy = tempfile::tempdir().expect("tempdir");
        let init_legacy =
            mq_launch::patch_minecraft_options_lang(tmp_legacy.path(), "1.7.10", false)
                .expect("patch");
        assert!(init_legacy);
        let opts_legacy =
            std::fs::read_to_string(tmp_legacy.path().join("options.txt")).expect("read options");
        assert_eq!(opts_legacy.trim(), "lang:ru_RU");

        // Initialized instance with custom lang is never overwritten
        std::fs::write(inst_dir.join("options.txt"), "fov:90.0\nlang:en_us\n").expect("write");
        let init_existing =
            mq_launch::patch_minecraft_options_lang(inst_dir, "1.21.1", true).expect("patch");
        assert!(init_existing);
        let opts_existing =
            std::fs::read_to_string(inst_dir.join("options.txt")).expect("read options");
        assert!(opts_existing.contains("lang:en_us"));
        assert!(!opts_existing.contains("lang:ru_ru"));

        server.shutdown().await;
    }

    #[tokio::test]
    async fn failure_injection_http_errors_truncation_corrupted_sha1_and_chaos() {
        let server = MockServer::start(MockConfig::small()).await;
        let addr = server.addr();

        // 1. Test 404 injection
        server.inject_404(Some("/mc/game/version_manifest_v2.json"), 1);
        let resp_404 =
            raw_http_request(addr, "GET", "/mc/game/version_manifest_v2.json", &[], None).await;
        assert_eq!(resp_404.status, 404);

        // Next request succeeds normally (count expired)
        let resp_ok =
            raw_http_request(addr, "GET", "/mc/game/version_manifest_v2.json", &[], None).await;
        assert_eq!(resp_ok.status, 200);

        // 2. Test 403 Forbidden injection
        server.inject_403(Some("/mc/meta/"), 1);
        let resp_403 =
            raw_http_request(addr, "GET", "/mc/meta/1.21.1/version.json", &[], None).await;
        assert_eq!(resp_403.status, 403);

        // 3. Test 429 Too Many Requests with Retry-After header
        server.inject_429(Some("/mc/meta/"), 5, 1);
        let resp_429 =
            raw_http_request(addr, "GET", "/mc/meta/1.21.1/version.json", &[], None).await;
        assert_eq!(resp_429.status, 429);
        assert_eq!(
            resp_429.headers.get("retry-after").map(String::as_str),
            Some("5")
        );

        // 4. Test 500 Internal Server Error injection
        server.inject_500(Some("/mc/meta/"), 1);
        let resp_500 =
            raw_http_request(addr, "GET", "/mc/meta/1.21.1/version.json", &[], None).await;
        assert_eq!(resp_500.status, 500);

        // Fetch normal response as baseline
        let normal_ver_resp =
            raw_http_request(addr, "GET", "/mc/meta/1.21.1/version.json", &[], None).await;
        let normal_ver_len = normal_ver_resp.body.len();
        let normal_sha1 = sha1_hex(&sha1_bytes(&normal_ver_resp.body));

        // 5. Test Body Truncation mid-file
        server.inject_truncation(Some("/mc/meta/1.21.1/version.json"), 16, 1);
        let resp_trunc =
            raw_http_request(addr, "GET", "/mc/meta/1.21.1/version.json", &[], None).await;
        assert_eq!(resp_trunc.status, 200);
        assert_eq!(resp_trunc.body.len(), 16);
        let cl: usize = resp_trunc
            .headers
            .get("content-length")
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(cl, 16);
        assert!(cl < normal_ver_len);

        // 6. Test Corrupted SHA-1 payload
        server.inject_corrupt_sha1(Some("/mc/meta/1.21.1/version.json"), 1);
        let resp_corrupt =
            raw_http_request(addr, "GET", "/mc/meta/1.21.1/version.json", &[], None).await;
        assert_eq!(resp_corrupt.status, 200);
        let corrupted_sha1 = sha1_hex(&sha1_bytes(&resp_corrupt.body));
        assert_ne!(
            corrupted_sha1, normal_sha1,
            "Corrupted payload SHA-1 must not match normal SHA-1"
        );

        // 7. Test Invalid Content-Length header mismatch
        server.inject_invalid_content_length(Some("/mc/meta/1.21.1/version.json"), 42, 1);
        let resp_bad_cl =
            raw_http_request(addr, "GET", "/mc/meta/1.21.1/version.json", &[], None).await;
        assert_eq!(resp_bad_cl.status, 200);
        assert_eq!(
            resp_bad_cl
                .headers
                .get("content-length")
                .map(String::as_str),
            Some("42")
        );
        assert_eq!(resp_bad_cl.body.len(), 42);
        assert_ne!(resp_bad_cl.body.len(), normal_ver_len);

        // 8. Test Read-Only directory simulation
        let tmp = tempfile::tempdir().expect("tempdir");
        let test_dir = tmp.path().join("readonly_test");
        std::fs::create_dir_all(&test_dir).expect("create dir");

        let res = with_read_only_dir(&test_dir, |p| {
            // Attempting to write into read-only dir must fail with PermissionDenied
            std::fs::write(p.join("fail.txt"), "hello")
        })
        .expect("guard");
        assert!(res.is_err(), "write in read-only dir must fail");
        assert_eq!(
            res.unwrap_err().kind(),
            std::io::ErrorKind::PermissionDenied
        );

        // Permissions restored after closure: write now succeeds
        std::fs::write(test_dir.join("success.txt"), "hello").expect("write after restore");
        assert!(test_dir.join("success.txt").exists());

        server.shutdown().await;
    }
}
