//! Modrinth API v2 client (`https://api.modrinth.com/v2`) supporting faceted search,
//! project lookup, version filtering, and mock base-URL overrides.

use std::time::Duration;

use mq_core::{AppConfig, ModLoaderKind, MqError, MqResult, Sha1Digest, Sha512Digest};
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

/// Default production base URL for Modrinth API v2.
pub const DEFAULT_MODRINTH_API_URL: &str = "https://api.modrinth.com/v2";

/// Asynchronous HTTP client for Modrinth API v2.
#[derive(Debug, Clone)]
pub struct ModrinthClient {
    http: reqwest::Client,
    base_url: String,
}

impl Default for ModrinthClient {
    fn default() -> Self {
        Self::new().expect("default reqwest::Client for ModrinthClient should initialize")
    }
}

impl ModrinthClient {
    /// Creates a new [`ModrinthClient`] pointing at [`DEFAULT_MODRINTH_API_URL`].
    pub fn new() -> MqResult<Self> {
        Self::with_base_url(DEFAULT_MODRINTH_API_URL)
    }

    /// Creates a [`ModrinthClient`] honoring `config.mock_url` when configured.
    pub fn from_config(config: &AppConfig) -> MqResult<Self> {
        let base_url = match config
            .mock_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(mock) => {
                let trimmed = mock.trim_end_matches('/');
                if trimmed.ends_with("/modrinth/v2") || trimmed.ends_with("/v2") {
                    trimmed.to_owned()
                } else {
                    format!("{trimmed}/modrinth/v2")
                }
            }
            None => DEFAULT_MODRINTH_API_URL.to_owned(),
        };
        Self::with_base_url(base_url)
    }

    /// Creates a new [`ModrinthClient`] with a custom `base_url` (e.g. `"{mock}/modrinth/v2"`).
    pub fn with_base_url(base_url: impl Into<String>) -> MqResult<Self> {
        let http = reqwest::Client::builder()
            .user_agent(MQ_USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|err| {
                MqError::Network(format!("failed to build Modrinth HTTP client: {err}"))
            })?;
        Ok(Self::with_http_client(http, base_url))
    }

    /// Creates a [`ModrinthClient`] wrapping a pre-built [`reqwest::Client`] and `base_url`.
    pub fn with_http_client(http: reqwest::Client, base_url: impl Into<String>) -> Self {
        let normalized = base_url.into().trim().trim_end_matches('/').to_owned();
        Self {
            http,
            base_url: if normalized.is_empty() {
                DEFAULT_MODRINTH_API_URL.to_owned()
            } else {
                normalized
            },
        }
    }

    /// Returns the configured Modrinth API v2 base URL without a trailing slash.
    #[inline]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Builds the Modrinth v2 `facets` matrix (`Vec<Vec<String>>`) for a [`SearchQuery`].
    pub fn build_facets(q: &SearchQuery) -> Vec<Vec<String>> {
        let mut facets = vec![vec![format!(
            "project_type:{}",
            q.project_kind.as_modrinth_project_type()
        )]];

        if let Some(mc_ver) = q
            .mc_version
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            facets.push(vec![format!("versions:{mc_ver}")]);
        }

        if matches!(q.project_kind, ProjectKind::Mod | ProjectKind::Modpack) {
            if let Some(loader) = q.loader {
                if loader != ModLoaderKind::Vanilla {
                    facets.push(vec![format!("categories:{}", loader.as_str())]);
                }
            }
        }

        facets
    }

    /// Searches the Modrinth catalog (`GET /search`) using query text, pagination, and facets.
    pub async fn search(&self, q: &SearchQuery) -> MqResult<Vec<CatalogHit>> {
        let mut url = self.endpoint_url(&["search"])?;
        let limit = if q.limit == 0 { 20 } else { q.limit.min(100) };
        let facets_json = serde_json::to_string(&Self::build_facets(q))?;

        {
            let mut qp = url.query_pairs_mut();
            qp.append_pair("query", &q.query);
            qp.append_pair("limit", &limit.to_string());
            qp.append_pair("offset", &q.offset.to_string());
            qp.append_pair("facets", &facets_json);
        }

        debug!(url = %url, "searching Modrinth v2 catalog");
        let resp: ModrinthSearchResponseDto = self.get_json(url).await?;
        Ok(resp
            .hits
            .into_iter()
            .map(|hit| hit.into_catalog_hit(q.project_kind))
            .collect())
    }

    /// Fetches project metadata (`GET /project/{id_or_slug}`) as a [`CatalogHit`].
    pub async fn get_project(&self, id_or_slug: &str) -> MqResult<CatalogHit> {
        let url = self.endpoint_url(&["project", id_or_slug])?;
        debug!(url = %url, id_or_slug, "fetching Modrinth project metadata");
        let dto: ModrinthProjectDto = self.get_json(url).await?;
        Ok(dto.into_catalog_hit())
    }

    /// Fetches versions for a project (`GET /project/{id_or_slug}/version`), applying both
    /// server query parameters and client-side filtering for `mc_version` and `loader`.
    pub async fn get_project_versions(
        &self,
        id_or_slug: &str,
        mc_version: Option<&str>,
        loader: Option<ModLoaderKind>,
    ) -> MqResult<Vec<ProjectVersion>> {
        let mut url = self.endpoint_url(&["project", id_or_slug, "version"])?;
        let mc_filter = mc_version.map(str::trim).filter(|s| !s.is_empty());
        let loader_filter = loader.filter(|l| *l != ModLoaderKind::Vanilla);

        {
            let mut qp = url.query_pairs_mut();
            if let Some(ver) = mc_filter {
                let gv_json = serde_json::to_string(&[ver])?;
                qp.append_pair("game_versions", &gv_json);
            }
            if let Some(ldr) = loader_filter {
                let ldr_json = serde_json::to_string(&[ldr.as_str()])?;
                qp.append_pair("loaders", &ldr_json);
            }
        }

        debug!(url = %url, id_or_slug, ?mc_filter, ?loader_filter, "fetching Modrinth project versions");
        let raw_versions: Vec<ModrinthVersionDto> = self.get_json(url).await?;

        let mut out = Vec::with_capacity(raw_versions.len());
        for dto in raw_versions {
            let ver = dto.into_project_version()?;
            if let Some(target_mc) = mc_filter {
                if !ver
                    .game_versions
                    .iter()
                    .any(|gv| gv.eq_ignore_ascii_case(target_mc))
                {
                    continue;
                }
            }
            if let Some(target_loader) = loader_filter {
                if !ver
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

    /// Fetches a specific version by ID (`GET /version/{version_id}`).
    pub async fn get_version(&self, version_id: &str) -> MqResult<ProjectVersion> {
        let url = self.endpoint_url(&["version", version_id])?;
        debug!(url = %url, version_id, "fetching Modrinth version by id");
        let dto: ModrinthVersionDto = self.get_json(url).await?;
        dto.into_project_version()
    }

    /// Resolves a project and all transitive required dependencies into a deterministic download plan.
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
                "invalid Modrinth base URL '{}': {err}",
                self.base_url
            ))
        })?;
        {
            let mut path_segs = url.path_segments_mut().map_err(|()| {
                MqError::Metadata(format!(
                    "Modrinth base URL '{}' cannot be a base",
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

    async fn get_json<T: DeserializeOwned>(&self, url: Url) -> MqResult<T> {
        let url_str = url.to_string();
        let resp = self
            .http
            .get(url)
            .header(USER_AGENT, MQ_USER_AGENT)
            .header(ACCEPT, "application/json")
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
struct ModrinthSearchResponseDto {
    #[serde(default)]
    hits: Vec<ModrinthSearchHitDto>,
}

#[derive(Debug, Deserialize)]
struct ModrinthSearchHitDto {
    #[serde(alias = "id")]
    project_id: String,
    #[serde(default)]
    project_type: String,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    display_categories: Vec<String>,
    #[serde(default)]
    loaders: Vec<String>,
    #[serde(default, alias = "game_versions")]
    versions: Vec<String>,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    icon_url: Option<String>,
    #[serde(default)]
    latest_version: Option<String>,
}

impl ModrinthSearchHitDto {
    fn into_catalog_hit(self, fallback_kind: ProjectKind) -> CatalogHit {
        let project_kind =
            ProjectKind::from_modrinth_project_type(&self.project_type).unwrap_or(fallback_kind);
        let loaders = if !self.loaders.is_empty() {
            self.loaders
        } else {
            extract_loaders_from_categories(&self.categories, &self.display_categories)
        };
        let slug = if self.slug.is_empty() {
            self.project_id.clone()
        } else {
            self.slug
        };

        CatalogHit {
            source: CatalogSource::Modrinth,
            project_id: self.project_id,
            slug,
            title: self.title,
            description: self.description,
            author: self.author,
            downloads: self.downloads,
            icon_url: self.icon_url.filter(|u| !u.trim().is_empty()),
            project_kind,
            loaders,
            game_versions: self.versions,
            latest_version: self.latest_version.filter(|v| !v.trim().is_empty()),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ModrinthProjectDto {
    id: String,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    project_type: String,
    #[serde(default)]
    team: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    icon_url: Option<String>,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    loaders: Vec<String>,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    versions: Vec<String>,
}

impl ModrinthProjectDto {
    fn into_catalog_hit(self) -> CatalogHit {
        let project_kind =
            ProjectKind::from_modrinth_project_type(&self.project_type).unwrap_or(ProjectKind::Mod);
        let loaders = if !self.loaders.is_empty() {
            self.loaders
        } else {
            extract_loaders_from_categories(&self.categories, &[])
        };
        let latest_version = self.versions.last().cloned();
        let slug = if self.slug.is_empty() {
            self.id.clone()
        } else {
            self.slug
        };

        CatalogHit {
            source: CatalogSource::Modrinth,
            project_id: self.id,
            slug,
            title: self.title,
            description: self.description,
            author: self.team,
            downloads: self.downloads,
            icon_url: self.icon_url.filter(|u| !u.trim().is_empty()),
            project_kind,
            loaders,
            game_versions: self.game_versions,
            latest_version,
        }
    }
}

fn extract_loaders_from_categories(primary: &[String], secondary: &[String]) -> Vec<String> {
    let mut loaders = Vec::new();
    for cat in primary.iter().chain(secondary.iter()) {
        let lower = cat.trim().to_ascii_lowercase();
        if matches!(lower.as_str(), "fabric" | "quilt" | "forge" | "neoforge")
            && !loaders.contains(&lower)
        {
            loaders.push(lower);
        }
    }
    loaders
}

#[derive(Debug, Deserialize)]
struct ModrinthVersionDto {
    id: String,
    project_id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    version_number: String,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    loaders: Vec<String>,
    #[serde(default)]
    files: Vec<ModrinthFileDto>,
    #[serde(default)]
    dependencies: Vec<ModrinthDependencyDto>,
}

#[derive(Debug, Default, Deserialize)]
struct ModrinthHashesDto {
    #[serde(default)]
    sha1: Option<String>,
    #[serde(default)]
    sha512: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModrinthFileDto {
    #[serde(default)]
    hashes: ModrinthHashesDto,
    url: String,
    filename: String,
    #[serde(default)]
    primary: bool,
    #[serde(default)]
    size: u64,
}

#[derive(Debug, Deserialize)]
struct ModrinthDependencyDto {
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    version_id: Option<String>,
    #[serde(default)]
    file_name: Option<String>,
    #[serde(default)]
    dependency_type: String,
}

impl ModrinthVersionDto {
    fn into_project_version(self) -> MqResult<ProjectVersion> {
        let has_explicit_primary = self.files.iter().any(|f| f.primary);
        let mut files = Vec::with_capacity(self.files.len());
        for (idx, file_dto) in self.files.into_iter().enumerate() {
            let sha1 = match file_dto
                .hashes
                .sha1
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                Some(hex_str) => Some(Sha1Digest::from_hex(hex_str)?),
                None => None,
            };
            let sha512 = match file_dto
                .hashes
                .sha512
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                Some(hex_str) => Some(Sha512Digest::from_hex(hex_str)?),
                None => None,
            };
            files.push(ProjectVersionFile {
                url: file_dto.url,
                filename: file_dto.filename,
                size: file_dto.size,
                sha1,
                sha512,
                primary: if has_explicit_primary {
                    file_dto.primary
                } else {
                    idx == 0
                },
            });
        }

        let dependencies = self
            .dependencies
            .into_iter()
            .filter_map(|dep| {
                let kind = DependencyKind::from_modrinth_type(&dep.dependency_type)?;
                Some(ModDependencyRef {
                    project_id: dep.project_id.filter(|s| !s.trim().is_empty()),
                    version_id: dep.version_id.filter(|s| !s.trim().is_empty()),
                    file_name: dep.file_name.filter(|s| !s.trim().is_empty()),
                    kind,
                })
            })
            .collect();

        let name = if self.name.is_empty() {
            self.version_number.clone()
        } else {
            self.name
        };

        Ok(ProjectVersion {
            id: self.id,
            project_id: self.project_id,
            name,
            version_number: self.version_number,
            game_versions: self.game_versions,
            loaders: self.loaders,
            files,
            dependencies,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    #[derive(Debug, Clone)]
    struct RecordedRequest {
        request_line: String,
        user_agent: Option<String>,
    }

    async fn spawn_mock_http_server<F>(handler: F) -> (String, Arc<Mutex<Vec<RecordedRequest>>>)
    where
        F: Fn(&str) -> (u16, String) + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral mock listener");
        let addr = listener.local_addr().expect("local addr");
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let recorded_clone = Arc::clone(&recorded);
        let handler = Arc::new(handler);

        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let recorded_req = Arc::clone(&recorded_clone);
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
                    let mut user_agent = None;
                    for line in lines {
                        if line.is_empty() {
                            break;
                        }
                        if let Some((k, v)) = line.split_once(':') {
                            if k.trim().eq_ignore_ascii_case("user-agent") {
                                user_agent = Some(v.trim().to_owned());
                            }
                        }
                    }
                    recorded_req.lock().unwrap().push(RecordedRequest {
                        request_line: request_line.clone(),
                        user_agent,
                    });

                    let path_and_query = request_line.split_whitespace().nth(1).unwrap_or("/");
                    let (status, body) = h(path_and_query);
                    let status_text = if status == 200 { "OK" } else { "Not Found" };
                    let resp = format!(
                        "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                });
            }
        });

        (format!("http://{addr}/modrinth/v2"), recorded)
    }

    #[test]
    fn modrinth_facet_builder_formats_project_type_version_and_loader() {
        let mod_query = SearchQuery::new("sodium")
            .with_project_kind(ProjectKind::Mod)
            .with_mc_version("1.21.1")
            .with_loader(ModLoaderKind::Fabric);
        assert_eq!(
            ModrinthClient::build_facets(&mod_query),
            vec![
                vec!["project_type:mod".to_owned()],
                vec!["versions:1.21.1".to_owned()],
                vec!["categories:fabric".to_owned()],
            ]
        );

        // Shaders and resourcepacks should not emit a mod loader category facet even if loader is set.
        let shader_query = SearchQuery::new("complementary")
            .with_project_kind(ProjectKind::Shader)
            .with_mc_version("1.21.1")
            .with_loader(ModLoaderKind::Fabric);
        assert_eq!(
            ModrinthClient::build_facets(&shader_query),
            vec![
                vec!["project_type:shader".to_owned()],
                vec!["versions:1.21.1".to_owned()],
            ]
        );
    }

    #[tokio::test]
    async fn modrinth_search_sends_mandatory_user_agent_and_parses_facets() {
        let (base_url, recorded) = spawn_mock_http_server(|path_and_query| {
            assert!(path_and_query.starts_with("/modrinth/v2/search?"));
            let body = json!({
                "hits": [
                    {
                        "project_id": "AANobbMI",
                        "project_type": "mod",
                        "slug": "sodium",
                        "author": "jellysquid3",
                        "title": "Sodium",
                        "description": "Modern rendering engine",
                        "categories": ["fabric", "quilt", "optimization"],
                        "versions": ["1.21.1", "1.20.4"],
                        "downloads": 42500000,
                        "icon_url": "https://cdn.modrinth.com/data/AANobbMI/icon.png",
                        "latest_version": "ver-sodium-1211"
                    }
                ],
                "offset": 0,
                "limit": 10,
                "total_hits": 1
            })
            .to_string();
            (200, body)
        })
        .await;

        let client = ModrinthClient::with_base_url(&base_url).expect("create modrinth client");
        let query = SearchQuery::new("sodium")
            .with_project_kind(ProjectKind::Mod)
            .with_mc_version("1.21.1")
            .with_loader(ModLoaderKind::Fabric)
            .with_limit(10);

        let hits = client.search(&query).await.expect("modrinth search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source, CatalogSource::Modrinth);
        assert_eq!(hits[0].project_id, "AANobbMI");
        assert_eq!(hits[0].slug, "sodium");
        assert_eq!(hits[0].title, "Sodium");
        assert_eq!(hits[0].project_kind, ProjectKind::Mod);
        assert_eq!(hits[0].loaders, vec!["fabric", "quilt"]);
        assert_eq!(hits[0].game_versions, vec!["1.21.1", "1.20.4"]);

        let reqs = recorded.lock().unwrap();
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].user_agent.as_deref(), Some(MQ_USER_AGENT));

        let parsed_url = Url::parse(&format!(
            "http://localhost{}",
            reqs[0].request_line.split_whitespace().nth(1).unwrap()
        ))
        .expect("parse recorded request URL");
        let qp: std::collections::HashMap<String, String> =
            parsed_url.query_pairs().into_owned().collect();
        assert_eq!(qp.get("query").map(String::as_str), Some("sodium"));
        assert_eq!(qp.get("limit").map(String::as_str), Some("10"));
        assert_eq!(
            qp.get("facets").map(String::as_str),
            Some("[[\"project_type:mod\"],[\"versions:1.21.1\"],[\"categories:fabric\"]]")
        );
    }

    #[tokio::test]
    async fn modrinth_get_project_versions_applies_client_side_filter_safety() {
        let sha1_hex = "a94a8fe5ccb19ba61c4c0873d391e987982fbbd3";
        let sha512_hex = "5a".repeat(64);
        let sha1_clone = sha1_hex.to_owned();
        let sha512_clone = sha512_hex.clone();

        let (base_url, recorded) = spawn_mock_http_server(move |_path| {
            // Return both a 1.21.1 Fabric version and a 1.20.1 Forge version to verify client-side filtering.
            let body = json!([
                {
                    "id": "ver-sodium-1211-fabric",
                    "project_id": "AANobbMI",
                    "name": "Sodium 0.6.0",
                    "version_number": "0.6.0+mc1.21.1",
                    "game_versions": ["1.21.1"],
                    "loaders": ["fabric", "quilt"],
                    "dependencies": [
                        {
                            "project_id": "P7dR8mSH",
                            "version_id": "ver-fapi-1211",
                            "dependency_type": "required"
                        }
                    ],
                    "files": [
                        {
                            "hashes": {
                                "sha1": sha1_clone,
                                "sha512": sha512_clone
                            },
                            "url": "https://cdn.modrinth.com/sodium-0.6.0.jar",
                            "filename": "sodium-0.6.0.jar",
                            "primary": true,
                            "size": 102400
                        }
                    ]
                },
                {
                    "id": "ver-sodium-1201-forge",
                    "project_id": "AANobbMI",
                    "name": "Sodium 0.5.0 Forge",
                    "version_number": "0.5.0+mc1.20.1",
                    "game_versions": ["1.20.1"],
                    "loaders": ["forge"],
                    "dependencies": [],
                    "files": []
                }
            ])
            .to_string();
            (200, body)
        })
        .await;

        let client = ModrinthClient::with_base_url(&base_url).expect("client");
        let versions = client
            .get_project_versions("sodium", Some("1.21.1"), Some(ModLoaderKind::Fabric))
            .await
            .expect("get versions");

        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].id, "ver-sodium-1211-fabric");
        assert_eq!(versions[0].dependencies.len(), 1);
        assert_eq!(versions[0].dependencies[0].kind, DependencyKind::Required);
        assert_eq!(
            versions[0].dependencies[0].project_id.as_deref(),
            Some("P7dR8mSH")
        );
        let file = versions[0].primary_file().expect("primary file");
        assert_eq!(file.filename, "sodium-0.6.0.jar");
        assert_eq!(file.sha1.unwrap().to_hex(), sha1_hex);
        assert_eq!(file.sha512.unwrap().to_hex(), sha512_hex);

        let reqs = recorded.lock().unwrap();
        assert_eq!(reqs[0].user_agent.as_deref(), Some(MQ_USER_AGENT));
    }
}
