#![deny(clippy::await_holding_lock)]
//! `mq-net`: Asynchronous HTTP/2 + TLS networking and streaming download engine.
//!
//! Provides bounded concurrency via `tokio::sync::Semaphore` (default 32),
//! retry with exponential backoff and deterministic jitter, HTTP `Range: bytes=<offset>-`
//! (`206 Partial Content`) resume on partial `.part` files, streaming chunk I/O via
//! `mq_sched::BufferPool` (`64 KiB`–`256 KiB`) without whole-file buffering, incremental
//! SHA-1 and SHA-512 verification, O(1) concurrent in-flight deduplication via
//! `mq_sched::InFlightMap`, and epoch-validated atomic `.part` rename.

pub mod config;
pub mod engine;

pub use config::{
    compute_backoff_delay, DownloadOutcome, DownloadSpec, NetConfig, DEFAULT_BASE_BACKOFF_MS,
    DEFAULT_CONNECT_TIMEOUT, DEFAULT_MAX_CONNECTIONS, DEFAULT_MAX_JSON_BYTES, DEFAULT_MAX_RETRIES,
    DEFAULT_READ_TIMEOUT, USER_AGENT,
};
pub use engine::NetEngine;

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use mq_core::{Event, EventBus, MqError, Sha1Digest, Sha512Digest};
    use mq_sched::{BufferPool, CancelEpoch, MIN_CHUNK_SIZE};
    use serde::{Deserialize, Serialize};
    use sha1::{Digest as _, Sha1};
    use sha2::Sha512;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::sync::{Barrier, Notify};

    use super::*;

    fn compute_sha1(data: &[u8]) -> Sha1Digest {
        let mut h = Sha1::new();
        h.update(data);
        Sha1Digest::from_bytes(h.finalize().into())
    }

    fn compute_sha512(data: &[u8]) -> Sha512Digest {
        let mut h = Sha512::new();
        h.update(data);
        Sha512Digest::from_bytes(h.finalize().into())
    }

    /// Reads raw HTTP request headers from a test socket and returns `(request_line, headers_lowercased)`.
    async fn read_http_request(stream: &mut TcpStream) -> (String, Vec<(String, String)>) {
        let mut buf = Vec::with_capacity(1024);
        let mut tmp = [0u8; 512];
        loop {
            let n = stream.read(&mut tmp).await.unwrap_or(0);
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&tmp[..n]);
            if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        let text = String::from_utf8_lossy(&buf);
        let header_section = text.split("\r\n\r\n").next().unwrap_or("");
        let mut lines = header_section.lines();
        let request_line = lines.next().unwrap_or("").to_owned();
        let mut headers = Vec::new();
        for line in lines {
            if let Some((k, v)) = line.split_once(':') {
                headers.push((k.trim().to_ascii_lowercase(), v.trim().to_owned()));
            }
        }
        (request_line, headers)
    }

    #[tokio::test]
    async fn streaming_sha1_and_sha512_verification_and_hash_mismatch_rejection() {
        let payload: Vec<u8> = (0..(300 * 1024))
            .map(|i| ((i * 31 + 17) & 0xff) as u8)
            .collect();
        let expected_sha1 = compute_sha1(&payload);
        let expected_sha512 = compute_sha512(&payload);

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let addr = listener.local_addr().expect("local addr");
        let server_payload = Arc::new(payload.clone());

        let server = tokio::spawn(async move {
            for _ in 0..3 {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let data = Arc::clone(&server_payload);
                tokio::spawn(async move {
                    let (_req_line, headers) = read_http_request(&mut socket).await;
                    let ua = headers
                        .iter()
                        .find(|(k, _)| k == "user-agent")
                        .map(|(_, v)| v.as_str())
                        .unwrap_or("");
                    assert_eq!(ua, USER_AGENT);

                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        data.len()
                    );
                    let _ = socket.write_all(head.as_bytes()).await;
                    let _ = socket.write_all(&data).await;
                    let _ = socket.shutdown().await;
                });
            }
        });

        let tmp = tempfile::tempdir().expect("tempdir");
        let pool = Arc::new(BufferPool::with_limits(MIN_CHUNK_SIZE, 4, 4));
        let epoch = Arc::new(CancelEpoch::new());
        let engine = NetEngine::with_components(
            NetConfig::default().with_max_retries(0),
            Arc::clone(&pool),
            Arc::clone(&epoch),
            None,
        )
        .expect("build engine");

        // 1. Valid SHA-1 + SHA-512 download succeeds and recycles BufferPool chunks.
        let good_dest = tmp.path().join("good.bin");
        let good_spec = DownloadSpec::new(
            "good-artifact",
            format!("http://{addr}/good.bin"),
            &good_dest,
        )
        .with_sha1(expected_sha1)
        .with_sha512(expected_sha512)
        .with_expected_size(payload.len() as u64);

        let guard = epoch.snapshot();
        let outcome = engine
            .download_file(&good_spec, &guard)
            .await
            .expect("good download should succeed");

        assert_eq!(outcome.id, "good-artifact");
        assert_eq!(outcome.bytes_written, payload.len() as u64);
        assert_eq!(outcome.sha1, expected_sha1);
        assert_eq!(outcome.sha512, Some(expected_sha512));
        assert_eq!(outcome.resumed_from_offset, 0);
        assert!(!outcome.deduplicated);
        assert_eq!(std::fs::read(&good_dest).expect("read good file"), payload);
        assert!(pool.stats().allocated_total >= 1);

        // 2. Wrong SHA-1 is strictly rejected with MqError::HashMismatch and deletes .part file.
        let bad_sha1_dest = tmp.path().join("bad_sha1.bin");
        let wrong_sha1 = Sha1Digest::from_bytes([0xaa; 20]);
        let bad_sha1_spec = DownloadSpec::new(
            "bad-sha1-artifact",
            format!("http://{addr}/bad_sha1.bin"),
            &bad_sha1_dest,
        )
        .with_sha1(wrong_sha1)
        .with_expected_size(payload.len() as u64);

        let err = engine
            .download_file(&bad_sha1_spec, &guard)
            .await
            .expect_err("wrong SHA-1 must fail");
        match err {
            MqError::HashMismatch {
                target,
                algo,
                expected,
                actual,
            } => {
                assert_eq!(target, "bad-sha1-artifact");
                assert_eq!(algo, "SHA-1");
                assert_eq!(expected, wrong_sha1.to_hex());
                assert_eq!(actual, expected_sha1.to_hex());
            }
            other => panic!("expected HashMismatch(SHA-1), got {other:?}"),
        }
        assert!(!bad_sha1_dest.exists());
        assert!(!tmp.path().join("bad_sha1.bin.part").exists());
        assert!(!tmp.path().join("bad_sha1.bin.e1.part").exists());

        // 3. Wrong SHA-512 is strictly rejected with MqError::HashMismatch and deletes .part file.
        let bad_sha512_dest = tmp.path().join("bad_sha512.bin");
        let wrong_sha512 = Sha512Digest::from_bytes([0x55; 64]);
        let bad_sha512_spec = DownloadSpec::new(
            "bad-sha512-artifact",
            format!("http://{addr}/bad_sha512.bin"),
            &bad_sha512_dest,
        )
        .with_sha1(expected_sha1)
        .with_sha512(wrong_sha512)
        .with_expected_size(payload.len() as u64);

        let err512 = engine
            .download_file(&bad_sha512_spec, &guard)
            .await
            .expect_err("wrong SHA-512 must fail");
        match err512 {
            MqError::HashMismatch {
                target,
                algo,
                expected,
                actual,
            } => {
                assert_eq!(target, "bad-sha512-artifact");
                assert_eq!(algo, "SHA-512");
                assert_eq!(expected, wrong_sha512.to_hex());
                assert_eq!(actual, expected_sha512.to_hex());
            }
            other => panic!("expected HashMismatch(SHA-512), got {other:?}"),
        }
        assert!(!bad_sha512_dest.exists());
        assert!(!tmp.path().join("bad_sha512.bin.part").exists());
        assert!(!tmp.path().join("bad_sha512.bin.e1.part").exists());

        server.abort();
    }

    #[tokio::test]
    async fn http_206_partial_content_range_resume_from_part_file() {
        let full_payload: Vec<u8> = (0..(200 * 1024))
            .map(|i| ((i * 73 + 9) & 0xff) as u8)
            .collect();
        let split_offset = 75 * 1024;
        let expected_sha1 = compute_sha1(&full_payload);
        let expected_sha512 = compute_sha512(&full_payload);

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let server_data = Arc::new(full_payload.clone());

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let (_req_line, headers) = read_http_request(&mut socket).await;
            let range_val = headers
                .iter()
                .find(|(k, _)| k == "range")
                .map(|(_, v)| v.clone())
                .expect("Range header must be sent for existing .part file");
            assert_eq!(range_val, format!("bytes={split_offset}-"));

            let tail = &server_data[split_offset..];
            let response_head = format!(
                "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {}-{}/{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                split_offset,
                server_data.len() - 1,
                server_data.len(),
                tail.len()
            );
            socket.write_all(response_head.as_bytes()).await.unwrap();
            socket.write_all(tail).await.unwrap();
            let _ = socket.shutdown().await;
        });

        let tmp = tempfile::tempdir().expect("tempdir");
        let dest_path = tmp.path().join("resumed.jar");
        let part_path = tmp.path().join("resumed.jar.part");

        // Pre-populate `.part` file with the first `split_offset` bytes.
        std::fs::write(&part_path, &full_payload[..split_offset]).expect("write .part prefix");

        let engine = NetEngine::with_defaults().expect("engine");
        let guard = engine.cancel_epoch().snapshot();

        let spec = DownloadSpec::new(
            "resumed-jar",
            format!("http://{addr}/resumed.jar"),
            &dest_path,
        )
        .with_sha1(expected_sha1)
        .with_sha512(expected_sha512)
        .with_expected_size(full_payload.len() as u64);

        let outcome = engine
            .download_file(&spec, &guard)
            .await
            .expect("range resume should succeed");

        assert_eq!(outcome.resumed_from_offset, split_offset as u64);
        assert_eq!(outcome.bytes_written, full_payload.len() as u64);
        assert_eq!(outcome.sha1, expected_sha1);
        assert_eq!(outcome.sha512, Some(expected_sha512));
        assert_eq!(
            std::fs::read(&dest_path).expect("read final file"),
            full_payload
        );
        assert!(!part_path.exists());
        assert!(!tmp.path().join("resumed.jar.e1.part").exists());

        server.await.expect("server task");
    }

    #[tokio::test]
    async fn transient_503_failure_recovers_via_retry_and_backoff() {
        let payload = b"recovered-after-transient-503-errors".repeat(256);
        let expected_sha1 = compute_sha1(&payload);

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let attempts = Arc::new(AtomicUsize::new(0));
        let server_attempts = Arc::clone(&attempts);
        let server_payload = Arc::new(payload.clone());

        let server = tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let count = server_attempts.fetch_add(1, Ordering::SeqCst) + 1;
                let data = Arc::clone(&server_payload);
                tokio::spawn(async move {
                    let _ = read_http_request(&mut socket).await;
                    if count < 3 {
                        let body = "temporarily unavailable";
                        let resp = format!(
                            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = socket.write_all(resp.as_bytes()).await;
                    } else {
                        let resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            data.len()
                        );
                        let _ = socket.write_all(resp.as_bytes()).await;
                        let _ = socket.write_all(&data).await;
                    }
                    let _ = socket.shutdown().await;
                });
            }
        });

        let tmp = tempfile::tempdir().expect("tempdir");
        let dest = tmp.path().join("retry.jar");

        let cfg = NetConfig::default()
            .with_max_retries(4)
            .with_base_backoff_ms(10);
        let engine = NetEngine::new(cfg).expect("engine");
        let guard = engine.cancel_epoch().snapshot();

        let spec = DownloadSpec::new("retry-jar", format!("http://{addr}/retry.jar"), &dest)
            .with_sha1(expected_sha1)
            .with_expected_size(payload.len() as u64);

        let outcome = engine
            .download_file(&spec, &guard)
            .await
            .expect("should recover on 3rd attempt");

        assert_eq!(attempts.load(Ordering::SeqCst), 3);
        assert_eq!(outcome.sha1, expected_sha1);
        assert_eq!(std::fs::read(&dest).expect("read file"), payload);

        server.abort();
    }

    #[tokio::test]
    async fn concurrent_inflight_map_deduplication_ten_callers_one_http_request() {
        let payload: Vec<u8> = (0..(128 * 1024)).map(|i| (i % 251) as u8).collect();
        let expected_sha1 = compute_sha1(&payload);

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let http_requests = Arc::new(AtomicUsize::new(0));
        let server_requests = Arc::clone(&http_requests);
        let server_payload = Arc::new(payload.clone());

        let server = tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                server_requests.fetch_add(1, Ordering::SeqCst);
                let data = Arc::clone(&server_payload);
                tokio::spawn(async move {
                    let _ = read_http_request(&mut socket).await;
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        data.len()
                    );
                    let _ = socket.write_all(head.as_bytes()).await;
                    // Stream in two halves with a brief pause so all 10 callers overlap in flight.
                    let mid = data.len() / 2;
                    let _ = socket.write_all(&data[..mid]).await;
                    tokio::time::sleep(Duration::from_millis(40)).await;
                    let _ = socket.write_all(&data[mid..]).await;
                    let _ = socket.shutdown().await;
                });
            }
        });

        let tmp = tempfile::tempdir().expect("tempdir");
        let shared_dest = tmp.path().join("dedup_shared.jar");
        let engine = NetEngine::with_defaults().expect("engine");
        let guard = engine.cancel_epoch().snapshot();
        let barrier = Arc::new(Barrier::new(10));

        let mut handles = Vec::with_capacity(10);
        for idx in 0..10 {
            let eng = engine.clone();
            let g = guard.clone();
            let bar = Arc::clone(&barrier);
            let spec = DownloadSpec::new(
                format!("caller-{idx}"),
                format!("http://{addr}/dedup.jar"),
                &shared_dest,
            )
            .with_sha1(expected_sha1)
            .with_expected_size(payload.len() as u64);

            handles.push(tokio::spawn(async move {
                bar.wait().await;
                eng.download_file(&spec, &g).await
            }));
        }

        let mut leader_count = 0usize;
        let mut follower_count = 0usize;
        for h in handles {
            let outcome = h
                .await
                .expect("task join")
                .expect("deduplicated download must succeed");
            assert_eq!(outcome.sha1, expected_sha1);
            assert_eq!(outcome.bytes_written, payload.len() as u64);
            if outcome.deduplicated {
                follower_count += 1;
            } else {
                leader_count += 1;
            }
        }

        assert_eq!(
            http_requests.load(Ordering::SeqCst),
            1,
            "10 concurrent callers must issue exactly 1 HTTP request"
        );
        assert_eq!(leader_count, 1);
        assert_eq!(follower_count, 9);
        assert!(engine.inflight_map().is_empty());

        server.abort();
    }

    #[tokio::test]
    async fn mid_download_cancellation_via_cancel_epoch_cleans_up_part_files() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let first_chunk_sent = Arc::new(Notify::new());
        let first_chunk_sent_srv = Arc::clone(&first_chunk_sent);

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let _ = read_http_request(&mut socket).await;
            let total_len = 512 * 1024usize;
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {total_len}\r\nConnection: close\r\n\r\n"
            );
            let _ = socket.write_all(head.as_bytes()).await;
            // Send first 64 KiB chunk, then stall so client cancels mid-stream.
            let chunk = vec![0x42u8; 64 * 1024];
            let _ = socket.write_all(&chunk).await;
            let _ = socket.flush().await;
            first_chunk_sent_srv.notify_waiters();
            tokio::time::sleep(Duration::from_secs(5)).await;
        });

        let tmp = tempfile::tempdir().expect("tempdir");
        let dest = tmp.path().join("cancelled.jar");

        let engine = NetEngine::with_defaults().expect("engine");
        let cancel_epoch = Arc::clone(engine.cancel_epoch());
        let guard = cancel_epoch.snapshot();
        assert_eq!(guard.epoch(), 1);

        let spec = DownloadSpec::new(
            "cancel-target",
            format!("http://{addr}/cancelled.jar"),
            &dest,
        )
        .with_expected_size(512 * 1024);

        let eng_clone = engine.clone();
        let guard_clone = guard.clone();
        let dl_task =
            tokio::spawn(async move { eng_clone.download_file(&spec, &guard_clone).await });

        // Wait until first chunk has been sent and written to `.e1.part`.
        first_chunk_sent.notified().await;
        tokio::time::sleep(Duration::from_millis(20)).await;

        let new_epoch = cancel_epoch.cancel_and_advance();
        assert_eq!(new_epoch, 2);

        let res = tokio::time::timeout(Duration::from_secs(1), dl_task)
            .await
            .expect("download task must abort promptly on CancelEpoch advance")
            .expect("task should not panic");

        assert!(matches!(res, Err(MqError::Cancelled { epoch: 1 })));
        assert!(
            !dest.exists(),
            "destination file must not exist after cancel"
        );

        // Verify zero `.part` files remain in temporary directory.
        let remaining_entries: Vec<_> = std::fs::read_dir(tmp.path())
            .expect("read_dir")
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            remaining_entries.is_empty(),
            "all .part files must be cleaned up on cancellation, found: {remaining_entries:?}"
        );

        server.abort();
    }

    #[tokio::test]
    async fn download_batch_preserves_deterministic_order_and_emits_events() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");

        // Reverse response delays: item 0 is slowest (75ms), item 4 is fastest (0ms).
        let server = tokio::spawn(async move {
            for _ in 0..5 {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let (req_line, _) = read_http_request(&mut socket).await;
                    let idx: usize = req_line
                        .split_whitespace()
                        .nth(1)
                        .and_then(|p| p.strip_prefix("/item-"))
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);

                    let delay_ms = (4usize.saturating_sub(idx) as u64) * 18;
                    if delay_ms > 0 {
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    }

                    let body = format!("payload-for-item-{idx}").into_bytes();
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = socket.write_all(head.as_bytes()).await;
                    let _ = socket.write_all(&body).await;
                    let _ = socket.shutdown().await;
                });
            }
        });

        let tmp = tempfile::tempdir().expect("tempdir");
        let bus = EventBus::new();
        let engine = NetEngine::with_defaults()
            .expect("engine")
            .with_event_bus(bus.clone());
        let guard = engine.cancel_epoch().snapshot();

        let mut specs = Vec::with_capacity(5);
        let mut expected_sha1s = Vec::with_capacity(5);
        for idx in 0..5 {
            let body = format!("payload-for-item-{idx}").into_bytes();
            let sha1 = compute_sha1(&body);
            expected_sha1s.push(sha1);
            let dest = tmp.path().join(format!("item-{idx}.bin"));
            specs.push(
                DownloadSpec::new(
                    format!("id-{idx}"),
                    format!("http://{addr}/item-{idx}"),
                    dest,
                )
                .with_sha1(sha1)
                .with_expected_size(body.len() as u64),
            );
        }

        let outcomes = engine
            .download_batch(specs, "test-batch-phase", &guard)
            .await
            .expect("download_batch should succeed");

        assert_eq!(outcomes.len(), 5);
        for (idx, outcome) in outcomes.iter().enumerate() {
            assert_eq!(outcome.id, format!("id-{idx}"));
            assert_eq!(outcome.sha1, expected_sha1s[idx]);
        }

        let events = bus.drain_all();
        assert!(
            matches!(
                events.first(),
                Some(Event::PhaseStarted {
                    phase,
                    total_items: 5
                }) if phase == "test-batch-phase"
            ),
            "first event must be PhaseStarted"
        );
        assert!(
            matches!(
                events.last(),
                Some(Event::PhaseCompleted { phase, .. }) if phase == "test-batch-phase"
            ),
            "last event must be PhaseCompleted"
        );
        let progress_count = events
            .iter()
            .filter(|ev| matches!(ev, Event::Progress(_)))
            .count();
        assert_eq!(progress_count, 5);

        server.abort();
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct SampleManifest {
        name: String,
        version: u32,
    }

    #[tokio::test]
    async fn fetch_json_and_fetch_bytes_small_handle_retries_and_size_caps() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let req_count = Arc::new(AtomicUsize::new(0));
        let srv_count = Arc::clone(&req_count);

        let server = tokio::spawn(async move {
            for _ in 0..3 {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let n = srv_count.fetch_add(1, Ordering::SeqCst) + 1;
                tokio::spawn(async move {
                    let _ = read_http_request(&mut socket).await;
                    if n == 1 {
                        let resp = "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 4\r\nConnection: close\r\n\r\nbusy";
                        let _ = socket.write_all(resp.as_bytes()).await;
                    } else {
                        let body = r#"{"name":"mq-test","version":42}"#;
                        let resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = socket.write_all(resp.as_bytes()).await;
                    }
                    let _ = socket.shutdown().await;
                });
            }
        });

        let engine = NetEngine::new(NetConfig::default().with_base_backoff_ms(5)).expect("engine");
        let parsed: SampleManifest = engine
            .fetch_json(&format!("http://{addr}/manifest.json"))
            .await
            .expect("fetch_json should recover after 503");
        assert_eq!(
            parsed,
            SampleManifest {
                name: "mq-test".to_owned(),
                version: 42,
            }
        );

        // Exceeding max_bytes is rejected cleanly.
        let too_small = engine
            .fetch_bytes_small(&format!("http://{addr}/manifest.json"), 8)
            .await;
        assert!(too_small.is_err());

        server.abort();
    }

    #[tokio::test]
    async fn range_request_falling_back_to_200_ok_truncates_and_restarts_cleanly() {
        let full_payload: Vec<u8> = (0..(96 * 1024))
            .map(|i| ((i * 19 + 3) & 0xff) as u8)
            .collect();
        let expected_sha1 = compute_sha1(&full_payload);

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let server_data = Arc::new(full_payload.clone());

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let (_req_line, headers) = read_http_request(&mut socket).await;
            assert!(
                headers.iter().any(|(k, _)| k == "range"),
                "client should send Range header when .part exists"
            );

            // Server ignores Range and returns 200 OK with the full payload.
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                server_data.len()
            );
            socket.write_all(head.as_bytes()).await.unwrap();
            socket.write_all(&server_data).await.unwrap();
            let _ = socket.shutdown().await;
        });

        let tmp = tempfile::tempdir().expect("tempdir");
        let dest_path = tmp.path().join("fallback200.jar");
        let part_path = tmp.path().join("fallback200.jar.part");

        // Write garbage prefix into .part; 200 OK must truncate and overwrite from 0.
        std::fs::write(&part_path, vec![0xffu8; 16 * 1024]).expect("write stale .part");

        let engine = NetEngine::with_defaults().expect("engine");
        let guard = engine.cancel_epoch().snapshot();
        let spec = DownloadSpec::new(
            "fallback-200",
            format!("http://{addr}/fallback200.jar"),
            &dest_path,
        )
        .with_sha1(expected_sha1)
        .with_expected_size(full_payload.len() as u64);

        let outcome = engine
            .download_file(&spec, &guard)
            .await
            .expect("200 OK fallback should succeed");

        assert_eq!(outcome.resumed_from_offset, 0);
        assert_eq!(outcome.bytes_written, full_payload.len() as u64);
        assert_eq!(outcome.sha1, expected_sha1);
        assert_eq!(std::fs::read(&dest_path).expect("read final"), full_payload);

        server.await.expect("server");
    }

    #[tokio::test]
    async fn fallback_mirror_url_and_existing_file_fast_path_work() {
        let payload = b"mirror-fallback-and-fast-path-payload".repeat(128);
        let expected_sha1 = compute_sha1(&payload);

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let req_counter = Arc::new(AtomicUsize::new(0));
        let srv_counter = Arc::clone(&req_counter);
        let srv_payload = Arc::new(payload.clone());

        let server = tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                srv_counter.fetch_add(1, Ordering::SeqCst);
                let data = Arc::clone(&srv_payload);
                tokio::spawn(async move {
                    let (req_line, _) = read_http_request(&mut socket).await;
                    if req_line.contains("/primary-broken") {
                        let resp =
                            "HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\nConnection: close\r\n\r\nnot found";
                        let _ = socket.write_all(resp.as_bytes()).await;
                    } else {
                        let head = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            data.len()
                        );
                        let _ = socket.write_all(head.as_bytes()).await;
                        let _ = socket.write_all(&data).await;
                    }
                    let _ = socket.shutdown().await;
                });
            }
        });

        let tmp = tempfile::tempdir().expect("tempdir");
        let dest = tmp.path().join("mirror.jar");
        let engine = NetEngine::new(NetConfig::default().with_max_retries(0)).expect("engine");
        let guard = engine.cancel_epoch().snapshot();

        let spec = DownloadSpec::new(
            "mirror-test",
            format!("http://{addr}/primary-broken"),
            &dest,
        )
        .with_fallback_url(format!("http://{addr}/mirror-working"))
        .with_sha1(expected_sha1)
        .with_expected_size(payload.len() as u64);

        let first = engine
            .download_file(&spec, &guard)
            .await
            .expect("fallback mirror should succeed");
        assert_eq!(first.sha1, expected_sha1);
        assert_eq!(req_counter.load(Ordering::SeqCst), 2);

        // Second call with already verified destination file hits the fast path (0 additional HTTP requests).
        let second = engine
            .download_file(&spec, &guard)
            .await
            .expect("fast path should succeed");
        assert_eq!(second.sha1, expected_sha1);
        assert_eq!(req_counter.load(Ordering::SeqCst), 2);

        server.abort();
    }
}
