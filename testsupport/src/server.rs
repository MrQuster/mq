//! Local `axum` HTTP mock server bound to `127.0.0.1:0` with Range (`206`) and fault injection.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use bytes::Bytes;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::catalog::{
    curseforge_mod_by_id, curseforge_mod_file_by_id, curseforge_mod_file_download_url,
    curseforge_mod_files, curseforge_search, modrinth_project, modrinth_project_versions,
    modrinth_projects_batch, modrinth_search, modrinth_version_by_id, modrinth_versions_batch,
    CurseForgeSearchQuery, ModrinthBatchIdsQuery, ModrinthSearchQuery,
};
use crate::ely::{
    serve_authlib_latest_json, serve_ely_yggdrasil_meta, serve_skin_png, AuthenticateRequest,
    ElyAuthStore, RefreshRequest, ValidateRequest,
};
use crate::faults::{apply_body_fault, FaultAction, FaultInjector};
use crate::world::{MockConfig, MockWorld};

#[derive(Debug)]
struct ServerState {
    world: Arc<MockWorld>,
    ely: ElyAuthStore,
    faults: Arc<FaultInjector>,
    request_count: AtomicUsize,
    remaining_failures: AtomicUsize,
}

/// Embedded local HTTP mock server running on an ephemeral `127.0.0.1:0` port.
pub struct MockServer {
    addr: SocketAddr,
    base_url: String,
    state: Arc<ServerState>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    join_handle: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for MockServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockServer")
            .field("addr", &self.addr)
            .field("base_url", &self.base_url)
            .field("request_count", &self.request_count())
            .finish()
    }
}

impl MockServer {
    /// Binds `127.0.0.1:0`, initializes compact [`MockWorld`] metadata, and spawns `axum::serve`.
    pub async fn start(config: MockConfig) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral 127.0.0.1:0 listener for MockServer");
        let addr = listener.local_addr().expect("query local socket address");
        let base_url = format!("http://{addr}");

        let fail_first = config.fail_first_n_requests;
        let world = Arc::new(MockWorld::new(config, &base_url));
        let faults = Arc::new(FaultInjector::new());
        let state = Arc::new(ServerState {
            world,
            ely: ElyAuthStore::default(),
            faults,
            request_count: AtomicUsize::new(0),
            remaining_failures: AtomicUsize::new(fail_first),
        });

        let router = build_router(Arc::clone(&state));
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        let join_handle = tokio::spawn(async move {
            let _ = axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.await;
                })
                .await;
        });

        Self {
            addr,
            base_url,
            state,
            shutdown_tx: Some(shutdown_tx),
            join_handle: Some(join_handle),
        }
    }

    /// Starts a [`MockServer`] with [`MockConfig::default()`].
    pub async fn start_default() -> Self {
        Self::start(MockConfig::default()).await
    }

    /// Returns the base URL (`"http://127.0.0.1:<port>"`) without a trailing slash.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Returns the bound local socket address.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Returns a reference to the underlying [`MockWorld`].
    pub fn world(&self) -> &Arc<MockWorld> {
        &self.state.world
    }

    /// Returns the total number of HTTP requests received by this server instance.
    pub fn request_count(&self) -> usize {
        self.state.request_count.load(Ordering::SeqCst)
    }

    /// Dynamically configures the server to fail the next `count` incoming requests with `503`.
    pub fn set_fail_next_requests(&self, count: usize) {
        self.state.remaining_failures.store(count, Ordering::SeqCst);
    }

    /// Access the underlying [`FaultInjector`].
    pub fn faults(&self) -> &Arc<FaultInjector> {
        &self.state.faults
    }

    /// Helper to inject an arbitrary HTTP status code for the next `count` matching requests.
    pub fn inject_status(&self, path_filter: Option<&str>, status: StatusCode, count: usize) {
        self.state.faults.inject_status(path_filter, status, count);
    }

    /// Helper to inject HTTP 404 Not Found.
    pub fn inject_404(&self, path_filter: Option<&str>, count: usize) {
        self.state.faults.inject_404(path_filter, count);
    }

    /// Helper to inject HTTP 403 Forbidden.
    pub fn inject_403(&self, path_filter: Option<&str>, count: usize) {
        self.state.faults.inject_403(path_filter, count);
    }

    /// Helper to inject HTTP 429 Too Many Requests with a `Retry-After` header.
    pub fn inject_429(&self, path_filter: Option<&str>, retry_after_secs: u64, count: usize) {
        self.state
            .faults
            .inject_429(path_filter, retry_after_secs, count);
    }

    /// Helper to inject HTTP 500 Internal Server Error.
    pub fn inject_500(&self, path_filter: Option<&str>, count: usize) {
        self.state.faults.inject_500(path_filter, count);
    }

    /// Helper to inject HTTP 503 Service Unavailable.
    pub fn inject_503(
        &self,
        path_filter: Option<&str>,
        retry_after_secs: Option<u64>,
        count: usize,
    ) {
        self.state
            .faults
            .inject_503(path_filter, retry_after_secs, count);
    }

    /// Helper to inject body truncation mid-file.
    pub fn inject_truncation(&self, path_filter: Option<&str>, bytes_to_send: usize, count: usize) {
        self.state
            .faults
            .inject_truncation(path_filter, bytes_to_send, count);
    }

    /// Helper to inject SHA-1 byte corruption.
    pub fn inject_corrupt_sha1(&self, path_filter: Option<&str>, count: usize) {
        self.state.faults.inject_corrupt_sha1(path_filter, count);
    }

    /// Helper to inject invalid Content-Length header mismatch.
    pub fn inject_invalid_content_length(
        &self,
        path_filter: Option<&str>,
        claimed_len: usize,
        count: usize,
    ) {
        self.state
            .faults
            .inject_invalid_content_length(path_filter, claimed_len, count);
    }

    /// Clear all active fault rules.
    pub fn clear_faults(&self) {
        self.state.faults.clear();
    }

    /// Builds the Modrinth `.mrpack` bytes pointing to this server's `base_url`.
    pub fn build_mrpack_bytes(&self) -> Vec<u8> {
        self.state.world.build_mrpack_bytes()
    }

    /// Builds the CurseForge `.zip` modpack bytes pointing to this server's `base_url`.
    pub fn build_curseforge_zip_bytes(&self) -> Vec<u8> {
        self.state.world.build_curseforge_zip_bytes()
    }

    /// Convenience helper returning `"{base_url}/mc/game/version_manifest_v2.json"`.
    pub fn version_manifest_url(&self) -> String {
        format!("{}/mc/game/version_manifest_v2.json", self.base_url)
    }

    /// Convenience helper returning `"{base_url}/fabric"`.
    pub fn fabric_meta_url(&self) -> String {
        format!("{}/fabric", self.base_url)
    }

    /// Convenience helper returning `"{base_url}/quilt"`.
    pub fn quilt_meta_url(&self) -> String {
        format!("{}/quilt", self.base_url)
    }

    /// Convenience helper returning `"{base_url}/modrinth/v2"`.
    pub fn modrinth_api_url(&self) -> String {
        format!("{}/modrinth/v2", self.base_url)
    }

    /// Convenience helper returning `"{base_url}/curseforge/v1"`.
    pub fn curseforge_api_url(&self) -> String {
        format!("{}/curseforge/v1", self.base_url)
    }

    /// Convenience helper returning `"{base_url}/ely"`.
    pub fn ely_base_url(&self) -> String {
        format!("{}/ely", self.base_url)
    }

    /// Convenience helper returning `"{base_url}/authlib-injector/artifact/latest.json"`.
    pub fn authlib_latest_url(&self) -> String {
        format!("{}/authlib-injector/artifact/latest.json", self.base_url)
    }

    /// Gracefully stops the background `axum` server task.
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.join_handle.take() {
            handle.abort();
            let _ = handle.await;
        }
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.join_handle.take() {
            handle.abort();
        }
    }
}

fn build_router(state: Arc<ServerState>) -> Router {
    Router::new()
        // Mojang endpoints
        .route(
            "/mc/game/version_manifest_v2.json",
            get(handle_version_manifest),
        )
        .route("/mc/meta/{ver}/version.json", get(handle_version_json))
        .route(
            "/mc/assets/indexes/{ver_json}",
            get(handle_asset_index_json),
        )
        .route(
            "/mc/assets/objects/{prefix}/{hash}",
            get(handle_asset_object),
        )
        .route(
            "/{prefix}/{hash}",
            get(handle_asset_object),
        )
        // Fabric Meta endpoints (+ root aliases)
        .route(
            "/fabric/v2/versions/loader/{mc_ver}",
            get(handle_fabric_loader_list),
        )
        .route(
            "/fabric/v2/versions/loader/{mc_ver}/{loader_ver}/profile/json",
            get(handle_fabric_profile_json),
        )
        .route(
            "/v2/versions/loader/{mc_ver}",
            get(handle_fabric_loader_list),
        )
        .route(
            "/v2/versions/loader/{mc_ver}/{loader_ver}/profile/json",
            get(handle_fabric_profile_json),
        )
        // Quilt Meta endpoints (+ root aliases)
        .route(
            "/quilt/v3/versions/loader/{mc_ver}",
            get(handle_quilt_loader_list),
        )
        .route(
            "/quilt/v3/versions/loader/{mc_ver}/{loader_ver}/profile/json",
            get(handle_quilt_profile_json),
        )
        .route(
            "/v3/versions/loader/{mc_ver}",
            get(handle_quilt_loader_list),
        )
        .route(
            "/v3/versions/loader/{mc_ver}/{loader_ver}/profile/json",
            get(handle_quilt_profile_json),
        )
        // Modrinth API v2 endpoints (+ root aliases)
        .route("/modrinth/v2/search", get(handle_modrinth_search))
        .route("/modrinth/v2/projects", get(handle_modrinth_projects_batch))
        .route("/modrinth/v2/versions", get(handle_modrinth_versions_batch))
        .route(
            "/modrinth/v2/project/{id_or_slug}",
            get(handle_modrinth_project),
        )
        .route(
            "/modrinth/v2/project/{id_or_slug}/version",
            get(handle_modrinth_project_versions),
        )
        .route(
            "/modrinth/v2/version/{version_id}",
            get(handle_modrinth_version_by_id),
        )
        .route("/v2/search", get(handle_modrinth_search))
        .route("/v2/projects", get(handle_modrinth_projects_batch))
        .route("/v2/versions", get(handle_modrinth_versions_batch))
        .route("/v2/project/{id_or_slug}", get(handle_modrinth_project))
        .route(
            "/v2/project/{id_or_slug}/version",
            get(handle_modrinth_project_versions),
        )
        .route(
            "/v2/version/{version_id}",
            get(handle_modrinth_version_by_id),
        )
        // CurseForge API v1 endpoints (+ root aliases)
        .route("/curseforge/v1/mods/search", get(handle_cf_search))
        .route("/curseforge/v1/mods/{mod_id}", get(handle_cf_mod_by_id))
        .route(
            "/curseforge/v1/mods/{mod_id}/files",
            get(handle_cf_mod_files),
        )
        .route(
            "/curseforge/v1/mods/{mod_id}/files/{file_id}",
            get(handle_cf_mod_file_by_id),
        )
        .route(
            "/curseforge/v1/mods/{mod_id}/files/{file_id}/download-url",
            get(handle_cf_mod_file_download_url),
        )
        .route("/v1/mods/search", get(handle_cf_search))
        .route("/v1/mods/{mod_id}", get(handle_cf_mod_by_id))
        .route("/v1/mods/{mod_id}/files", get(handle_cf_mod_files))
        .route(
            "/v1/mods/{mod_id}/files/{file_id}",
            get(handle_cf_mod_file_by_id),
        )
        .route(
            "/v1/mods/{mod_id}/files/{file_id}/download-url",
            get(handle_cf_mod_file_download_url),
        )
        // Ely.by & skins & authlib-injector endpoints (+ root aliases)
        .route("/ely/auth/authenticate", post(handle_ely_authenticate))
        .route("/ely/auth/refresh", post(handle_ely_refresh))
        .route("/ely/auth/validate", post(handle_ely_validate))
        .route("/ely/skins/{username_png}", get(handle_ely_skin))
        .route("/ely/api/authlib-injector", get(handle_ely_yggdrasil_meta))
        .route("/auth/authenticate", post(handle_ely_authenticate))
        .route("/auth/refresh", post(handle_ely_refresh))
        .route("/auth/validate", post(handle_ely_validate))
        .route("/skins/{username_png}", get(handle_ely_skin))
        .route("/api/authlib-injector", get(handle_ely_yggdrasil_meta))
        .route(
            "/authlib-injector/artifact/latest.json",
            get(handle_authlib_latest),
        )
        .route("/artifact/latest.json", get(handle_authlib_latest))
        // Binary download routes with HTTP Range (`206 Partial Content`) support
        .route("/download/{*rel_path}", get(handle_download))
        .layer(middleware::from_fn_with_state(
            Arc::clone(&state),
            request_gate_middleware,
        ))
        .with_state(state)
}

async fn request_gate_middleware(
    State(state): State<Arc<ServerState>>,
    req: Request,
    next: Next,
) -> Response {
    state.request_count.fetch_add(1, Ordering::SeqCst);

    let path = req.uri().path().to_string();
    let fault = state.faults.evaluate(&path);

    if let Some(FaultAction::HttpStatus {
        status,
        retry_after_secs,
        message,
    }) = fault
    {
        let mut resp = (status, message).into_response();
        if let Some(secs) = retry_after_secs {
            if let Ok(val) = secs.to_string().parse() {
                resp.headers_mut().insert(header::RETRY_AFTER, val);
            }
        }
        return resp;
    }

    let mut should_fail = false;
    let mut current = state.remaining_failures.load(Ordering::SeqCst);
    while current > 0 {
        match state.remaining_failures.compare_exchange_weak(
            current,
            current - 1,
            Ordering::SeqCst,
            Ordering::SeqCst,
        ) {
            Ok(_) => {
                should_fail = true;
                break;
            }
            Err(actual) => current = actual,
        }
    }

    if should_fail {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [(header::RETRY_AFTER, "0")],
            "injected mock transient failure",
        )
            .into_response();
    }

    let resp = next.run(req).await;

    if let Some(action) = fault {
        apply_body_fault(resp, action).await
    } else {
        resp
    }
}

async fn handle_version_manifest(State(state): State<Arc<ServerState>>) -> Response {
    json_bytes_response(state.world.version_manifest_v2_json())
}

async fn handle_version_json(
    State(state): State<Arc<ServerState>>,
    Path(ver): Path<String>,
) -> Response {
    match state.world.version_json(&ver) {
        Some(bytes) => json_bytes_response(bytes),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn handle_asset_index_json(
    State(state): State<Arc<ServerState>>,
    Path(ver): Path<String>,
) -> Response {
    let index_ver = ver.strip_suffix(".json").unwrap_or(&ver);
    match state.world.asset_index_json(index_ver) {
        Some(bytes) => json_bytes_response(bytes),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn handle_asset_object(
    State(state): State<Arc<ServerState>>,
    Path((prefix, hash)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    match state.world.asset_payload_by_hash(&prefix, &hash) {
        Some(bytes) => serve_bytes_with_range(bytes, "application/octet-stream", &headers),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn handle_fabric_loader_list(
    State(state): State<Arc<ServerState>>,
    Path(mc_ver): Path<String>,
) -> Response {
    match state.world.fabric_loader_list_json(&mc_ver) {
        Some(bytes) => json_bytes_response(bytes),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn handle_fabric_profile_json(
    State(state): State<Arc<ServerState>>,
    Path((mc_ver, loader_ver)): Path<(String, String)>,
) -> Response {
    match state.world.fabric_profile_json(&mc_ver, &loader_ver) {
        Some(bytes) => json_bytes_response(bytes),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn handle_quilt_loader_list(
    State(state): State<Arc<ServerState>>,
    Path(mc_ver): Path<String>,
) -> Response {
    match state.world.quilt_loader_list_json(&mc_ver) {
        Some(bytes) => json_bytes_response(bytes),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn handle_quilt_profile_json(
    State(state): State<Arc<ServerState>>,
    Path((mc_ver, loader_ver)): Path<(String, String)>,
) -> Response {
    match state.world.quilt_profile_json(&mc_ver, &loader_ver) {
        Some(bytes) => json_bytes_response(bytes),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn handle_modrinth_search(
    State(state): State<Arc<ServerState>>,
    Query(query): Query<ModrinthSearchQuery>,
) -> Response {
    modrinth_search(&state.world, &query)
}

async fn handle_modrinth_projects_batch(
    State(state): State<Arc<ServerState>>,
    Query(query): Query<ModrinthBatchIdsQuery>,
) -> Response {
    modrinth_projects_batch(&state.world, &query)
}

async fn handle_modrinth_versions_batch(
    State(state): State<Arc<ServerState>>,
    Query(query): Query<ModrinthBatchIdsQuery>,
) -> Response {
    modrinth_versions_batch(&state.world, &query)
}

async fn handle_modrinth_project(
    State(state): State<Arc<ServerState>>,
    Path(id_or_slug): Path<String>,
) -> Response {
    modrinth_project(&state.world, &id_or_slug)
}

async fn handle_modrinth_project_versions(
    State(state): State<Arc<ServerState>>,
    Path(id_or_slug): Path<String>,
) -> Response {
    modrinth_project_versions(&state.world, &id_or_slug)
}

async fn handle_modrinth_version_by_id(
    State(state): State<Arc<ServerState>>,
    Path(version_id): Path<String>,
) -> Response {
    modrinth_version_by_id(&state.world, &version_id)
}

async fn handle_cf_search(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Query(query): Query<CurseForgeSearchQuery>,
) -> Response {
    curseforge_search(&state.world, &headers, &query)
}

async fn handle_cf_mod_by_id(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Path(mod_id): Path<u32>,
) -> Response {
    curseforge_mod_by_id(&state.world, &headers, mod_id)
}

async fn handle_cf_mod_files(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Path(mod_id): Path<u32>,
) -> Response {
    curseforge_mod_files(&state.world, &headers, mod_id)
}

async fn handle_cf_mod_file_by_id(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Path((mod_id, file_id)): Path<(u32, u32)>,
) -> Response {
    curseforge_mod_file_by_id(&state.world, &headers, mod_id, file_id)
}

async fn handle_cf_mod_file_download_url(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Path((mod_id, file_id)): Path<(u32, u32)>,
) -> Response {
    curseforge_mod_file_download_url(&state.world, &headers, mod_id, file_id)
}

async fn handle_ely_authenticate(
    State(state): State<Arc<ServerState>>,
    Json(req): Json<AuthenticateRequest>,
) -> Response {
    state.ely.authenticate(req)
}

async fn handle_ely_refresh(
    State(state): State<Arc<ServerState>>,
    Json(req): Json<RefreshRequest>,
) -> Response {
    state.ely.refresh(req)
}

async fn handle_ely_validate(
    State(state): State<Arc<ServerState>>,
    Json(req): Json<ValidateRequest>,
) -> Response {
    state.ely.validate(req)
}

async fn handle_ely_skin(Path(username_png): Path<String>) -> Response {
    serve_skin_png(&username_png)
}

async fn handle_ely_yggdrasil_meta() -> Response {
    serve_ely_yggdrasil_meta()
}

async fn handle_authlib_latest(State(state): State<Arc<ServerState>>) -> Response {
    serve_authlib_latest_json(&state.world)
}

async fn handle_download(
    State(state): State<Arc<ServerState>>,
    Path(rel_path): Path<String>,
    headers: HeaderMap,
) -> Response {
    match state.world.download_payload(&rel_path) {
        Some((bytes, content_type)) => serve_bytes_with_range(bytes, content_type, &headers),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn json_bytes_response(bytes: Bytes) -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        bytes,
    )
        .into_response()
}

/// Serves `full_bytes` supporting `Range: bytes=<start>-` and `Range: bytes=<start>-<end>`.
fn serve_bytes_with_range(
    full_bytes: Bytes,
    content_type: &'static str,
    headers: &HeaderMap,
) -> Response {
    let total = full_bytes.len();
    if let Some(range_val) = headers.get(header::RANGE).and_then(|v| v.to_str().ok()) {
        if let Some(spec) = range_val.trim().strip_prefix("bytes=") {
            let (start_str, end_str) = spec.split_once('-').unwrap_or((spec, ""));
            if let Ok(start) = start_str.trim().parse::<usize>() {
                if start >= total {
                    let content_range = format!("bytes */{total}");
                    return (
                        StatusCode::RANGE_NOT_SATISFIABLE,
                        [(header::CONTENT_RANGE, content_range)],
                        Bytes::new(),
                    )
                        .into_response();
                }
                let end = if end_str.trim().is_empty() {
                    total - 1
                } else {
                    match end_str.trim().parse::<usize>() {
                        Ok(e) => e.min(total - 1),
                        Err(_) => total - 1,
                    }
                };
                if start <= end {
                    let slice = full_bytes.slice(start..=end);
                    let slice_len = slice.len().to_string();
                    let content_range = format!("bytes {start}-{end}/{total}");
                    return (
                        StatusCode::PARTIAL_CONTENT,
                        [
                            (header::CONTENT_TYPE, content_type.to_string()),
                            (header::ACCEPT_RANGES, "bytes".to_string()),
                            (header::CONTENT_RANGE, content_range),
                            (header::CONTENT_LENGTH, slice_len),
                        ],
                        slice,
                    )
                        .into_response();
                }
            }
        }
    }

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type.to_string()),
            (header::ACCEPT_RANGES, "bytes".to_string()),
            (header::CONTENT_LENGTH, total.to_string()),
        ],
        full_bytes,
    )
        .into_response()
}
