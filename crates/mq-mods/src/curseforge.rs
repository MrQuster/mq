//! CurseForge Core API v1 client (`https://api.curseforge.com/v1`) with API key enforcement,
//! search, file enumeration, and `edge.forgecdn.net` fallback download URL synthesis.

use std::time::Duration;

use mq_core::{
    AppConfig, ModLoaderKind, MqError, MqResult, SecretString, Sha1Digest, ENV_CF_API_KEY,
};
use reqwest::header::{ACCEPT, USER_AGENT};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use tracing::debug;
use url::Url;

use crate::models::{
    CatalogHit, CatalogSource, DependencyKind, ModDependencyRef, ProjectKind, ProjectVersion,
    ProjectVersionFile, ResolvedModDownload, SearchQuery, MQ_USER_AGENT,
};
use crate::resolver::{IntoLoaderFilter, IntoMcVersionFilter};

/// Default production base URL for CurseForge Core API v1.
pub const DEFAULT_CURSEFORGE_API_URL: &str = "https://api.curseforge.com/v1";

/// CurseForge `gameId` for Minecraft: Java Edition.
pub const MINECRAFT_GAME_ID: u32 = 432;

/// CurseForge `FileRelationType = 3` (`RequiredDependency`).
pub const CF_RELATION_REQUIRED_DEPENDENCY: u32 = 3;

/// Computes the standard `edge.forgecdn.net` CDN download URL when CurseForge `downloadUrl` is `null`.
///
/// Format: `https://edge.forgecdn.net/files/{id / 1000}/{id % 1000}/{fileName}`.
pub fn compute_curseforge_cdn_fallback_url(file_id: u64, file_name: &str) -> String {
    let major = file_id / 1000;
    let minor = file_id % 1000;
    let encoded_name = file_name.replace(' ', "%20");
    format!("https://edge.forgecdn.net/files/{major}/{minor}/{encoded_name}")
}

/// Maps a [`ModLoaderKind`] to CurseForge's `ModLoaderType` numeric enum (`1=Forge, 4=Fabric, 5=Quilt, 6=NeoForge`).
#[inline]
pub const fn mod_loader_to_cf_type(loader: ModLoaderKind) -> Option<u32> {
    match loader {
        ModLoaderKind::Vanilla => None,
        ModLoaderKind::Forge => Some(1),
        ModLoaderKind::Fabric => Some(4),
        ModLoaderKind::Quilt => Some(5),
        ModLoaderKind::NeoForge => Some(6),
    }
}

/// Maps a CurseForge `ModLoaderType` integer to a canonical loader name.
#[inline]
pub const fn cf_type_to_mod_loader(cf_loader_type: u32) -> Option<&'static str> {
    match cf_loader_type {
        1 => Some("forge"),
        4 => Some("fabric"),
        5 => Some("quilt"),
        6 => Some("neoforge"),
        _ => None,
    }
}

/// Asynchronous HTTP client for CurseForge Core API v1.
#[derive(Debug, Clone)]
pub struct CurseForgeClient {
    http: reqwest::Client,
    base_url: String,
    api_key: Option<SecretString>,
}

impl CurseForgeClient {
    /// Creates a [`CurseForgeClient`] pointing at [`DEFAULT_CURSEFORGE_API_URL`].
    pub fn new(api_key: Option<String>) -> MqResult<Self> {
        Self::with_base_url(DEFAULT_CURSEFORGE_API_URL, api_key)
    }

    /// Creates a [`CurseForgeClient`] reading `$MQ_CF_API_KEY` from the process environment.
    pub fn from_env() -> MqResult<Self> {
        let key = std::env::var(ENV_CF_API_KEY).ok();
        Self::new(key)
    }

    /// Creates a [`CurseForgeClient`] from [`AppConfig`], honoring `$MQ_CF_API_KEY`,
    /// `config.curseforge_api_key`, and `config.mock_url`.
    pub fn from_config(config: &AppConfig) -> MqResult<Self> {
        let base_url = match config
            .mock_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(mock) => {
                let trimmed = mock.trim_end_matches('/');
                if trimmed.ends_with("/curseforge/v1") || trimmed.ends_with("/v1") {
                    trimmed.to_owned()
                } else {
                    format!("{trimmed}/curseforge/v1")
                }
            }
            None => DEFAULT_CURSEFORGE_API_URL.to_owned(),
        };
        Self::with_base_url(base_url, config.effective_cf_api_key())
    }

    /// Creates a [`CurseForgeClient`] with a custom `base_url` and optional `api_key`.
    pub fn with_base_url(base_url: impl Into<String>, api_key: Option<String>) -> MqResult<Self> {
        let http = reqwest::Client::builder()
            .user_agent(MQ_USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|err| {
                MqError::Network(format!("failed to build CurseForge HTTP client: {err}"))
            })?;
        Ok(Self::with_http_client(http, base_url, api_key))
    }

    /// Creates a [`CurseForgeClient`] wrapping an existing [`reqwest::Client`], `base_url`, and `api_key`.
    pub fn with_http_client(
        http: reqwest::Client,
        base_url: impl Into<String>,
        api_key: Option<String>,
    ) -> Self {
        let normalized_url = base_url.into().trim().trim_end_matches('/').to_owned();
        let normalized_key = api_key
            .map(|k| k.trim().to_owned())
            .filter(|k| !k.is_empty())
            .map(SecretString::new);

        Self {
            http,
            base_url: if normalized_url.is_empty() {
                DEFAULT_CURSEFORGE_API_URL.to_owned()
            } else {
                normalized_url
            },
            api_key: normalized_key,
        }
    }

    /// Returns the configured CurseForge API v1 base URL without a trailing slash.
    #[inline]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Returns `true` if a non-empty CurseForge API key is configured.
    #[inline]
    pub fn has_api_key(&self) -> bool {
        self.api_key.is_some()
    }

    /// Validates that a non-empty CurseForge API key is present or returns [`MqError::MissingApiKey`].
    pub fn require_api_key(&self) -> MqResult<&str> {
        self.api_key
            .as_ref()
            .map(SecretString::expose_secret)
            .filter(|s| !s.trim().is_empty())
            .ok_or(MqError::MissingApiKey {
                service: "CurseForge",
                env_var: ENV_CF_API_KEY,
            })
    }

    /// Searches CurseForge (`GET /mods/search?gameId=432&classId=...&searchFilter=...&gameVersion=...&modLoaderType=...`).
    pub async fn search(&self, q: &SearchQuery) -> MqResult<Vec<CatalogHit>> {
        let api_key = self.require_api_key()?;
        let mut url = self.endpoint_url(&["mods", "search"])?;
        let page_size = if q.limit == 0 { 20 } else { q.limit.min(50) };

        {
            let mut qp = url.query_pairs_mut();
            qp.append_pair("gameId", &MINECRAFT_GAME_ID.to_string());
            qp.append_pair("classId", &q.project_kind.curseforge_class_id().to_string());
            qp.append_pair("searchFilter", &q.query);
            if let Some(mc_ver) = q
                .mc_version
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                qp.append_pair("gameVersion", mc_ver);
            }
            if matches!(q.project_kind, ProjectKind::Mod | ProjectKind::Modpack) {
                if let Some(loader) = q.loader {
                    if let Some(cf_loader) = mod_loader_to_cf_type(loader) {
                        qp.append_pair("modLoaderType", &cf_loader.to_string());
                    }
                }
            }
            qp.append_pair("pageSize", &page_size.to_string());
            qp.append_pair("index", &q.offset.to_string());
        }

        debug!(url = %url, "searching CurseForge v1 catalog");
        let env: CfEnvelope<Vec<CfModDto>> = self.get_json(url, api_key).await?;
        Ok(env
            .data
            .into_iter()
            .map(|dto| dto.into_catalog_hit(q.project_kind))
            .collect())
    }

    /// Fetches a single CurseForge project (`GET /mods/{mod_id}`).
    pub async fn get_mod(&self, mod_id: u64) -> MqResult<CatalogHit> {
        let api_key = self.require_api_key()?;
        let id_str = mod_id.to_string();
        let url = self.endpoint_url(&["mods", &id_str])?;
        debug!(url = %url, mod_id, "fetching CurseForge mod metadata");
        let env: CfEnvelope<CfModDto> = self.get_json(url, api_key).await?;
        Ok(env.data.into_catalog_hit(ProjectKind::Mod))
    }

    /// Fetches compatible files for a CurseForge mod (`GET /mods/{mod_id}/files`),
    /// synthesizing standard `edge.forgecdn.net` fallback URLs when `downloadUrl` is `null`.
    pub async fn get_mod_files(
        &self,
        mod_id: u64,
        mc_version: Option<&str>,
        loader: Option<ModLoaderKind>,
    ) -> MqResult<Vec<ProjectVersion>> {
        let api_key = self.require_api_key()?;
        let id_str = mod_id.to_string();
        let mut url = self.endpoint_url(&["mods", &id_str, "files"])?;
        let mc_filter = mc_version.map(str::trim).filter(|s| !s.is_empty());
        let loader_filter = loader.filter(|l| *l != ModLoaderKind::Vanilla);

        {
            let mut qp = url.query_pairs_mut();
            if let Some(ver) = mc_filter {
                qp.append_pair("gameVersion", ver);
            }
            if let Some(ldr) = loader_filter {
                if let Some(cf_loader) = mod_loader_to_cf_type(ldr) {
                    qp.append_pair("modLoaderType", &cf_loader.to_string());
                }
            }
        }

        debug!(url = %url, mod_id, ?mc_filter, ?loader_filter, "fetching CurseForge mod files");
        let env: CfEnvelope<Vec<CfFileDto>> = self.get_json(url, api_key).await?;

        let mut out = Vec::with_capacity(env.data.len());
        for dto in env.data {
            let ver = dto.into_project_version(mod_id)?;
            if let Some(target_mc) = mc_filter {
                if !ver.game_versions.is_empty()
                    && !ver
                        .game_versions
                        .iter()
                        .any(|gv| gv.eq_ignore_ascii_case(target_mc))
                {
                    continue;
                }
            }
            if let Some(target_loader) = loader_filter {
                if !ver.loaders.is_empty()
                    && !ver
                        .loaders
                        .iter()
                        .any(|ld| ld.eq_ignore_ascii_case(target_loader.as_str()))
                {
                    continue;
                }
            }
            out.push(ver);
        }

        Ok(out)
    }

    /// Fetches a specific file for a CurseForge mod (`GET /mods/{mod_id}/files/{file_id}`).
    pub async fn get_mod_file(&self, mod_id: u64, file_id: u64) -> MqResult<ProjectVersion> {
        let api_key = self.require_api_key()?;
        let mod_str = mod_id.to_string();
        let file_str = file_id.to_string();
        let url = self.endpoint_url(&["mods", &mod_str, "files", &file_str])?;
        debug!(url = %url, mod_id, file_id, "fetching CurseForge mod file by id");
        let env: CfEnvelope<CfFileDto> = self.get_json(url, api_key).await?;
        env.data.into_project_version(mod_id)
    }

    /// Fetches the download URL for a specific CurseForge mod file (`GET /mods/{mod_id}/files/{file_id}/download-url`).
    pub async fn get_mod_file_download_url(&self, mod_id: u64, file_id: u64) -> MqResult<String> {
        let api_key = self.require_api_key()?;
        let mod_str = mod_id.to_string();
        let file_str = file_id.to_string();
        let url = self.endpoint_url(&["mods", &mod_str, "files", &file_str, "download-url"])?;
        let env: CfEnvelope<String> = self.get_json(url, api_key).await?;
        Ok(env.data)
    }

    /// Resolves a CurseForge mod and all transitive required dependencies (`relationType == 3`)
    /// into a deterministic download plan.
    pub async fn resolve_mod_with_dependencies<V, L>(
        &self,
        project_id_or_slug: &str,
        mc_version: V,
        loader: L,
    ) -> MqResult<Vec<ResolvedModDownload>>
    where
        V: IntoMcVersionFilter,
        L: IntoLoaderFilter,
    {
        crate::resolver::resolve_mod_with_dependencies(self, project_id_or_slug, mc_version, loader)
            .await
    }

    fn endpoint_url(&self, segments: &[&str]) -> MqResult<Url> {
        let mut url = Url::parse(&self.base_url).map_err(|err| {
            MqError::Metadata(format!(
                "invalid CurseForge base URL '{}': {err}",
                self.base_url
            ))
        })?;
        {
            let mut path_segs = url.path_segments_mut().map_err(|()| {
                MqError::Metadata(format!(
                    "CurseForge base URL '{}' cannot be a base",
                    self.base_url
                ))
            })?;
            path_segs.pop_if_empty();
            for seg in segments {
                path_segs.push(seg);
            }
        }
        Ok(url)
    }

    async fn get_json<T: DeserializeOwned>(&self, url: Url, api_key: &str) -> MqResult<T> {
        let url_str = url.to_string();
        let resp = self
            .http
            .get(url)
            .header(USER_AGENT, MQ_USER_AGENT)
            .header(ACCEPT, "application/json")
            .header("x-api-key", api_key)
            .send()
            .await
            .map_err(|err| MqError::Network(format!("GET {url_str} failed: {err}")))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(MqError::HttpStatus {
                status: status.as_u16(),
                url: url_str,
                body,
            });
        }

        let bytes = resp.bytes().await.map_err(|err| {
            MqError::Network(format!(
                "failed reading response body from {url_str}: {err}"
            ))
        })?;
        serde_json::from_slice(&bytes).map_err(MqError::Json)
    }
}

#[derive(Debug, Deserialize)]
struct CfEnvelope<T> {
    data: T,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfModDto {
    id: u64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    class_id: Option<u32>,
    #[serde(default)]
    download_count: u64,
    #[serde(default)]
    authors: Vec<CfAuthorDto>,
    #[serde(default)]
    logo: Option<CfLogoDto>,
    #[serde(default)]
    main_file_id: Option<u64>,
    #[serde(default)]
    latest_files: Vec<CfFileDto>,
    #[serde(default)]
    latest_files_indexes: Vec<CfFileIndexDto>,
}

#[derive(Debug, Deserialize)]
struct CfAuthorDto {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize)]
struct CfLogoDto {
    #[serde(default)]
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfFileIndexDto {
    #[serde(default)]
    game_version: String,
    #[serde(default)]
    file_id: u64,
    #[serde(default)]
    mod_loader: Option<u32>,
}

impl CfModDto {
    fn into_catalog_hit(self, fallback_kind: ProjectKind) -> CatalogHit {
        let project_kind = self
            .class_id
            .and_then(ProjectKind::from_curseforge_class_id)
            .unwrap_or(fallback_kind);
        let author = self
            .authors
            .into_iter()
            .map(|a| a.name)
            .filter(|n| !n.trim().is_empty())
            .collect::<Vec<_>>()
            .join(", ");

        let mut loaders = Vec::new();
        let mut game_versions = Vec::new();

        for idx in &self.latest_files_indexes {
            if let Some(ldr_code) = idx.mod_loader {
                if let Some(ldr_str) = cf_type_to_mod_loader(ldr_code) {
                    if !loaders.iter().any(|l| l == ldr_str) {
                        loaders.push(ldr_str.to_owned());
                    }
                }
            }
            classify_cf_version_tag(&idx.game_version, &mut game_versions, &mut loaders);
        }

        for file in &self.latest_files {
            for tag in &file.game_versions {
                classify_cf_version_tag(tag, &mut game_versions, &mut loaders);
            }
        }

        let latest_version = self
            .main_file_id
            .filter(|id| *id > 0)
            .map(|id| id.to_string())
            .or_else(|| self.latest_files.first().map(|f| f.id.to_string()))
            .or_else(|| {
                self.latest_files_indexes
                    .first()
                    .filter(|i| i.file_id > 0)
                    .map(|i| i.file_id.to_string())
            });

        let slug = if self.slug.is_empty() {
            self.id.to_string()
        } else {
            self.slug
        };

        CatalogHit {
            source: CatalogSource::CurseForge,
            project_id: self.id.to_string(),
            slug,
            title: self.name,
            description: self.summary,
            author,
            downloads: self.download_count,
            icon_url: self
                .logo
                .and_then(|l| l.url)
                .filter(|u| !u.trim().is_empty()),
            project_kind,
            loaders,
            game_versions,
            latest_version,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfFileDto {
    id: u64,
    #[serde(default)]
    mod_id: Option<u64>,
    #[serde(default)]
    display_name: Option<String>,
    file_name: String,
    #[serde(default)]
    file_length: u64,
    #[serde(default)]
    download_url: Option<String>,
    #[serde(default)]
    hashes: Vec<CfHashDto>,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    dependencies: Vec<CfDependencyDto>,
}

#[derive(Debug, Deserialize)]
struct CfHashDto {
    #[serde(default)]
    value: String,
    #[serde(default)]
    algo: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfDependencyDto {
    mod_id: u64,
    relation_type: u32,
}

impl CfFileDto {
    fn into_project_version(self, fallback_mod_id: u64) -> MqResult<ProjectVersion> {
        let project_id = self.mod_id.unwrap_or(fallback_mod_id).to_string();
        let url = self
            .download_url
            .as_deref()
            .map(str::trim)
            .filter(|u| !u.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| compute_curseforge_cdn_fallback_url(self.id, &self.file_name));

        // In CurseForge API v1, algo == 1 is SHA-1 (algo == 2 is MD5).
        let sha1 = match self
            .hashes
            .iter()
            .find(|h| h.algo == 1 && !h.value.trim().is_empty())
        {
            Some(h) => Some(Sha1Digest::from_hex(h.value.trim())?),
            None => None,
        };

        let mut game_versions = Vec::new();
        let mut loaders = Vec::new();
        for tag in &self.game_versions {
            classify_cf_version_tag(tag, &mut game_versions, &mut loaders);
        }

        let dependencies = self
            .dependencies
            .into_iter()
            .filter_map(|dep| {
                let kind = DependencyKind::from_curseforge_relation_type(dep.relation_type)?;
                Some(ModDependencyRef {
                    project_id: Some(dep.mod_id.to_string()),
                    version_id: None,
                    file_name: None,
                    kind,
                })
            })
            .collect();

        let name = self
            .display_name
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| self.file_name.clone());
        let version_number = self.file_name.clone();

        let file = ProjectVersionFile {
            url,
            filename: self.file_name,
            size: self.file_length,
            sha1,
            sha512: None,
            primary: true,
        };

        Ok(ProjectVersion {
            id: self.id.to_string(),
            project_id,
            name,
            version_number,
            game_versions,
            loaders,
            files: vec![file],
            dependencies,
        })
    }
}

fn classify_cf_version_tag(
    raw_tag: &str,
    game_versions: &mut Vec<String>,
    loaders: &mut Vec<String>,
) {
    let trimmed = raw_tag.trim();
    if trimmed.is_empty() {
        return;
    }
    let lower = trimmed.to_ascii_lowercase();
    match lower.as_str() {
        "fabric" | "quilt" | "forge" | "neoforge" => {
            if !loaders.contains(&lower) {
                loaders.push(lower);
            }
        }
        "client" | "server" => {}
        _ if lower.starts_with("java ") => {}
        _ => {
            if !game_versions
                .iter()
                .any(|v| v.eq_ignore_ascii_case(trimmed))
            {
                game_versions.push(trimmed.to_owned());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    #[derive(Debug, Clone)]
    struct RecordedCfRequest {
        request_line: String,
        headers: HashMap<String, String>,
    }

    async fn spawn_mock_cf_server<F>(handler: F) -> (String, Arc<Mutex<Vec<RecordedCfRequest>>>)
    where
        F: Fn(&str, &HashMap<String, String>) -> (u16, String) + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock cf listener");
        let addr = listener.local_addr().expect("local addr");
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let recorded_clone = Arc::clone(&recorded);
        let handler = Arc::new(handler);

        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let rec = Arc::clone(&recorded_clone);
                let h = Arc::clone(&handler);
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 8192];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    let raw = String::from_utf8_lossy(&buf[..n]);
                    let mut lines = raw.lines();
                    let request_line = lines.next().unwrap_or("").to_owned();
                    let mut headers = HashMap::new();
                    for line in lines {
                        if line.is_empty() {
                            break;
                        }
                        if let Some((k, v)) = line.split_once(':') {
                            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_owned());
                        }
                    }

                    let path_and_query = request_line.split_whitespace().nth(1).unwrap_or("/");
                    let (status, body) = h(path_and_query, &headers);
                    rec.lock().unwrap().push(RecordedCfRequest {
                        request_line: request_line.clone(),
                        headers,
                    });

                    let status_text = if status == 200 { "OK" } else { "Forbidden" };
                    let resp = format!(
                        "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                });
            }
        });

        (format!("http://{addr}/curseforge/v1"), recorded)
    }

    #[tokio::test]
    async fn curseforge_missing_api_key_returns_clear_error_without_network_call() {
        for missing in [None, Some(String::new()), Some("   ".to_owned())] {
            let client =
                CurseForgeClient::with_base_url("http://127.0.0.1:9/curseforge/v1", missing)
                    .expect("build client");
            assert!(!client.has_api_key());

            let search_err = client
                .search(&SearchQuery::new("jei"))
                .await
                .expect_err("search must fail without API key");
            match search_err {
                MqError::MissingApiKey { service, env_var } => {
                    assert_eq!(service, "CurseForge");
                    assert_eq!(env_var, "MQ_CF_API_KEY");
                }
                other => panic!("expected MissingApiKey, got {other:?}"),
            }

            let files_err = client
                .get_mod_files(394468, Some("1.21.1"), Some(ModLoaderKind::Fabric))
                .await
                .expect_err("get_mod_files must fail without API key");
            assert!(matches!(
                files_err,
                MqError::MissingApiKey {
                    service: "CurseForge",
                    env_var: "MQ_CF_API_KEY"
                }
            ));
        }
    }

    #[test]
    fn curseforge_cdn_fallback_url_calculation_matches_edge_forgecdn_format() {
        assert_eq!(
            compute_curseforge_cdn_fallback_url(4572915, "jei-1.21.1-19.8.0.jar"),
            "https://edge.forgecdn.net/files/4572/915/jei-1.21.1-19.8.0.jar"
        );
        assert_eq!(
            compute_curseforge_cdn_fallback_url(5002, "sodium-0.6.0.jar"),
            "https://edge.forgecdn.net/files/5/2/sodium-0.6.0.jar"
        );
        assert_eq!(
            compute_curseforge_cdn_fallback_url(5400123, "My Mod 1.0.jar"),
            "https://edge.forgecdn.net/files/5400/123/My%20Mod%201.0.jar"
        );
    }

    #[tokio::test]
    async fn curseforge_search_and_mod_files_send_headers_and_compute_fallback_url() {
        let sha1_hex = "a94a8fe5ccb19ba61c4c0873d391e987982fbbd3";
        let sha1_clone = sha1_hex.to_owned();

        let (base_url, recorded) = spawn_mock_cf_server(move |path_and_query, headers| {
            assert_eq!(
                headers.get("x-api-key").map(String::as_str),
                Some("cf-secret-test-key")
            );
            assert_eq!(
                headers.get("user-agent").map(String::as_str),
                Some(MQ_USER_AGENT)
            );

            if path_and_query.starts_with("/curseforge/v1/mods/search?") {
                let body = json!({
                    "data": [
                        {
                            "id": 394468,
                            "gameId": 432,
                            "name": "Sodium",
                            "slug": "sodium",
                            "classId": 6,
                            "summary": "Modern rendering engine",
                            "downloadCount": 1500000,
                            "authors": [{ "name": "jellysquid3" }],
                            "logo": { "url": "https://media.forgecdn.net/avatars/sodium.png" },
                            "mainFileId": 4572915,
                            "latestFiles": [],
                            "latestFilesIndexes": [
                                {
                                    "gameVersion": "1.21.1",
                                    "fileId": 4572915,
                                    "modLoader": 4
                                }
                            ]
                        }
                    ]
                })
                .to_string();
                (200, body)
            } else if path_and_query.starts_with("/curseforge/v1/mods/394468/files") {
                // Return downloadUrl: null to verify edge.forgecdn.net fallback synthesis!
                let body = json!({
                    "data": [
                        {
                            "id": 4572915,
                            "modId": 394468,
                            "displayName": "Sodium 0.6.0",
                            "fileName": "sodium-fabric-0.6.0+mc1.21.1.jar",
                            "fileLength": 987654,
                            "downloadUrl": null,
                            "hashes": [
                                { "value": sha1_clone, "algo": 1 },
                                { "value": "098f6bcd4621d373cade4e832627b4f6", "algo": 2 }
                            ],
                            "gameVersions": ["1.21.1", "Fabric", "Client"],
                            "dependencies": [
                                { "modId": 306612, "relationType": 3 },
                                { "modId": 999999, "relationType": 2 }
                            ]
                        }
                    ]
                })
                .to_string();
                (200, body)
            } else {
                (404, json!({ "error": "NotFound" }).to_string())
            }
        })
        .await;

        let client =
            CurseForgeClient::with_base_url(&base_url, Some("cf-secret-test-key".to_owned()))
                .expect("build cf client");

        // Ensure Debug redacts API key
        let dbg_str = format!("{client:?}");
        assert!(!dbg_str.contains("cf-secret-test-key"));

        let query = SearchQuery::new("sodium")
            .with_project_kind(ProjectKind::Mod)
            .with_mc_version("1.21.1")
            .with_loader(ModLoaderKind::Fabric);

        let hits = client.search(&query).await.expect("cf search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source, CatalogSource::CurseForge);
        assert_eq!(hits[0].project_id, "394468");
        assert_eq!(hits[0].slug, "sodium");
        assert_eq!(hits[0].loaders, vec!["fabric"]);
        assert_eq!(hits[0].game_versions, vec!["1.21.1"]);
        assert_eq!(hits[0].latest_version.as_deref(), Some("4572915"));

        let versions = client
            .get_mod_files(394468, Some("1.21.1"), Some(ModLoaderKind::Fabric))
            .await
            .expect("cf get_mod_files");
        assert_eq!(versions.len(), 1);
        let ver = &versions[0];
        assert_eq!(ver.id, "4572915");
        assert_eq!(ver.project_id, "394468");
        assert_eq!(ver.dependencies.len(), 2);
        assert_eq!(ver.dependencies[0].project_id.as_deref(), Some("306612"));
        assert_eq!(ver.dependencies[0].kind, DependencyKind::Required);
        assert_eq!(ver.dependencies[1].kind, DependencyKind::Optional);

        let file = ver.primary_file().expect("primary file");
        assert_eq!(
            file.url,
            "https://edge.forgecdn.net/files/4572/915/sodium-fabric-0.6.0+mc1.21.1.jar"
        );
        assert_eq!(file.sha1.unwrap().to_hex(), sha1_hex);

        let reqs = recorded.lock().unwrap();
        assert_eq!(reqs.len(), 2);
        assert_eq!(
            reqs[0].headers.get("x-api-key").map(String::as_str),
            Some("cf-secret-test-key")
        );
        assert_eq!(
            reqs[0].headers.get("user-agent").map(String::as_str),
            Some(MQ_USER_AGENT)
        );
        let search_url = Url::parse(&format!(
            "http://localhost{}",
            reqs[0].request_line.split_whitespace().nth(1).unwrap()
        ))
        .unwrap();
        let qp: HashMap<String, String> = search_url.query_pairs().into_owned().collect();
        assert_eq!(qp.get("gameId").map(String::as_str), Some("432"));
        assert_eq!(qp.get("classId").map(String::as_str), Some("6"));
        assert_eq!(qp.get("searchFilter").map(String::as_str), Some("sodium"));
        assert_eq!(qp.get("gameVersion").map(String::as_str), Some("1.21.1"));
        assert_eq!(qp.get("modLoaderType").map(String::as_str), Some("4"));
    }
}
