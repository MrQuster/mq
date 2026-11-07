#![deny(clippy::await_holding_lock)]
//! Asynchronous streaming HTTP download engine with bounded concurrency, retry + jitter backoff,
//! HTTP `206 Partial Content` range resume, `InFlightMap` deduplication, `BufferPool` streaming,
//! incremental SHA-1/SHA-512 verification, and `CancelEpoch` protection.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use bytes::{Bytes, BytesMut};
use dashmap::DashMap;
use mq_core::{Event, EventBus, MqError, MqResult, PhaseProgress, Sha1Digest, Sha512Digest};
use mq_sched::{
    wait_for_inflight, AtomicFileState, BufferPool, CancelEpoch, EpochGuard, FileState,
    InFlightMap, InFlightResult,
};
use reqwest::header::RANGE;
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use sha1::{Digest as _, Sha1};
use sha2::Sha512;
use tokio::fs::{self, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::config::{
    compute_backoff_delay_with_retry_after, parse_retry_after, DownloadOutcome, DownloadSpec,
    NetConfig, DEFAULT_MAX_JSON_BYTES,
};

/// Maximum bytes of an HTTP error body captured into [`MqError::HttpStatus`].
const MAX_ERROR_BODY_PREVIEW: usize = 4096;

/// RAII guard ensuring temporary `.part` files are removed on error, cancellation, or task abort.
struct PartFileCleanupGuard {
    epoch_part: PathBuf,
    plain_part: PathBuf,
    armed: bool,
}

impl PartFileCleanupGuard {
    fn new(epoch_part: PathBuf, plain_part: PathBuf) -> Self {
        Self {
            epoch_part,
            plain_part,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for PartFileCleanupGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.epoch_part);
            let _ = std::fs::remove_file(&self.plain_part);
        }
    }
}

/// Asynchronous HTTP networking and streaming download engine.
///
/// Cloning [`NetEngine`] is cheap (`O(1)` reference-counted handles) and shares the
/// underlying connection pool, concurrency `Semaphore`, `BufferPool`, `InFlightMap`,
/// and `CancelEpoch`.
#[derive(Debug, Clone)]
pub struct NetEngine {
    client: reqwest::Client,
    config: NetConfig,
    semaphore: Arc<Semaphore>,
    buffer_pool: Arc<BufferPool>,
    inflight: Arc<InFlightMap<String>>,
    completed_by_sha1: Arc<DashMap<Sha1Digest, PathBuf>>,
    cancel_epoch: Arc<CancelEpoch>,
    event_bus: Option<EventBus>,
    compute_sha512: bool,
}

impl NetEngine {
    /// Creates a new [`NetEngine`] with the provided [`NetConfig`] and fresh default
    /// `BufferPool`, `InFlightMap`, and `CancelEpoch`.
    pub fn new(config: NetConfig) -> MqResult<Self> {
        Self::with_components(
            config,
            Arc::new(BufferPool::new()),
            Arc::new(CancelEpoch::new()),
            None,
        )
    }

    /// Creates a new [`NetEngine`] using [`NetConfig::default()`].
    pub fn with_defaults() -> MqResult<Self> {
        Self::new(NetConfig::default())
    }

    /// Creates a [`NetEngine`] wired to shared `BufferPool`, `CancelEpoch`, and optional `EventBus`.
    pub fn with_components(
        config: NetConfig,
        buffer_pool: Arc<BufferPool>,
        cancel_epoch: Arc<CancelEpoch>,
        event_bus: Option<EventBus>,
    ) -> MqResult<Self> {
        let max_conn = config.max_connections.max(1);
        let client = reqwest::Client::builder()
            .user_agent(&config.user_agent)
            .connect_timeout(config.connect_timeout)
            .read_timeout(config.read_timeout)
            .pool_max_idle_per_host(max_conn)
            .gzip(true)
            .build()
            .map_err(|err| MqError::Network(format!("failed to build HTTP client: {err}")))?;

        Ok(Self {
            client,
            config: NetConfig {
                max_connections: max_conn,
                ..config
            },
            semaphore: Arc::new(Semaphore::new(max_conn)),
            buffer_pool,
            inflight: Arc::new(InFlightMap::new()),
            completed_by_sha1: Arc::new(DashMap::new()),
            cancel_epoch,
            event_bus,
            compute_sha512: false,
        })
    }

    /// Replaces the shared [`BufferPool`].
    #[must_use]
    pub fn with_buffer_pool(mut self, buffer_pool: Arc<BufferPool>) -> Self {
        self.buffer_pool = buffer_pool;
        self
    }

    /// Replaces the shared [`CancelEpoch`].
    #[must_use]
    pub fn with_cancel_epoch(mut self, cancel_epoch: Arc<CancelEpoch>) -> Self {
        self.cancel_epoch = cancel_epoch;
        self
    }

    /// Attaches an [`EventBus`] for publishing phase lifecycle and progress events.
    #[must_use]
    pub fn with_event_bus(mut self, event_bus: EventBus) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    /// Sets or clears the optional [`EventBus`].
    #[must_use]
    pub fn with_optional_event_bus(mut self, event_bus: Option<EventBus>) -> Self {
        self.event_bus = event_bus;
        self
    }

    /// Replaces the shared [`InFlightMap`].
    #[must_use]
    pub fn with_inflight_map(mut self, inflight: Arc<InFlightMap<String>>) -> Self {
        self.inflight = inflight;
        self
    }

    /// Configures whether SHA-512 digests should always be computed even when
    /// `DownloadSpec::expected_sha512` is `None`.
    #[must_use]
    pub fn with_compute_sha512(mut self, compute_sha512: bool) -> Self {
        self.compute_sha512 = compute_sha512;
        self
    }

    /// Borrows the underlying `reqwest::Client`.
    #[must_use]
    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    /// Borrows the active [`NetConfig`].
    #[must_use]
    pub fn config(&self) -> &NetConfig {
        &self.config
    }

    /// Borrows the connection concurrency [`Semaphore`].
    #[must_use]
    pub fn semaphore(&self) -> &Arc<Semaphore> {
        &self.semaphore
    }

    /// Borrows the shared [`BufferPool`].
    #[must_use]
    pub fn buffer_pool(&self) -> &Arc<BufferPool> {
        &self.buffer_pool
    }

    /// Borrows the shared [`InFlightMap`].
    #[must_use]
    pub fn inflight_map(&self) -> &Arc<InFlightMap<String>> {
        &self.inflight
    }

    /// Borrows the shared [`CancelEpoch`].
    #[must_use]
    pub fn cancel_epoch(&self) -> &Arc<CancelEpoch> {
        &self.cancel_epoch
    }

    /// Borrows the optional [`EventBus`].
    #[must_use]
    pub fn event_bus(&self) -> Option<&EventBus> {
        self.event_bus.as_ref()
    }

    /// Fetches and deserializes a JSON document from `url` with bounded concurrency
    /// and exponential backoff + jitter on transient failures.
    pub async fn fetch_json<T: DeserializeOwned>(&self, url: &str) -> MqResult<T> {
        let bytes = self.fetch_bytes_small(url, DEFAULT_MAX_JSON_BYTES).await?;
        serde_json::from_slice::<T>(&bytes).map_err(MqError::Json)
    }

    /// Fetches a small response body (strictly capped at `max_bytes`) into [`Bytes`]
    /// with bounded concurrency and exponential backoff + jitter on transient failures.
    pub async fn fetch_bytes_small(&self, url: &str, max_bytes: usize) -> MqResult<Bytes> {
        let epoch_guard = self.cancel_epoch.snapshot();
        self.check_epoch(&epoch_guard)?;

        let _permit = tokio::select! {
            biased;
            _ = epoch_guard.cancelled() => {
                return Err(MqError::Cancelled { epoch: epoch_guard.epoch() });
            }
            res = self.semaphore.acquire() => {
                res.map_err(|_| MqError::Internal("NetEngine semaphore closed".to_owned()))?
            }
        };

        let mut last_err: Option<MqError> = None;

        for attempt in 0..=self.config.max_retries {
            self.check_epoch(&epoch_guard)?;

            let mut retry_after: Option<std::time::Duration> = None;
            match self
                .fetch_bytes_once(url, max_bytes, &epoch_guard, &mut retry_after)
                .await
            {
                Ok(bytes) => return Ok(bytes),
                Err(err) => {
                    if err.is_cancelled()
                        || !err.is_retryable()
                        || attempt == self.config.max_retries
                    {
                        return Err(err);
                    }
                    tracing::debug!(
                        url = %url,
                        attempt = attempt + 1,
                        max_retries = self.config.max_retries,
                        error = %err,
                        "retrying small fetch after transient error"
                    );
                    let delay = compute_backoff_delay_with_retry_after(
                        attempt,
                        self.config.base_backoff_ms,
                        url,
                        retry_after,
                    );
                    last_err = Some(err);
                    self.sleep_with_cancel(delay, &epoch_guard).await?;
                }
            }
        }

        Err(last_err
            .unwrap_or_else(|| MqError::Network(format!("exhausted retries fetching {url}"))))
    }

    async fn fetch_bytes_once(
        &self,
        url: &str,
        max_bytes: usize,
        epoch_guard: &EpochGuard,
        retry_after_out: &mut Option<std::time::Duration>,
    ) -> MqResult<Bytes> {
        let mut response = tokio::select! {
            biased;
            _ = epoch_guard.cancelled() => {
                return Err(MqError::Cancelled { epoch: epoch_guard.epoch() });
            }
            res = self.client.get(url).send() => {
                res.map_err(|err| MqError::Network(format!("GET {url} failed: {err}")))?
            }
        };

        let status = response.status();
        if !status.is_success() {
            *retry_after_out = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(parse_retry_after);
            let body = read_error_body_preview(&mut response, epoch_guard).await;
            return Err(MqError::HttpStatus {
                status: status.as_u16(),
                url: url.to_owned(),
                body,
            });
        }

        if let Some(content_len) = response.content_length() {
            if content_len > max_bytes as u64 {
                return Err(MqError::Metadata(format!(
                    "response Content-Length ({content_len} bytes) exceeds max_bytes ({max_bytes}) for {url}"
                )));
            }
        }

        let initial_cap = response
            .content_length()
            .map_or(4096, |len| (len as usize).min(max_bytes).min(64 * 1024));
        let mut buf = BytesMut::with_capacity(initial_cap);

        loop {
            self.check_epoch(epoch_guard)?;
            let next_chunk = tokio::select! {
                biased;
                _ = epoch_guard.cancelled() => {
                    return Err(MqError::Cancelled { epoch: epoch_guard.epoch() });
                }
                res = tokio::time::timeout(self.config.read_timeout, response.chunk()) => {
                    match res {
                        Ok(Ok(chunk_opt)) => chunk_opt,
                        Ok(Err(err)) => {
                            return Err(MqError::Network(format!("error reading chunk from {url}: {err}")));
                        }
                        Err(_) => {
                            return Err(MqError::Network(format!("read timeout from {url}")));
                        }
                    }
                }
            };

            let Some(chunk) = next_chunk else {
                break;
            };

            if buf.len().saturating_add(chunk.len()) > max_bytes {
                return Err(MqError::Metadata(format!(
                    "response body exceeded max_bytes ({max_bytes}) for {url}"
                )));
            }
            buf.extend_from_slice(&chunk);
        }

        self.check_epoch(epoch_guard)?;
        Ok(buf.freeze())
    }

    /// Streams a file download to disk with `InFlightMap` deduplication, HTTP `206 Partial Content`
    /// resume, `BufferPool` chunk streaming, incremental SHA-1/SHA-512 verification, and atomic
    /// epoch-validated rename.
    pub async fn download_file(
        &self,
        spec: &DownloadSpec,
        epoch_guard: &EpochGuard,
    ) -> MqResult<DownloadOutcome> {
        self.check_epoch(epoch_guard)?;

        if spec.urls.is_empty() {
            return Err(MqError::Network(format!(
                "download spec '{}' has no URLs",
                spec.id
            )));
        }

        // 1. Fast-path if destination file already exists and matches size & digests.
        if let Some(existing_outcome) = self
            .try_verify_existing_file(spec, epoch_guard, false)
            .await?
        {
            return Ok(existing_outcome);
        }

        // 2. O(1) in-flight deduplication via InFlightMap.
        let dedup_key = compute_dedup_key(spec);
        match self.inflight.acquire(dedup_key) {
            InFlightResult::Follower(mut rx) => {
                let wait_res = tokio::select! {
                    biased;
                    _ = epoch_guard.cancelled() => {
                        return Err(MqError::Cancelled { epoch: epoch_guard.epoch() });
                    }
                    res = wait_for_inflight(&mut rx) => res,
                };

                self.check_epoch(epoch_guard)?;

                if let Err(leader_err) = wait_res {
                    return Err(MqError::Network(format!(
                        "in-flight leader failed for '{}': {leader_err}",
                        spec.id
                    )));
                }

                // If the leader wrote to a different destination path for the same SHA-1,
                // materialize our destination path from the leader's verified file.
                if !spec.dest_path.exists() {
                    if let Some(expected_sha1) = spec.expected_sha1 {
                        let source_opt = {
                            self.completed_by_sha1
                                .get(&expected_sha1)
                                .map(|entry| entry.value().clone())
                        };
                        if let Some(source_path) = source_opt {
                            if source_path != spec.dest_path && source_path.exists() {
                                self.materialize_from_existing(
                                    &source_path,
                                    &spec.dest_path,
                                    epoch_guard,
                                )
                                .await?;
                            }
                        }
                    }
                }

                if let Some(outcome) = self
                    .try_verify_existing_file(spec, epoch_guard, true)
                    .await?
                {
                    return Ok(outcome);
                }

                Err(MqError::Network(format!(
                    "in-flight leader completed for '{}', but destination '{}' was not materialized",
                    spec.id,
                    spec.dest_path.display()
                )))
            }
            InFlightResult::Leader(leader_guard) => {
                // Re-check destination in case a prior leader finished right before we acquired leadership.
                if let Some(existing_outcome) = self
                    .try_verify_existing_file(spec, epoch_guard, true)
                    .await?
                {
                    leader_guard.complete_ok();
                    return Ok(existing_outcome);
                }

                match self.download_as_leader(spec, epoch_guard).await {
                    Ok(outcome) => {
                        {
                            self.completed_by_sha1
                                .insert(outcome.sha1, spec.dest_path.clone());
                        }
                        leader_guard.complete_ok();
                        Ok(outcome)
                    }
                    Err(err) => {
                        leader_guard.complete_err(err.to_string());
                        Err(err)
                    }
                }
            }
        }
    }

    /// Executes a batch of downloads concurrently (bounded by `Semaphore(max_connections)`)
    /// using `tokio::task::JoinSet`, preserving deterministic output ordering matching `specs`
    /// (Lesson E8) and publishing lifecycle/progress events to `EventBus`.
    pub async fn download_batch(
        &self,
        specs: Vec<DownloadSpec>,
        phase_name: &str,
        epoch_guard: &EpochGuard,
    ) -> MqResult<Vec<DownloadOutcome>> {
        if let Err(err) = self.check_epoch(epoch_guard) {
            if let Some(bus) = &self.event_bus {
                bus.publish(Event::Cancelled {
                    epoch: epoch_guard.epoch(),
                });
            }
            return Err(err);
        }

        let started_at = Instant::now();
        let total_items = specs.len() as u64;
        let bytes_total: u64 = specs.iter().filter_map(|s| s.expected_size).sum();

        if let Some(bus) = &self.event_bus {
            bus.publish(Event::PhaseStarted {
                phase: phase_name.to_owned(),
                total_items,
            });
        }

        if specs.is_empty() {
            if let Some(bus) = &self.event_bus {
                bus.publish(Event::PhaseCompleted {
                    phase: phase_name.to_owned(),
                    elapsed_ms: 0,
                });
            }
            return Ok(Vec::new());
        }

        let mut ordered_outcomes: Vec<Option<DownloadOutcome>> = vec![None; specs.len()];
        let mut join_set: JoinSet<(usize, MqResult<DownloadOutcome>)> = JoinSet::new();

        for (idx, spec) in specs.into_iter().enumerate() {
            let engine = self.clone();
            let guard = epoch_guard.clone();
            join_set.spawn(async move {
                let res = engine.download_file(&spec, &guard).await;
                (idx, res)
            });
        }

        let mut completed_items = 0u64;
        let mut bytes_done = 0u64;

        while let Some(join_res) = join_set.join_next().await {
            let (idx, task_res) = match join_res {
                Ok(pair) => pair,
                Err(join_err) => {
                    join_set.abort_all();
                    while join_set.join_next().await.is_some() {}
                    let err = MqError::Internal(format!(
                        "download_batch worker task failed in phase '{phase_name}': {join_err}"
                    ));
                    if let Some(bus) = &self.event_bus {
                        bus.publish(Event::TaskError {
                            context: phase_name.to_owned(),
                            message: err.to_string(),
                        });
                    }
                    return Err(err);
                }
            };

            match task_res {
                Ok(outcome) => {
                    completed_items += 1;
                    bytes_done = bytes_done.saturating_add(outcome.bytes_written);
                    ordered_outcomes[idx] = Some(outcome);

                    if let Some(bus) = &self.event_bus {
                        let elapsed_secs = started_at.elapsed().as_secs_f64().max(1e-6);
                        let files_per_sec = completed_items as f64 / elapsed_secs;
                        let mib_per_sec = (bytes_done as f64 / (1024.0 * 1024.0)) / elapsed_secs;
                        let active_tasks = join_set.len().min(self.config.max_connections);

                        bus.publish(Event::Progress(PhaseProgress {
                            phase: phase_name.to_owned(),
                            completed_items,
                            total_items,
                            bytes_done,
                            bytes_total: bytes_total.max(bytes_done),
                            files_per_sec,
                            mib_per_sec,
                            active_tasks,
                        }));
                    }
                }
                Err(err) => {
                    join_set.abort_all();
                    while join_set.join_next().await.is_some() {}
                    if let Some(bus) = &self.event_bus {
                        if err.is_cancelled() {
                            bus.publish(Event::Cancelled {
                                epoch: epoch_guard.epoch(),
                            });
                        } else {
                            bus.publish(Event::TaskError {
                                context: phase_name.to_owned(),
                                message: err.to_string(),
                            });
                        }
                    }
                    return Err(err);
                }
            }
        }

        self.check_epoch(epoch_guard)?;

        if let Some(bus) = &self.event_bus {
            bus.publish(Event::PhaseCompleted {
                phase: phase_name.to_owned(),
                elapsed_ms: started_at.elapsed().as_millis() as u64,
            });
        }

        let results = ordered_outcomes
            .into_iter()
            .map(|opt| opt.expect("all download_batch tasks completed"))
            .collect();
        Ok(results)
    }

    async fn download_as_leader(
        &self,
        spec: &DownloadSpec,
        epoch_guard: &EpochGuard,
    ) -> MqResult<DownloadOutcome> {
        self.check_epoch(epoch_guard)?;

        let _permit = tokio::select! {
            biased;
            _ = epoch_guard.cancelled() => {
                return Err(MqError::Cancelled { epoch: epoch_guard.epoch() });
            }
            res = self.semaphore.acquire() => {
                res.map_err(|_| MqError::Internal("NetEngine semaphore closed".to_owned()))?
            }
        };

        if let Some(parent) = spec.dest_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).await?;
            }
        }

        let plain_part_path = make_part_path(&spec.dest_path, None);
        let epoch_part_path = make_part_path(&spec.dest_path, Some(epoch_guard.epoch()));
        let mut cleanup_guard =
            PartFileCleanupGuard::new(epoch_part_path.clone(), plain_part_path.clone());

        let file_state = AtomicFileState::new(FileState::Pending);
        let mut last_err: Option<MqError> = None;

        for attempt in 0..=self.config.max_retries {
            self.check_epoch(epoch_guard)?;

            if attempt > 0 {
                file_state.store(FileState::Pending);
            }

            if file_state
                .try_transition(
                    FileState::Pending,
                    FileState::Downloading,
                    epoch_guard.epoch(),
                    self.cancel_epoch.atomic_epoch(),
                )
                .is_err()
            {
                cleanup_part_files(&epoch_part_path, &plain_part_path).await;
                return Err(MqError::Cancelled {
                    epoch: epoch_guard.epoch(),
                });
            }

            let mut last_retry_after: Option<std::time::Duration> = None;

            for (url_idx, url) in spec.urls.iter().enumerate() {
                // Additional fallback mirrors require official expected SHA-1 digest.
                if url_idx > 0 && spec.expected_sha1.is_none() {
                    tracing::warn!(
                        id = %spec.id,
                        url = %url,
                        "skipping fallback mirror: mirrors require official expected SHA-1 digest"
                    );
                    continue;
                }

                if url_idx > 0 && file_state.load() != FileState::Downloading {
                    file_state.store(FileState::Pending);
                    if file_state
                        .try_transition(
                            FileState::Pending,
                            FileState::Downloading,
                            epoch_guard.epoch(),
                            self.cancel_epoch.atomic_epoch(),
                        )
                        .is_err()
                    {
                        cleanup_part_files(&epoch_part_path, &plain_part_path).await;
                        return Err(MqError::Cancelled {
                            epoch: epoch_guard.epoch(),
                        });
                    }
                }

                match self
                    .try_download_from_url(
                        url,
                        spec,
                        epoch_guard,
                        &file_state,
                        &epoch_part_path,
                        &plain_part_path,
                        &mut last_retry_after,
                    )
                    .await
                {
                    Ok(outcome) => {
                        cleanup_guard.disarm();
                        return Ok(outcome);
                    }
                    Err(err) => {
                        if err.is_cancelled() {
                            cleanup_part_files(&epoch_part_path, &plain_part_path).await;
                            return Err(err);
                        }
                        tracing::debug!(
                            id = %spec.id,
                            url = %url,
                            attempt = attempt + 1,
                            error = %err,
                            "download attempt failed"
                        );
                        last_err = Some(err);
                    }
                }
            }

            let err = last_err
                .take()
                .unwrap_or_else(|| MqError::Network(format!("download failed for '{}'", spec.id)));

            if !err.is_retryable() || attempt == self.config.max_retries {
                cleanup_part_files(&epoch_part_path, &plain_part_path).await;
                return Err(err);
            }

            let delay = compute_backoff_delay_with_retry_after(
                attempt,
                self.config.base_backoff_ms,
                &spec.id,
                last_retry_after.take(),
            );
            if let Err(cancel_err) = self.sleep_with_cancel(delay, epoch_guard).await {
                cleanup_part_files(&epoch_part_path, &plain_part_path).await;
                return Err(cancel_err);
            }
            last_err = Some(err);
        }

        cleanup_part_files(&epoch_part_path, &plain_part_path).await;
        Err(last_err.unwrap_or_else(|| {
            MqError::Network(format!("exhausted retries downloading '{}'", spec.id))
        }))
    }

    async fn try_download_from_url(
        &self,
        url: &str,
        spec: &DownloadSpec,
        epoch_guard: &EpochGuard,
        file_state: &AtomicFileState,
        epoch_part_path: &Path,
        plain_part_path: &Path,
        retry_after_out: &mut Option<std::time::Duration>,
    ) -> MqResult<DownloadOutcome> {
        self.check_epoch(epoch_guard)?;

        // Adopt `<dest>.part` into `<dest>.e<epoch>.part` if present so HTTP Range resume works
        // seamlessly while maintaining epoch isolation.
        if !epoch_part_path.exists() && plain_part_path.exists() {
            let _ = fs::rename(plain_part_path, epoch_part_path).await;
        }

        let mut existing_offset = match fs::metadata(epoch_part_path).await {
            Ok(meta) if meta.is_file() => meta.len(),
            _ => 0,
        };

        if let Some(expected_size) = spec.expected_size {
            if existing_offset >= expected_size {
                let _ = fs::remove_file(epoch_part_path).await;
                existing_offset = 0;
            }
        }

        let mut req = self.client.get(url);
        if existing_offset > 0 {
            req = req.header(RANGE, format!("bytes={existing_offset}-"));
        }

        let mut response = tokio::select! {
            biased;
            _ = epoch_guard.cancelled() => {
                cleanup_part_files(epoch_part_path, plain_part_path).await;
                return Err(MqError::Cancelled { epoch: epoch_guard.epoch() });
            }
            res = req.send() => {
                res.map_err(|err| MqError::Network(format!("GET {url} failed: {err}")))?
            }
        };

        let status = response.status();
        if status == StatusCode::RANGE_NOT_SATISFIABLE {
            let _ = fs::remove_file(epoch_part_path).await;
            return Err(MqError::Network(format!(
                "HTTP 416 Range Not Satisfiable for {url}; reset partial file"
            )));
        }

        if !status.is_success() {
            *retry_after_out = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(parse_retry_after);
            let body = read_error_body_preview(&mut response, epoch_guard).await;
            let _ = file_state.try_transition(
                FileState::Downloading,
                FileState::Failed,
                epoch_guard.epoch(),
                self.cancel_epoch.atomic_epoch(),
            );
            return Err(MqError::HttpStatus {
                status: status.as_u16(),
                url: url.to_owned(),
                body,
            });
        }

        // Lease a reusable chunk buffer from BufferPool (64 KiB - 256 KiB).
        let mut pooled_buf = tokio::select! {
            biased;
            _ = epoch_guard.cancelled() => {
                cleanup_part_files(epoch_part_path, plain_part_path).await;
                return Err(MqError::Cancelled { epoch: epoch_guard.epoch() });
            }
            buf = self.buffer_pool.acquire() => buf,
        };
        let buf_cap = pooled_buf.len();

        let need_sha512 = self.compute_sha512 || spec.expected_sha512.is_some();
        let mut sha1_hasher = Sha1::new();
        let mut sha512_hasher = need_sha512.then(Sha512::new);

        let (mut file, resumed_from_offset, mut total_bytes_on_disk) =
            if status == StatusCode::PARTIAL_CONTENT && existing_offset > 0 {
                // Feed already-downloaded prefix from `.part` through PooledBuffer into hashers.
                let mut existing_file = fs::File::open(epoch_part_path).await?;
                let mut hashed_prefix = 0u64;
                loop {
                    self.check_epoch(epoch_guard)?;
                    let n = existing_file.read(pooled_buf.as_mut_slice()).await?;
                    if n == 0 {
                        break;
                    }
                    let slice = &pooled_buf.as_slice()[..n];
                    sha1_hasher.update(slice);
                    if let Some(ref mut h512) = sha512_hasher {
                        h512.update(slice);
                    }
                    hashed_prefix += n as u64;
                }
                drop(existing_file);

                if hashed_prefix != existing_offset {
                    let _ = fs::remove_file(epoch_part_path).await;
                    return Err(MqError::Network(format!(
                        "partial file size changed during resume for '{}'",
                        spec.id
                    )));
                }

                let f = OpenOptions::new()
                    .create(true)
                    .write(true)
                    .append(true)
                    .open(epoch_part_path)
                    .await?;
                (f, existing_offset, existing_offset)
            } else {
                // Server returned 200 OK (or no partial offset existed): truncate and restart from 0.
                let f = OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(epoch_part_path)
                    .await?;
                (f, 0u64, 0u64)
            };

        loop {
            if epoch_guard.is_cancelled() || !self.cancel_epoch.is_valid(epoch_guard.epoch()) {
                drop(file);
                cleanup_part_files(epoch_part_path, plain_part_path).await;
                return Err(MqError::Cancelled {
                    epoch: epoch_guard.epoch(),
                });
            }

            let next_chunk = tokio::select! {
                biased;
                _ = epoch_guard.cancelled() => {
                    drop(file);
                    cleanup_part_files(epoch_part_path, plain_part_path).await;
                    return Err(MqError::Cancelled { epoch: epoch_guard.epoch() });
                }
                res = tokio::time::timeout(self.config.read_timeout, response.chunk()) => {
                    match res {
                        Ok(Ok(chunk_opt)) => chunk_opt,
                        Ok(Err(err)) => {
                            let _ = file.flush().await;
                            drop(file);
                            let _ = file_state.try_transition(
                                FileState::Downloading,
                                FileState::Failed,
                                epoch_guard.epoch(),
                                self.cancel_epoch.atomic_epoch(),
                            );
                            return Err(MqError::Network(format!(
                                "stream chunk error for {url}: {err}"
                            )));
                        }
                        Err(_) => {
                            let _ = file.flush().await;
                            drop(file);
                            let _ = file_state.try_transition(
                                FileState::Downloading,
                                FileState::Failed,
                                epoch_guard.epoch(),
                                self.cancel_epoch.atomic_epoch(),
                            );
                            return Err(MqError::Network(format!(
                                "stream read timeout for {url}"
                            )));
                        }
                    }
                }
            };

            let Some(chunk) = next_chunk else {
                break;
            };

            for subslice in chunk.chunks(buf_cap) {
                if epoch_guard.is_cancelled() || !self.cancel_epoch.is_valid(epoch_guard.epoch()) {
                    drop(file);
                    cleanup_part_files(epoch_part_path, plain_part_path).await;
                    return Err(MqError::Cancelled {
                        epoch: epoch_guard.epoch(),
                    });
                }

                pooled_buf.as_mut_slice()[..subslice.len()].copy_from_slice(subslice);
                let dst = &pooled_buf.as_slice()[..subslice.len()];
                sha1_hasher.update(dst);
                if let Some(ref mut h512) = sha512_hasher {
                    h512.update(dst);
                }
                file.write_all(dst).await?;
                total_bytes_on_disk += dst.len() as u64;
            }
        }

        // Release PooledBuffer back to the pool as soon as streaming completes.
        drop(pooled_buf);

        file.flush().await?;
        file.sync_data().await?;
        drop(file);

        // Transition Downloading -> Verifying guarded by CancelEpoch.
        if file_state
            .try_transition(
                FileState::Downloading,
                FileState::Verifying,
                epoch_guard.epoch(),
                self.cancel_epoch.atomic_epoch(),
            )
            .is_err()
        {
            cleanup_part_files(epoch_part_path, plain_part_path).await;
            return Err(MqError::Cancelled {
                epoch: epoch_guard.epoch(),
            });
        }

        // Verify expected_size if specified.
        if let Some(expected_size) = spec.expected_size {
            if total_bytes_on_disk != expected_size {
                let _ = file_state.try_transition(
                    FileState::Verifying,
                    FileState::Failed,
                    epoch_guard.epoch(),
                    self.cancel_epoch.atomic_epoch(),
                );
                cleanup_part_files(epoch_part_path, plain_part_path).await;
                return Err(MqError::Network(format!(
                    "size mismatch for '{}': expected {expected_size} bytes, got {total_bytes_on_disk} bytes",
                    spec.id
                )));
            }
        }

        let actual_sha1_bytes: [u8; 20] = sha1_hasher.finalize().into();
        let actual_sha1 = Sha1Digest::from_bytes(actual_sha1_bytes);

        if let Some(expected_sha1) = spec.expected_sha1 {
            if actual_sha1 != expected_sha1 {
                let _ = file_state.try_transition(
                    FileState::Verifying,
                    FileState::Failed,
                    epoch_guard.epoch(),
                    self.cancel_epoch.atomic_epoch(),
                );
                cleanup_part_files(epoch_part_path, plain_part_path).await;
                return Err(MqError::HashMismatch {
                    target: spec.id.clone(),
                    algo: "SHA-1",
                    expected: expected_sha1.to_hex(),
                    actual: actual_sha1.to_hex(),
                });
            }
        }

        let actual_sha512 = sha512_hasher.map(|h| {
            let bytes: [u8; 64] = h.finalize().into();
            Sha512Digest::from_bytes(bytes)
        });

        if let Some(expected_sha512) = spec.expected_sha512 {
            let actual = actual_sha512.expect("SHA-512 computed when expected_sha512 is Some");
            if actual != expected_sha512 {
                let _ = file_state.try_transition(
                    FileState::Verifying,
                    FileState::Failed,
                    epoch_guard.epoch(),
                    self.cancel_epoch.atomic_epoch(),
                );
                cleanup_part_files(epoch_part_path, plain_part_path).await;
                return Err(MqError::HashMismatch {
                    target: spec.id.clone(),
                    algo: "SHA-512",
                    expected: expected_sha512.to_hex(),
                    actual: actual.to_hex(),
                });
            }
        }

        // Final epoch validity check right before atomic rename!
        if epoch_guard.is_cancelled() || !self.cancel_epoch.is_valid(epoch_guard.epoch()) {
            let _ = file_state.try_transition(
                FileState::Verifying,
                FileState::Cancelled,
                epoch_guard.epoch(),
                self.cancel_epoch.atomic_epoch(),
            );
            cleanup_part_files(epoch_part_path, plain_part_path).await;
            return Err(MqError::Cancelled {
                epoch: epoch_guard.epoch(),
            });
        }

        fs::rename(epoch_part_path, &spec.dest_path).await?;
        let _ = fs::remove_file(plain_part_path).await;

        if file_state
            .try_transition(
                FileState::Verifying,
                FileState::Done,
                epoch_guard.epoch(),
                self.cancel_epoch.atomic_epoch(),
            )
            .is_err()
        {
            let _ = fs::remove_file(&spec.dest_path).await;
            cleanup_part_files(epoch_part_path, plain_part_path).await;
            return Err(MqError::Cancelled {
                epoch: epoch_guard.epoch(),
            });
        }

        Ok(DownloadOutcome {
            id: spec.id.clone(),
            dest_path: spec.dest_path.clone(),
            bytes_written: total_bytes_on_disk,
            sha1: actual_sha1,
            sha512: actual_sha512,
            resumed_from_offset,
            deduplicated: false,
        })
    }

    async fn try_verify_existing_file(
        &self,
        spec: &DownloadSpec,
        epoch_guard: &EpochGuard,
        deduplicated: bool,
    ) -> MqResult<Option<DownloadOutcome>> {
        let meta = match fs::metadata(&spec.dest_path).await {
            Ok(m) if m.is_file() => m,
            _ => return Ok(None),
        };

        let file_len = meta.len();
        if let Some(expected_size) = spec.expected_size {
            if file_len != expected_size {
                return Ok(None);
            }
        } else if file_len == 0 {
            return Ok(None);
        }

        let mut file = match fs::File::open(&spec.dest_path).await {
            Ok(f) => f,
            Err(_) => return Ok(None),
        };

        let mut pooled_buf = tokio::select! {
            biased;
            _ = epoch_guard.cancelled() => {
                return Err(MqError::Cancelled { epoch: epoch_guard.epoch() });
            }
            buf = self.buffer_pool.acquire() => buf,
        };

        let need_sha512 = self.compute_sha512 || spec.expected_sha512.is_some();
        let mut sha1_hasher = Sha1::new();
        let mut sha512_hasher = need_sha512.then(Sha512::new);
        let mut total_read = 0u64;

        loop {
            self.check_epoch(epoch_guard)?;
            let n = match file.read(pooled_buf.as_mut_slice()).await {
                Ok(n) => n,
                Err(_) => return Ok(None),
            };
            if n == 0 {
                break;
            }
            let slice = &pooled_buf.as_slice()[..n];
            sha1_hasher.update(slice);
            if let Some(ref mut h512) = sha512_hasher {
                h512.update(slice);
            }
            total_read += n as u64;
        }

        if total_read != file_len {
            return Ok(None);
        }

        let actual_sha1 = Sha1Digest::from_bytes(sha1_hasher.finalize().into());
        if let Some(expected_sha1) = spec.expected_sha1 {
            if actual_sha1 != expected_sha1 {
                return Ok(None);
            }
        }

        let actual_sha512 = sha512_hasher.map(|h| Sha512Digest::from_bytes(h.finalize().into()));
        if let Some(expected_sha512) = spec.expected_sha512 {
            if actual_sha512 != Some(expected_sha512) {
                return Ok(None);
            }
        }

        Ok(Some(DownloadOutcome {
            id: spec.id.clone(),
            dest_path: spec.dest_path.clone(),
            bytes_written: total_read,
            sha1: actual_sha1,
            sha512: actual_sha512,
            resumed_from_offset: 0,
            deduplicated,
        }))
    }

    async fn materialize_from_existing(
        &self,
        source_path: &Path,
        dest_path: &Path,
        epoch_guard: &EpochGuard,
    ) -> MqResult<()> {
        self.check_epoch(epoch_guard)?;
        if let Some(parent) = dest_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).await?;
            }
        }

        let tmp_path = make_part_path(dest_path, Some(epoch_guard.epoch()));
        if fs::hard_link(source_path, &tmp_path).await.is_ok() {
            self.check_epoch(epoch_guard)?;
            fs::rename(&tmp_path, dest_path).await?;
            return Ok(());
        }

        let mut src = fs::File::open(source_path).await?;
        let mut dst = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp_path)
            .await?;

        let mut pooled_buf = self.buffer_pool.acquire().await;
        loop {
            if let Err(err) = self.check_epoch(epoch_guard) {
                drop(dst);
                let _ = fs::remove_file(&tmp_path).await;
                return Err(err);
            }
            let n = src.read(pooled_buf.as_mut_slice()).await?;
            if n == 0 {
                break;
            }
            dst.write_all(&pooled_buf.as_slice()[..n]).await?;
        }
        drop(pooled_buf);
        dst.flush().await?;
        dst.sync_data().await?;
        drop(dst);

        if let Err(err) = self.check_epoch(epoch_guard) {
            let _ = fs::remove_file(&tmp_path).await;
            return Err(err);
        }

        fs::rename(&tmp_path, dest_path).await?;
        Ok(())
    }

    #[inline]
    fn check_epoch(&self, epoch_guard: &EpochGuard) -> MqResult<()> {
        if epoch_guard.is_cancelled() || !self.cancel_epoch.is_valid(epoch_guard.epoch()) {
            Err(MqError::Cancelled {
                epoch: epoch_guard.epoch(),
            })
        } else {
            Ok(())
        }
    }

    async fn sleep_with_cancel(
        &self,
        delay: std::time::Duration,
        epoch_guard: &EpochGuard,
    ) -> MqResult<()> {
        if delay.is_zero() {
            return self.check_epoch(epoch_guard);
        }
        tokio::select! {
            biased;
            _ = epoch_guard.cancelled() => {
                Err(MqError::Cancelled { epoch: epoch_guard.epoch() })
            }
            _ = tokio::time::sleep(delay) => {
                self.check_epoch(epoch_guard)
            }
        }
    }
}

fn compute_dedup_key(spec: &DownloadSpec) -> String {
    if let Some(sha1) = spec.expected_sha1 {
        format!("sha1:{}", sha1.to_hex())
    } else {
        format!("path:{}", spec.dest_path.display())
    }
}

fn make_part_path(dest_path: &Path, epoch: Option<u64>) -> PathBuf {
    let mut os = OsString::from(dest_path.as_os_str());
    match epoch {
        Some(ep) => os.push(format!(".e{ep}.part")),
        None => os.push(".part"),
    }
    PathBuf::from(os)
}

async fn cleanup_part_files(epoch_part_path: &Path, plain_part_path: &Path) {
    let _ = fs::remove_file(epoch_part_path).await;
    let _ = fs::remove_file(plain_part_path).await;
}

async fn read_error_body_preview(
    response: &mut reqwest::Response,
    epoch_guard: &EpochGuard,
) -> String {
    let mut preview = Vec::new();
    while preview.len() < MAX_ERROR_BODY_PREVIEW {
        if epoch_guard.is_cancelled() {
            break;
        }
        match tokio::time::timeout(std::time::Duration::from_secs(2), response.chunk()).await {
            Ok(Ok(Some(chunk))) => {
                let remaining = MAX_ERROR_BODY_PREVIEW - preview.len();
                let take = chunk.len().min(remaining);
                preview.extend_from_slice(&chunk[..take]);
                if take < chunk.len() {
                    break;
                }
            }
            _ => break,
        }
    }
    String::from_utf8_lossy(&preview).trim().to_owned()
}
