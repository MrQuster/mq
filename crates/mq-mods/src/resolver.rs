//! Automatic recursive dependency graph resolver and unified [`ContentProvider`] trait
//! for Modrinth and CurseForge.
//!
//! Supports all four dependency relation types:
//! - [`DependencyKind::Required`]: automatically resolved and installed in topological order.
//! - [`DependencyKind::Optional`]: returned in [`DependencyResolutionPlan::optional_dependencies`]
//!   for user selection via checkboxes (and installed if selected in [`ResolveOptions`]).
//! - [`DependencyKind::Incompatible`]: blocks installation with a clear Russian error message.
//! - [`DependencyKind::Embedded`]: skipped (already bundled inside the parent archive).
//!
//! Also provides Minecraft version matching without `1.x.y` semver assumptions (supporting `26.x`
//! such as `26.1`, `26.1.1`) and `Quilt` instance compatibility with `Fabric` mods.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::future::Future;

use mq_core::{ModLoaderKind, MqError, MqResult};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::curseforge::{
    build_curseforge_manual_download_url, CurseForgeClient, CurseForgeSearchOptions,
    CurseForgeSortField, CurseForgeSortOrder,
};
use crate::models::{
    BlockedDownload, CatalogHit, CatalogSource, DependencyKind, ModDependencyRef, ProjectKind,
    ProjectVersion, ResolvedModDownload, SearchQuery, SearchSort,
};
use crate::modrinth::{ModrinthClient, ModrinthSearchIndex, ModrinthSearchParams};

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

/// Checks whether a version's `game_versions` list is compatible with `target_mc`
/// without parsing as legacy `1.x.y` semver (supports `26.x` versions such as `26.1`, `26.1.1`).
pub fn is_mc_version_compatible(game_versions: &[String], target_mc: Option<&str>) -> bool {
    let Some(target) = target_mc.map(str::trim).filter(|s| !s.is_empty()) else {
        return true;
    };
    if game_versions.is_empty() {
        return true;
    }
    ModrinthClient::matches_mc_version(game_versions, target)
}

/// Checks whether a version's `loaders` list is compatible with `target_loader`.
/// A `Quilt` instance is compatible with both `Quilt` and `Fabric` mods.
pub fn is_loader_compatible(
    version_loaders: &[String],
    target_loader: Option<ModLoaderKind>,
) -> bool {
    let Some(loader) = target_loader else {
        return true;
    };
    if version_loaders.is_empty() {
        return true;
    }
    ModrinthClient::matches_loader(version_loaders, loader)
}

/// Filters and orders `versions` for compatibility with `mc_version` (including `26.x`)
/// and `loader` (preferring native `quilt` versions before `fabric` fallback on `Quilt` instances).
pub fn filter_compatible_versions(
    versions: &[ProjectVersion],
    mc_version: Option<&str>,
    loader: Option<ModLoaderKind>,
) -> Vec<ProjectVersion> {
    let mut filtered: Vec<ProjectVersion> = versions
        .iter()
        .filter(|v| {
            is_mc_version_compatible(&v.game_versions, mc_version)
                && is_loader_compatible(&v.loaders, loader)
        })
        .cloned()
        .collect();

    if loader == Some(ModLoaderKind::Quilt) {
        filtered.sort_by_key(|v| {
            let has_native_quilt = v
                .loaders
                .iter()
                .any(|l| l.trim().eq_ignore_ascii_case("quilt"));
            usize::from(!has_native_quilt)
        });
    }

    filtered
}

/// Unified content provider trait across Modrinth API v2 and CurseForge Core API v1.
#[allow(async_fn_in_trait)]
pub trait ContentProvider: Send + Sync {
    /// Returns the catalog source identifier (`Modrinth` or `CurseForge`).
    fn source(&self) -> CatalogSource;

    /// Searches the catalog for mods, resourcepacks, shaders, or modpacks.
    async fn search(&self, query: &SearchQuery) -> MqResult<Vec<CatalogHit>>;

    /// Fetches summary metadata for a single project by ID or slug.
    async fn get_project(&self, project_id_or_slug: &str) -> MqResult<CatalogHit>;

    /// Fetches versions for `project_id_or_slug` compatible with `mc_version` and `loader`.
    async fn get_versions(
        &self,
        project_id_or_slug: &str,
        mc_version: Option<&str>,
        loader: Option<ModLoaderKind>,
    ) -> MqResult<Vec<ProjectVersion>>;

    /// Fetches a specific version by project ID and version/file ID.
    async fn get_version(&self, project_id: &str, version_id: &str) -> MqResult<ProjectVersion>;

    /// Identifies locally installed files by SHA-1/SHA-512 hashes or CurseForge Murmur2 fingerprints.
    async fn identify_by_hashes(
        &self,
        sha1_or_sha512: &[String],
        fingerprints: &[u32],
    ) -> MqResult<HashMap<String, ProjectVersion>>;
}

impl ContentProvider for ModrinthClient {
    fn source(&self) -> CatalogSource {
        CatalogSource::Modrinth
    }

    async fn search(&self, query: &SearchQuery) -> MqResult<Vec<CatalogHit>> {
        if query.categories.is_empty() && query.sort == SearchSort::Relevance {
            ModrinthClient::search(self, query).await
        } else {
            let index = match query.sort {
                SearchSort::Relevance => ModrinthSearchIndex::Relevance,
                SearchSort::Downloads => ModrinthSearchIndex::Downloads,
                SearchSort::Follows => ModrinthSearchIndex::Follows,
                SearchSort::Newest => ModrinthSearchIndex::Newest,
                SearchSort::Updated => ModrinthSearchIndex::Updated,
            };
            let params = ModrinthSearchParams::from_query(query.clone())
                .with_categories(query.categories.clone())
                .with_index(index);
            let page = self.search_with_options(&params).await?;
            Ok(page.hits)
        }
    }

    async fn get_project(&self, project_id_or_slug: &str) -> MqResult<CatalogHit> {
        ModrinthClient::get_project(self, project_id_or_slug).await
    }

    async fn get_versions(
        &self,
        project_id_or_slug: &str,
        mc_version: Option<&str>,
        loader: Option<ModLoaderKind>,
    ) -> MqResult<Vec<ProjectVersion>> {
        self.get_project_versions(project_id_or_slug, mc_version, loader)
            .await
    }

    async fn get_version(&self, _project_id: &str, version_id: &str) -> MqResult<ProjectVersion> {
        ModrinthClient::get_version(self, version_id).await
    }

    async fn identify_by_hashes(
        &self,
        sha1_or_sha512: &[String],
        _fingerprints: &[u32],
    ) -> MqResult<HashMap<String, ProjectVersion>> {
        let map = ModrinthClient::identify_by_hashes(self, sha1_or_sha512).await?;
        Ok(map.into_iter().collect())
    }
}

impl ContentProvider for CurseForgeClient {
    fn source(&self) -> CatalogSource {
        CatalogSource::CurseForge
    }

    async fn search(&self, query: &SearchQuery) -> MqResult<Vec<CatalogHit>> {
        let mut opts = CurseForgeSearchOptions::new();
        for cat in &query.categories {
            if let Ok(cat_id) = cat.trim().parse::<u32>() {
                opts = opts.with_category_id(cat_id);
            }
        }
        match query.sort {
            SearchSort::Relevance => {}
            SearchSort::Downloads => {
                opts = opts
                    .with_sort_field(CurseForgeSortField::TotalDownloads)
                    .with_sort_order(CurseForgeSortOrder::Desc);
            }
            SearchSort::Follows => {
                opts = opts
                    .with_sort_field(CurseForgeSortField::Popularity)
                    .with_sort_order(CurseForgeSortOrder::Desc);
            }
            SearchSort::Newest | SearchSort::Updated => {
                opts = opts
                    .with_sort_field(CurseForgeSortField::LastUpdated)
                    .with_sort_order(CurseForgeSortOrder::Desc);
            }
        }
        self.search_with_options(query, &opts).await
    }

    async fn get_project(&self, project_id_or_slug: &str) -> MqResult<CatalogHit> {
        let trimmed = project_id_or_slug.trim();
        if let Ok(mod_id) = trimmed.parse::<u64>() {
            return self.get_mod(mod_id).await;
        }
        let q = SearchQuery::new(trimmed);
        let opts = CurseForgeSearchOptions::new().with_slug(trimmed);
        let hits = self.search_with_options(&q, &opts).await?;
        hits.into_iter()
            .find(|h| h.slug.eq_ignore_ascii_case(trimmed))
            .ok_or_else(|| {
                MqError::DependencyResolution(format!("CurseForge project '{trimmed}' not found"))
            })
    }

    async fn get_versions(
        &self,
        project_id_or_slug: &str,
        mc_version: Option<&str>,
        loader: Option<ModLoaderKind>,
    ) -> MqResult<Vec<ProjectVersion>> {
        <Self as ModCatalogProvider>::fetch_compatible_versions(
            self,
            project_id_or_slug,
            mc_version,
            loader,
        )
        .await
    }

    async fn get_version(&self, project_id: &str, version_id: &str) -> MqResult<ProjectVersion> {
        let file_id = version_id.trim().parse::<u64>().map_err(|e| {
            MqError::Metadata(format!("invalid CurseForge file_id '{version_id}': {e}"))
        })?;
        if let Ok(mod_id) = project_id.trim().parse::<u64>() {
            return self.get_mod_file(mod_id, file_id).await;
        }
        let mut batch = self.get_files_batch(&[file_id]).await?;
        batch.pop().ok_or_else(|| {
            MqError::DependencyResolution(format!("CurseForge file '{file_id}' not found"))
        })
    }

    async fn identify_by_hashes(
        &self,
        _sha1_or_sha512: &[String],
        fingerprints: &[u32],
    ) -> MqResult<HashMap<String, ProjectVersion>> {
        let by_fp = self.identify_by_fingerprints(fingerprints).await?;
        let mut out = HashMap::with_capacity(by_fp.len() * 2);
        for (fp, ver) in by_fp {
            if let Some(sha1) = ver.primary_file().and_then(|f| f.sha1) {
                out.insert(sha1.to_hex(), ver.clone());
            }
            out.insert(fp.to_string(), ver);
        }
        Ok(out)
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

        let mut files = self.get_mod_files(mod_id, mc_version, loader).await?;
        if files.is_empty() && loader == Some(ModLoaderKind::Quilt) {
            files = self
                .get_mod_files(mod_id, mc_version, Some(ModLoaderKind::Fabric))
                .await?;
        }
        Ok(files)
    }

    async fn fetch_version_by_id<'a>(
        &'a self,
        version_id: &'a str,
    ) -> MqResult<Option<ProjectVersion>> {
        let Ok(file_id) = version_id.trim().parse::<u64>() else {
            return Ok(None);
        };
        let mut files = self.get_files_batch(&[file_id]).await?;
        Ok(files.pop())
    }
}

/// Optional dependency discovered during resolution (returned so the UI can offer checkboxes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionalDependency {
    pub required_by_project_id: String,
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    pub file_name: Option<String>,
}

/// Options controlling dependency graph resolution.
#[derive(Debug, Default, Clone)]
pub struct ResolveOptions {
    /// Target content kind (`Mod`, `ResourcePack`, `Shader`, `Modpack`).
    pub kind: ProjectKind,
    /// Optional dependencies explicitly checked by the user to be installed alongside required ones.
    pub selected_optional_ids: BTreeSet<String>,
    /// Project IDs or slugs already installed in the instance (used for incompatibility checks).
    pub installed_project_ids: BTreeSet<String>,
}

impl ResolveOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_kind(mut self, kind: ProjectKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn with_selected_optionals<I, S>(mut self, ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for id in ids {
            let trimmed = id.into().trim().to_owned();
            if !trimmed.is_empty() {
                self.selected_optional_ids.insert(trimmed);
            }
        }
        self
    }

    pub fn with_installed_projects<I, S>(mut self, ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for id in ids {
            let trimmed = id.into().trim().to_owned();
            if !trimmed.is_empty() {
                self.installed_project_ids.insert(trimmed);
            }
        }
        self
    }
}

/// Complete dependency resolution plan supporting `Required`, `Optional`, `Incompatible`, and `Embedded`.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyResolutionPlan {
    /// Deterministic topological download plan (required/selected-optional dependencies first, root last).
    pub downloads: Vec<ResolvedModDownload>,
    /// Optional dependencies discovered in the graph (not yet selected) for UI checkbox selection.
    pub optional_dependencies: Vec<OptionalDependency>,
    /// Embedded dependencies skipped because they are already bundled inside the parent archive.
    pub embedded_dependencies: Vec<ModDependencyRef>,
    /// Files in `downloads` whose direct download URL is empty (`downloadUrl == null` on CurseForge).
    pub blocked_downloads: Vec<BlockedDownload>,
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

/// Resolves a root project (`project_id_or_slug`) and its dependency graph with [`ResolveOptions`]:
/// - `DependencyKind::Required`: automatically resolved and queued.
/// - `DependencyKind::Optional`: returned in `optional_dependencies` (or resolved if listed in `options.selected_optional_ids`).
/// - `DependencyKind::Incompatible`: blocks installation with a clear Russian error message.
/// - `DependencyKind::Embedded`: skipped and recorded in `embedded_dependencies`.
pub async fn resolve_dependency_plan<P, V, L>(
    provider: &P,
    project_id_or_slug: &str,
    mc_version: V,
    loader: L,
    options: &ResolveOptions,
) -> MqResult<DependencyResolutionPlan>
where
    P: ModCatalogProvider + ?Sized,
    V: IntoMcVersionFilter,
    L: IntoLoaderFilter,
{
    let root_id = project_id_or_slug.trim();
    if root_id.is_empty() {
        return Err(MqError::DependencyResolution(
            "Не указан идентификатор или slug проекта для установки".to_owned(),
        ));
    }

    let mc_ver_filter = mc_version.into_mc_version_filter();
    let loader_filter = loader.into_loader_filter();

    let mut visited_projects: BTreeSet<String> = BTreeSet::new();
    let mut queue: VecDeque<PendingMod> = VecDeque::new();
    let mut nodes: Vec<ResolvedNode> = Vec::new();
    let mut optional_dependencies: Vec<OptionalDependency> = Vec::new();
    let mut embedded_dependencies: Vec<ModDependencyRef> = Vec::new();

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
                    "В описании зависимости отсутствует project_id и version_id".to_owned(),
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
            let fetched = provider
                .fetch_compatible_versions(proj_key, mc_ver_filter.as_deref(), loader_filter)
                .await?;
            let compatible =
                filter_compatible_versions(&fetched, mc_ver_filter.as_deref(), loader_filter);

            if compatible.is_empty() {
                return Err(MqError::DependencyResolution(format!(
                    "Не найдена совместимая версия для проекта '{proj_key}' (Minecraft: {:?}, загрузчик: {:?})",
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
            let ver = provider.fetch_version_by_id(vid).await?.ok_or_else(|| {
                MqError::DependencyResolution(format!(
                    "Не удалось найти закреплённую версию зависимости '{vid}'"
                ))
            })?;
            if !is_mc_version_compatible(&ver.game_versions, mc_ver_filter.as_deref())
                || !is_loader_compatible(&ver.loaders, loader_filter)
            {
                return Err(MqError::DependencyResolution(format!(
                    "Версия зависимости '{vid}' несовместима с Minecraft {:?} / загрузчиком {:?}",
                    mc_ver_filter, loader_filter
                )));
            }
            ver
        } else {
            return Err(MqError::DependencyResolution(
                "Зависимость не содержит ни project_id, ни version_id".to_owned(),
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
                "У версии '{}' проекта '{}' нет файлов для скачивания",
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
            let dep_key = dep
                .project_id
                .as_deref()
                .or(dep.version_id.as_deref())
                .or(dep.file_name.as_deref())
                .map(str::trim)
                .filter(|s| !s.is_empty());

            match dep.kind {
                DependencyKind::Incompatible => {
                    let target_name = dep_key.unwrap_or("неизвестный мод");
                    return Err(MqError::DependencyResolution(format!(
                        "Установка заблокирована: проект '{}' (версия '{}') несовместим с '{}'",
                        canonical_id, selected_version.version_number, target_name
                    )));
                }
                DependencyKind::Embedded => {
                    debug!(
                        parent = %canonical_id,
                        ?dep_key,
                        "пропуск встроенной (embedded) зависимости"
                    );
                    if !embedded_dependencies.contains(dep) {
                        embedded_dependencies.push(dep.clone());
                    }
                }
                DependencyKind::Optional => {
                    let is_selected = dep_key.is_some_and(|k| {
                        options.selected_optional_ids.iter().any(|sel| {
                            sel.eq_ignore_ascii_case(k)
                                || dep
                                    .project_id
                                    .as_deref()
                                    .is_some_and(|p| sel.eq_ignore_ascii_case(p))
                                || dep
                                    .version_id
                                    .as_deref()
                                    .is_some_and(|v| sel.eq_ignore_ascii_case(v))
                        })
                    });

                    if is_selected {
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
                    } else {
                        let opt_entry = OptionalDependency {
                            required_by_project_id: canonical_id.clone(),
                            project_id: dep.project_id.clone(),
                            version_id: dep.version_id.clone(),
                            file_name: dep.file_name.clone(),
                        };
                        if !optional_dependencies.contains(&opt_entry) {
                            optional_dependencies.push(opt_entry);
                        }
                    }
                }
                DependencyKind::Required => {
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
    let mut downloads = Vec::with_capacity(nodes.len());

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
        topo_visit(
            start_idx,
            &nodes,
            &alias_to_idx,
            &mut states,
            &mut downloads,
        );
    }

    let subfolder = options.kind.target_subfolder();
    let mut blocked_downloads = Vec::new();
    for d in &downloads {
        if d.is_download_blocked() {
            let file_id_u64 = d.version_id.parse::<u64>().unwrap_or(0);
            let webpage_url = build_curseforge_manual_download_url(
                None,
                Some(&d.project_id),
                options.kind,
                file_id_u64,
            );
            let target_rel_path = if subfolder.is_empty() {
                d.filename.clone()
            } else {
                format!("{subfolder}/{}", d.filename)
            };
            blocked_downloads.push(BlockedDownload {
                project_id: d.project_id.clone(),
                file_id: d.version_id.clone(),
                project_title: d.project_id.clone(),
                filename: d.filename.clone(),
                webpage_url,
                expected_size: d.size,
                expected_sha1: d.sha1,
                expected_fingerprint: None,
                target_rel_path,
            });
        }
    }

    Ok(DependencyResolutionPlan {
        downloads,
        optional_dependencies,
        embedded_dependencies,
        blocked_downloads,
    })
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
    let plan = resolve_dependency_plan(
        provider,
        project_id_or_slug,
        mc_version,
        loader,
        &ResolveOptions::default(),
    )
    .await?;
    Ok(plan.downloads)
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

        let plan = resolve_dependency_plan(
            &cf,
            "394468",
            "1.21.1",
            ModLoaderKind::Fabric,
            &ResolveOptions::default(),
        )
        .await
        .expect("resolve cf sodium -> fabric-api");

        assert_eq!(plan.downloads.len(), 2);
        assert_eq!(plan.downloads[0].project_id, "306612");
        // downloadUrl: null MUST NOT synthesize edge.forgecdn.net; url stays empty ("") and produces BlockedDownload.
        assert_eq!(plan.downloads[0].url, "");
        assert!(plan.downloads[0].is_download_blocked());
        assert!(plan.downloads[0].is_dependency);

        assert_eq!(plan.downloads[1].project_id, "394468");
        assert_eq!(plan.downloads[1].url, "");
        assert!(plan.downloads[1].is_download_blocked());
        assert!(!plan.downloads[1].is_dependency);
        assert_eq!(plan.blocked_downloads.len(), 2);
    }

    #[tokio::test]
    async fn resolver_supports_optional_embedded_incompatible_quilt_fabric_and_mc_26x() {
        let (base, _requests) = spawn_mock_resolver_server(move |path| {
            if path.starts_with("/modrinth/v2/project/mod-26x/version") {
                let body = json!([{
                    "id": "ver-mod-26-1-1",
                    "project_id": "MOD26X",
                    "name": "Mod 26.1.1",
                    "version_number": "1.0.0+mc26.1.1",
                    "game_versions": ["26.1.x"],
                    "loaders": ["fabric"],
                    "dependencies": [
                        { "project_id": "OPT_MOD", "dependency_type": "optional" },
                        { "project_id": "EMBED_LIB", "dependency_type": "embedded" }
                    ],
                    "files": [{
                        "hashes": {},
                        "url": "https://cdn.modrinth.com/mod-26x.jar",
                        "filename": "mod-26x.jar",
                        "primary": true,
                        "size": 1000
                    }]
                }]);
                (200, body.to_string())
            } else if path.starts_with("/modrinth/v2/project/OPT_MOD/version") {
                let body = json!([{
                    "id": "ver-opt-26-1-1",
                    "project_id": "OPT_MOD",
                    "name": "Optional Mod",
                    "version_number": "1.0.0",
                    "game_versions": ["26.1.1"],
                    "loaders": ["fabric"],
                    "dependencies": [],
                    "files": [{
                        "hashes": {},
                        "url": "https://cdn.modrinth.com/opt-mod.jar",
                        "filename": "opt-mod.jar",
                        "primary": true,
                        "size": 500
                    }]
                }]);
                (200, body.to_string())
            } else if path.starts_with("/modrinth/v2/project/bad-mod/version") {
                let body = json!([{
                    "id": "ver-bad",
                    "project_id": "BAD_MOD",
                    "name": "Bad Mod",
                    "version_number": "1.0.0",
                    "game_versions": ["26.1"],
                    "loaders": ["quilt"],
                    "dependencies": [
                        { "project_id": "optifabric", "dependency_type": "incompatible" }
                    ],
                    "files": [{
                        "hashes": {},
                        "url": "https://cdn.modrinth.com/bad-mod.jar",
                        "filename": "bad-mod.jar",
                        "primary": true,
                        "size": 1000
                    }]
                }]);
                (200, body.to_string())
            } else {
                (404, json!({"error": "not_found"}).to_string())
            }
        })
        .await;

        let client =
            ModrinthClient::with_base_url(format!("{base}/modrinth/v2")).expect("modrinth client");

        // 1. Quilt instance resolves Fabric mod for MC 26.1.1 (matched against "26.1.x"),
        // skips Embedded, and returns Optional for UI checkbox selection.
        let plan = resolve_dependency_plan(
            &client,
            "mod-26x",
            "26.1.1",
            ModLoaderKind::Quilt,
            &ResolveOptions::default(),
        )
        .await
        .expect("resolve 26.1.1 on Quilt");
        assert_eq!(plan.downloads.len(), 1);
        assert_eq!(plan.downloads[0].project_id, "MOD26X");
        assert_eq!(plan.optional_dependencies.len(), 1);
        assert_eq!(
            plan.optional_dependencies[0].project_id.as_deref(),
            Some("OPT_MOD")
        );
        assert_eq!(plan.embedded_dependencies.len(), 1);
        assert_eq!(
            plan.embedded_dependencies[0].project_id.as_deref(),
            Some("EMBED_LIB")
        );

        // 2. When the user checks OPT_MOD, it is resolved and installed first.
        let opts_with_optional = ResolveOptions::new().with_selected_optionals(["OPT_MOD"]);
        let plan_with_opt = resolve_dependency_plan(
            &client,
            "mod-26x",
            "26.1.1",
            ModLoaderKind::Quilt,
            &opts_with_optional,
        )
        .await
        .expect("resolve with selected optional");
        assert_eq!(plan_with_opt.downloads.len(), 2);
        assert_eq!(plan_with_opt.downloads[0].project_id, "OPT_MOD");
        assert!(plan_with_opt.downloads[0].is_dependency);
        assert_eq!(plan_with_opt.downloads[1].project_id, "MOD26X");

        // 3. Incompatible dependency blocks installation with a clear Russian error.
        let err = resolve_mod_with_dependencies(&client, "bad-mod", "26.1", ModLoaderKind::Quilt)
            .await
            .expect_err("incompatible must fail");
        let msg = err.to_string();
        assert!(
            msg.contains("несовместим") && msg.contains("optifabric"),
            "unexpected error message: {msg}"
        );
    }
}
