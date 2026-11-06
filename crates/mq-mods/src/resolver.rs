//! Automatic recursive required-dependency graph resolver for Modrinth and CurseForge.
//!
//! Traverses `DependencyKind::Required` edges (`dependency_type == "required"` on Modrinth,
//! `relationType == 3` on CurseForge) using a `visited_projects: BTreeSet<String>` guard
//! against cycles and duplicate downloads, returning a deterministic topological download plan
//! (dependencies first, root mod last).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::future::Future;

use mq_core::{ModLoaderKind, MqError, MqResult};
use tracing::debug;

use crate::curseforge::CurseForgeClient;
use crate::models::{
    DependencyKind, ProjectKind, ProjectVersion, ResolvedModDownload, SearchQuery,
};
use crate::modrinth::ModrinthClient;

/// Conversion trait allowing [`resolve_mod_with_dependencies`] to accept `&str`, `String`,
/// or `Option<&str>` for the `mc_version` filter parameter.
pub trait IntoMcVersionFilter {
    fn into_mc_version_filter(self) -> Option<String>;
}

impl IntoMcVersionFilter for &str {
    #[inline]
    fn into_mc_version_filter(self) -> Option<String> {
        let trimmed = self.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    }
}

impl IntoMcVersionFilter for &String {
    #[inline]
    fn into_mc_version_filter(self) -> Option<String> {
        self.as_str().into_mc_version_filter()
    }
}

impl IntoMcVersionFilter for String {
    #[inline]
    fn into_mc_version_filter(self) -> Option<String> {
        self.as_str().into_mc_version_filter()
    }
}

impl IntoMcVersionFilter for Option<&str> {
    #[inline]
    fn into_mc_version_filter(self) -> Option<String> {
        self.and_then(IntoMcVersionFilter::into_mc_version_filter)
    }
}

/// Conversion trait allowing [`resolve_mod_with_dependencies`] to accept either
/// [`ModLoaderKind`] or `Option<ModLoaderKind>` for the `loader` filter parameter.
pub trait IntoLoaderFilter {
    fn into_loader_filter(self) -> Option<ModLoaderKind>;
}

impl IntoLoaderFilter for ModLoaderKind {
    #[inline]
    fn into_loader_filter(self) -> Option<ModLoaderKind> {
        Some(self)
    }
}

impl IntoLoaderFilter for Option<ModLoaderKind> {
    #[inline]
    fn into_loader_filter(self) -> Option<ModLoaderKind> {
        self
    }
}

/// Abstract catalog backend capable of fetching compatible project versions for dependency resolution.
pub trait ModCatalogProvider: Sync {
    /// Fetches versions for `project_id_or_slug` compatible with `mc_version` and `loader`.
    fn fetch_compatible_versions<'a>(
        &'a self,
        project_id_or_slug: &'a str,
        mc_version: Option<&'a str>,
        loader: Option<ModLoaderKind>,
    ) -> impl Future<Output = MqResult<Vec<ProjectVersion>>> + Send + 'a;

    /// Fetches a specific version by ID when a dependency pins `version_id` without `project_id`.
    fn fetch_version_by_id<'a>(
        &'a self,
        version_id: &'a str,
    ) -> impl Future<Output = MqResult<Option<ProjectVersion>>> + Send + 'a {
        let _ = version_id;
        async { Ok(None) }
    }
}

impl ModCatalogProvider for ModrinthClient {
    async fn fetch_compatible_versions<'a>(
        &'a self,
        project_id_or_slug: &'a str,
        mc_version: Option<&'a str>,
        loader: Option<ModLoaderKind>,
    ) -> MqResult<Vec<ProjectVersion>> {
        self.get_project_versions(project_id_or_slug, mc_version, loader)
            .await
    }

    async fn fetch_version_by_id<'a>(
        &'a self,
        version_id: &'a str,
    ) -> MqResult<Option<ProjectVersion>> {
        self.get_version(version_id).await.map(Some)
    }
}

impl ModCatalogProvider for CurseForgeClient {
    async fn fetch_compatible_versions<'a>(
        &'a self,
        project_id_or_slug: &'a str,
        mc_version: Option<&'a str>,
        loader: Option<ModLoaderKind>,
    ) -> MqResult<Vec<ProjectVersion>> {
        let trimmed = project_id_or_slug.trim();
        let mod_id = if let Ok(numeric_id) = trimmed.parse::<u64>() {
            numeric_id
        } else {
            let mut q = SearchQuery::new(trimmed).with_project_kind(ProjectKind::Mod);
            if let Some(ver) = mc_version {
                q = q.with_mc_version(ver);
            }
            if let Some(ldr) = loader {
                q = q.with_loader(ldr);
            }
            let hits = self.search(&q).await?;
            let matched = hits
                .iter()
                .find(|h| h.slug.eq_ignore_ascii_case(trimmed))
                .or_else(|| hits.first())
                .ok_or_else(|| {
                    MqError::DependencyResolution(format!("CurseForge mod '{trimmed}' not found"))
                })?;
            matched.project_id.parse::<u64>().map_err(|err| {
                MqError::DependencyResolution(format!(
                    "invalid CurseForge project_id '{}' for '{trimmed}': {err}",
                    matched.project_id
                ))
            })?
        };

        self.get_mod_files(mod_id, mc_version, loader).await
    }
}

#[derive(Debug, Clone)]
struct PendingMod {
    project_id: Option<String>,
    version_id: Option<String>,
    is_dependency: bool,
}

#[derive(Debug, Clone)]
struct ResolvedNode {
    canonical_project_id: String,
    aliases: Vec<String>,
    download: ResolvedModDownload,
    required_deps: Vec<String>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum VisitState {
    Unvisited,
    Visiting,
    Visited,
}

/// Resolves a root mod (`project_id_or_slug`) and all of its transitive `DependencyKind::Required`
/// dependencies for the target `mc_version` and `loader`.
///
/// Tracks `visited_projects: BTreeSet<String>` to prevent cycles and duplicate downloads,
/// returning a deterministic `Vec<ResolvedModDownload>` with required dependencies ordered first
/// and the root mod ordered last.
pub async fn resolve_mod_with_dependencies<P, V, L>(
    provider: &P,
    project_id_or_slug: &str,
    mc_version: V,
    loader: L,
) -> MqResult<Vec<ResolvedModDownload>>
where
    P: ModCatalogProvider + ?Sized,
    V: IntoMcVersionFilter,
    L: IntoLoaderFilter,
{
    let root_id = project_id_or_slug.trim();
    if root_id.is_empty() {
        return Err(MqError::DependencyResolution(
            "root project_id_or_slug cannot be empty".to_owned(),
        ));
    }

    let mc_ver_filter = mc_version.into_mc_version_filter();
    let loader_filter = loader.into_loader_filter();

    let mut visited_projects: BTreeSet<String> = BTreeSet::new();
    let mut queue: VecDeque<PendingMod> = VecDeque::new();
    let mut nodes: Vec<ResolvedNode> = Vec::new();

    queue.push_back(PendingMod {
        project_id: Some(root_id.to_owned()),
        version_id: None,
        is_dependency: false,
    });

    while let Some(pending) = queue.pop_front() {
        let lookup_key = pending
            .project_id
            .as_deref()
            .or(pending.version_id.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                MqError::DependencyResolution(
                    "dependency entry missing both project_id and version_id".to_owned(),
                )
            })?
            .to_owned();

        if visited_projects.contains(&lookup_key)
            || visited_projects.contains(&lookup_key.to_ascii_lowercase())
        {
            debug!(
                lookup_key,
                "skipping already-visited project in dependency graph"
            );
            continue;
        }

        let selected_version = if let Some(ref proj_key) = pending.project_id {
            let compatible = provider
                .fetch_compatible_versions(proj_key, mc_ver_filter.as_deref(), loader_filter)
                .await?;

            if compatible.is_empty() {
                return Err(MqError::DependencyResolution(format!(
                    "no compatible version found for project '{proj_key}' (mc_version={:?}, loader={:?})",
                    mc_ver_filter, loader_filter
                )));
            }

            if let Some(ref pinned_vid) = pending.version_id {
                compatible
                    .iter()
                    .find(|v| v.id == *pinned_vid)
                    .cloned()
                    .unwrap_or_else(|| compatible.into_iter().next().expect("non-empty versions"))
            } else {
                compatible.into_iter().next().expect("non-empty versions")
            }
        } else if let Some(ref vid) = pending.version_id {
            provider.fetch_version_by_id(vid).await?.ok_or_else(|| {
                MqError::DependencyResolution(format!(
                    "unable to resolve pinned dependency version '{vid}'"
                ))
            })?
        } else {
            return Err(MqError::DependencyResolution(
                "dependency reference has neither project_id nor version_id".to_owned(),
            ));
        };

        let canonical_id = selected_version.project_id.trim().to_owned();

        // Record both the lookup key (e.g. slug "sodium") and the canonical project_id (e.g. "AANobbMI").
        visited_projects.insert(lookup_key.clone());
        visited_projects.insert(lookup_key.to_ascii_lowercase());

        if nodes
            .iter()
            .any(|n| n.canonical_project_id.eq_ignore_ascii_case(&canonical_id))
        {
            debug!(
                lookup_key,
                canonical_id,
                "canonical project_id already resolved under another alias; skipping duplicate"
            );
            visited_projects.insert(canonical_id.clone());
            visited_projects.insert(canonical_id.to_ascii_lowercase());
            continue;
        }

        visited_projects.insert(canonical_id.clone());
        visited_projects.insert(canonical_id.to_ascii_lowercase());

        let file = selected_version.primary_file().ok_or_else(|| {
            MqError::DependencyResolution(format!(
                "version '{}' of project '{}' has no downloadable files",
                selected_version.id, canonical_id
            ))
        })?;

        let download = ResolvedModDownload {
            project_id: canonical_id.clone(),
            version_id: selected_version.id.clone(),
            filename: file.filename.clone(),
            url: file.url.clone(),
            size: file.size,
            sha1: file.sha1,
            sha512: file.sha512,
            is_dependency: pending.is_dependency,
        };

        let mut required_deps = Vec::new();
        for dep in &selected_version.dependencies {
            if dep.kind != DependencyKind::Required {
                continue;
            }
            let dep_key = dep
                .project_id
                .as_deref()
                .or(dep.version_id.as_deref())
                .map(str::trim)
                .filter(|s| !s.is_empty());

            if let Some(key) = dep_key {
                let key_owned = key.to_owned();
                if !required_deps.contains(&key_owned) {
                    required_deps.push(key_owned.clone());
                }
                if !visited_projects.contains(&key_owned)
                    && !visited_projects.contains(&key_owned.to_ascii_lowercase())
                {
                    queue.push_back(PendingMod {
                        project_id: dep.project_id.clone(),
                        version_id: dep.version_id.clone(),
                        is_dependency: true,
                    });
                }
            }
        }

        let mut aliases = vec![
            canonical_id.clone(),
            canonical_id.to_ascii_lowercase(),
            lookup_key.clone(),
            lookup_key.to_ascii_lowercase(),
            selected_version.id.clone(),
        ];
        aliases.dedup();

        nodes.push(ResolvedNode {
            canonical_project_id: canonical_id,
            aliases,
            download,
            required_deps,
        });
    }

    // Build deterministic post-order topological sort (dependencies first, root mod last).
    let mut alias_to_idx: BTreeMap<String, usize> = BTreeMap::new();
    for (idx, node) in nodes.iter().enumerate() {
        for alias in &node.aliases {
            alias_to_idx.entry(alias.clone()).or_insert(idx);
        }
    }

    let mut states = vec![VisitState::Unvisited; nodes.len()];
    let mut plan = Vec::with_capacity(nodes.len());

    fn topo_visit(
        idx: usize,
        nodes: &[ResolvedNode],
        alias_to_idx: &BTreeMap<String, usize>,
        states: &mut [VisitState],
        plan: &mut Vec<ResolvedModDownload>,
    ) {
        match states[idx] {
            VisitState::Visited | VisitState::Visiting => return,
            VisitState::Unvisited => states[idx] = VisitState::Visiting,
        }

        for dep_key in &nodes[idx].required_deps {
            let resolved_idx = alias_to_idx
                .get(dep_key)
                .or_else(|| alias_to_idx.get(&dep_key.to_ascii_lowercase()))
                .copied();
            if let Some(dep_idx) = resolved_idx {
                topo_visit(dep_idx, nodes, alias_to_idx, states, plan);
            }
        }

        states[idx] = VisitState::Visited;
        plan.push(nodes[idx].download.clone());
    }

    for start_idx in 0..nodes.len() {
        topo_visit(start_idx, &nodes, &alias_to_idx, &mut states, &mut plan);
    }

    Ok(plan)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;
    use crate::models::MQ_USER_AGENT;

    async fn spawn_mock_resolver_server<F>(handler: F) -> (String, Arc<Mutex<Vec<String>>>)
    where
        F: Fn(&str) -> (u16, String) + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock resolver listener");
        let addr = listener.local_addr().expect("local addr");
        let paths = Arc::new(Mutex::new(Vec::new()));
        let paths_clone = Arc::clone(&paths);
        let handler = Arc::new(handler);

        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let p_log = Arc::clone(&paths_clone);
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
                    let mut saw_ua = false;
                    for line in lines {
                        if line.is_empty() {
                            break;
                        }
                        if let Some((k, v)) = line.split_once(':') {
                            if k.trim().eq_ignore_ascii_case("user-agent")
                                && v.trim() == MQ_USER_AGENT
                            {
                                saw_ua = true;
                            }
                        }
                    }
                    assert!(
                        saw_ua,
                        "resolver requests must include mandatory User-Agent"
                    );

                    let path = request_line
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or("/")
                        .to_owned();
                    p_log.lock().unwrap().push(path.clone());

                    let (status, body) = h(&path);
                    let status_text = if status == 200 { "OK" } else { "Not Found" };
                    let resp = format!(
                        "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                });
            }
        });

        (format!("http://{addr}"), paths)
    }

    #[tokio::test]
    async fn resolve_sodium_automatically_pulls_required_fabric_api_dependency_first() {
        let sha1_sodium = "a94a8fe5ccb19ba61c4c0873d391e987982fbbd3";
        let sha1_fapi = "1111111111111111111111111111111111111111";

        let (base, _requests) = spawn_mock_resolver_server(move |path| {
            if path.starts_with("/modrinth/v2/project/sodium/version")
                || path.starts_with("/modrinth/v2/project/AANobbMI/version")
            {
                let body = json!([
                    {
                        "id": "ver-sodium-1211",
                        "project_id": "AANobbMI",
                        "name": "Sodium 0.6.0",
                        "version_number": "0.6.0+mc1.21.1",
                        "game_versions": ["1.21.1"],
                        "loaders": ["fabric"],
                        "dependencies": [
                            {
                                "project_id": "P7dR8mSH",
                                "version_id": "ver-fabric-api-1211",
                                "dependency_type": "required"
                            },
                            {
                                "project_id": "OPT_IGNORED",
                                "dependency_type": "optional"
                            }
                        ],
                        "files": [
                            {
                                "hashes": { "sha1": sha1_sodium },
                                "url": "https://cdn.modrinth.com/sodium-0.6.0.jar",
                                "filename": "sodium-0.6.0.jar",
                                "primary": true,
                                "size": 512000
                            }
                        ]
                    }
                ])
                .to_string();
                (200, body)
            } else if path.starts_with("/modrinth/v2/project/P7dR8mSH/version") {
                let body = json!([
                    {
                        "id": "ver-fabric-api-1211",
                        "project_id": "P7dR8mSH",
                        "name": "Fabric API 0.102.0",
                        "version_number": "0.102.0+1.21.1",
                        "game_versions": ["1.21.1"],
                        "loaders": ["fabric"],
                        "dependencies": [],
                        "files": [
                            {
                                "hashes": { "sha1": sha1_fapi },
                                "url": "https://cdn.modrinth.com/fabric-api-0.102.0.jar",
                                "filename": "fabric-api-0.102.0.jar",
                                "primary": true,
                                "size": 2048000
                            }
                        ]
                    }
                ])
                .to_string();
                (200, body)
            } else {
                (404, json!({ "error": "not_found" }).to_string())
            }
        })
        .await;

        let client =
            ModrinthClient::with_base_url(format!("{base}/modrinth/v2")).expect("modrinth client");
        let plan = resolve_mod_with_dependencies(
            &client,
            "sodium",
            Some("1.21.1"),
            Some(ModLoaderKind::Fabric),
        )
        .await
        .expect("resolve sodium + fabric-api");

        assert_eq!(plan.len(), 2);

        // Required dependency (fabric-api) must come first with is_dependency = true.
        assert_eq!(plan[0].project_id, "P7dR8mSH");
        assert_eq!(plan[0].version_id, "ver-fabric-api-1211");
        assert_eq!(plan[0].filename, "fabric-api-0.102.0.jar");
        assert!(plan[0].is_dependency);
        assert_eq!(plan[0].sha1.unwrap().to_hex(), sha1_fapi);

        // Root mod (sodium) must come last with is_dependency = false.
        assert_eq!(plan[1].project_id, "AANobbMI");
        assert_eq!(plan[1].version_id, "ver-sodium-1211");
        assert_eq!(plan[1].filename, "sodium-0.6.0.jar");
        assert!(!plan[1].is_dependency);
        assert_eq!(plan[1].sha1.unwrap().to_hex(), sha1_sodium);
    }

    #[tokio::test]
    async fn resolver_handles_dependency_cycles_and_diamonds_without_duplicate_downloads() {
        // Graph:
        //   root-slug (project_id = "PROJ_A") -> requires PROJ_B and PROJ_C
        //   PROJ_B -> requires PROJ_D and PROJ_A (cycle back to root by canonical project_id!)
        //   PROJ_C -> requires PROJ_D (diamond dependency on PROJ_D!)
        //   PROJ_D -> requires PROJ_B (second cycle B <-> D!)
        let (base, requests) = spawn_mock_resolver_server(move |path| {
            let body = if path.starts_with("/modrinth/v2/project/root-slug/version")
                || path.starts_with("/modrinth/v2/project/PROJ_A/version")
            {
                json!([{
                    "id": "ver-a",
                    "project_id": "PROJ_A",
                    "name": "Mod A",
                    "version_number": "1.0.0",
                    "game_versions": ["1.21.1"],
                    "loaders": ["fabric"],
                    "dependencies": [
                        { "project_id": "PROJ_B", "dependency_type": "required" },
                        { "project_id": "PROJ_C", "dependency_type": "required" }
                    ],
                    "files": [{
                        "hashes": {},
                        "url": "https://cdn.modrinth.com/mod-a.jar",
                        "filename": "mod-a.jar",
                        "primary": true,
                        "size": 1000
                    }]
                }])
            } else if path.starts_with("/modrinth/v2/project/PROJ_B/version") {
                json!([{
                    "id": "ver-b",
                    "project_id": "PROJ_B",
                    "name": "Mod B",
                    "version_number": "1.0.0",
                    "game_versions": ["1.21.1"],
                    "loaders": ["fabric"],
                    "dependencies": [
                        { "project_id": "PROJ_D", "dependency_type": "required" },
                        { "project_id": "PROJ_A", "dependency_type": "required" }
                    ],
                    "files": [{
                        "hashes": {},
                        "url": "https://cdn.modrinth.com/mod-b.jar",
                        "filename": "mod-b.jar",
                        "primary": true,
                        "size": 2000
                    }]
                }])
            } else if path.starts_with("/modrinth/v2/project/PROJ_C/version") {
                json!([{
                    "id": "ver-c",
                    "project_id": "PROJ_C",
                    "name": "Mod C",
                    "version_number": "1.0.0",
                    "game_versions": ["1.21.1"],
                    "loaders": ["fabric"],
                    "dependencies": [
                        { "project_id": "PROJ_D", "dependency_type": "required" }
                    ],
                    "files": [{
                        "hashes": {},
                        "url": "https://cdn.modrinth.com/mod-c.jar",
                        "filename": "mod-c.jar",
                        "primary": true,
                        "size": 3000
                    }]
                }])
            } else if path.starts_with("/modrinth/v2/project/PROJ_D/version") {
                json!([{
                    "id": "ver-d",
                    "project_id": "PROJ_D",
                    "name": "Mod D",
                    "version_number": "1.0.0",
                    "game_versions": ["1.21.1"],
                    "loaders": ["fabric"],
                    "dependencies": [
                        { "project_id": "PROJ_B", "dependency_type": "required" }
                    ],
                    "files": [{
                        "hashes": {},
                        "url": "https://cdn.modrinth.com/mod-d.jar",
                        "filename": "mod-d.jar",
                        "primary": true,
                        "size": 4000
                    }]
                }])
            } else {
                return (404, json!({"error": "not_found"}).to_string());
            };
            (200, body.to_string())
        })
        .await;

        let client =
            ModrinthClient::with_base_url(format!("{base}/modrinth/v2")).expect("modrinth client");
        let plan = client
            .resolve_mod_with_dependencies("root-slug", "1.21.1", ModLoaderKind::Fabric)
            .await
            .expect("resolve cyclic + diamond graph cleanly");

        let ids: Vec<&str> = plan.iter().map(|d| d.project_id.as_str()).collect();
        assert_eq!(ids, vec!["PROJ_D", "PROJ_B", "PROJ_C", "PROJ_A"]);
        assert!(plan[0].is_dependency);
        assert!(plan[1].is_dependency);
        assert!(plan[2].is_dependency);
        assert!(!plan[3].is_dependency);

        // Each of the 4 unique projects was fetched exactly once.
        assert_eq!(requests.lock().unwrap().len(), 4);
    }

    #[tokio::test]
    async fn curseforge_resolver_follows_relation_type_3_required_dependencies() {
        let (base, _requests) = spawn_mock_resolver_server(move |path| {
            if path.starts_with("/curseforge/v1/mods/394468/files") {
                let body = json!({
                    "data": [{
                        "id": 5002,
                        "modId": 394468,
                        "fileName": "sodium-0.6.0.jar",
                        "fileLength": 500000,
                        "downloadUrl": null,
                        "hashes": [],
                        "gameVersions": ["1.21.1", "Fabric"],
                        "dependencies": [
                            { "modId": 306612, "relationType": 3 }
                        ]
                    }]
                });
                (200, body.to_string())
            } else if path.starts_with("/curseforge/v1/mods/306612/files") {
                let body = json!({
                    "data": [{
                        "id": 5003,
                        "modId": 306612,
                        "fileName": "fabric-api-0.102.0.jar",
                        "fileLength": 2000000,
                        "downloadUrl": null,
                        "hashes": [],
                        "gameVersions": ["1.21.1", "Fabric"],
                        "dependencies": []
                    }]
                });
                (200, body.to_string())
            } else {
                (404, json!({"error": "NotFound"}).to_string())
            }
        })
        .await;

        let cf = CurseForgeClient::with_base_url(
            format!("{base}/curseforge/v1"),
            Some("test-cf-api-key".to_owned()),
        )
        .expect("cf client");

        let plan = resolve_mod_with_dependencies(&cf, "394468", "1.21.1", ModLoaderKind::Fabric)
            .await
            .expect("resolve cf sodium -> fabric-api");

        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].project_id, "306612");
        assert_eq!(
            plan[0].url,
            "https://edge.forgecdn.net/files/5/3/fabric-api-0.102.0.jar"
        );
        assert!(plan[0].is_dependency);

        assert_eq!(plan[1].project_id, "394468");
        assert_eq!(
            plan[1].url,
            "https://edge.forgecdn.net/files/5/2/sodium-0.6.0.jar"
        );
        assert!(!plan[1].is_dependency);
    }
}
