#![deny(clippy::await_holding_lock)]
//! Background operations behind the project page, install dialog, downloads queue and
//! the manual-download dialog. Never called from the UI thread.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use mq_core::{AppConfig, EventBus, InstanceId, ModLoaderKind, MqPaths};
use mq_mods::{
    resolve_dependency_plan, BlockedDownload, BlockedDownloadsWatcher, CatalogSource,
    ContentProvider, CurseForgeClient, DependencyResolutionPlan, GalleryItem, LockSource,
    ModrinthClient, ModrinthProjectDetails, ProjectDetails, ProjectKind, ProjectVersion,
    ResolveOptions, ResolvedModDownload,
};
use mq_net::{DownloadSpec, NetEngine};
use mq_sched::{CancelEpoch, ComputePool};

use crate::catalog_flow::{ModpackPreview, ProjectPage};

/// Shared handles a worker task needs.
#[derive(Clone)]
pub struct OpsCtx {
    pub config: AppConfig,
    pub paths: MqPaths,
    pub net: Arc<NetEngine>,
    pub epoch: Arc<CancelEpoch>,
    pub pool: Arc<ComputePool>,
    pub bus: EventBus,
}

const PACK_MAX_BYTES: u64 = 512 * 1024 * 1024;

fn err_text(config: &AppConfig, e: &dyn std::fmt::Display) -> String {
    config.sanitize_cf_api_key(&e.to_string())
}

/// URL path segment used by modrinth.com for a project kind.
pub const fn modrinth_type_path(kind: ProjectKind) -> &'static str {
    match kind {
        ProjectKind::Mod => "mod",
        ProjectKind::ResourcePack => "resourcepack",
        ProjectKind::Shader => "shader",
        ProjectKind::Modpack => "modpack",
    }
}

/// Maps Modrinth project details onto the unified [`ProjectDetails`] contract.
pub fn modrinth_to_project(d: ModrinthProjectDetails) -> ProjectDetails {
    let mut categories = d.categories.clone();
    categories.extend(d.additional_categories.iter().cloned());
    ProjectDetails {
        source: CatalogSource::Modrinth,
        project_id: d.id.clone(),
        slug: d.slug.clone(),
        title: d.title,
        description: d.description,
        body: Some(d.body),
        author: d.author,
        downloads: d.downloads,
        followers: d.followers,
        icon_url: d.icon_url,
        gallery: d
            .gallery
            .into_iter()
            .map(|g| GalleryItem {
                url: g.url,
                raw_url: g.raw_url,
                title: g.title,
                description: g.description,
                featured: g.featured,
            })
            .collect(),
        project_kind: d.project_type,
        loaders: d.loaders,
        game_versions: d.game_versions,
        categories,
        latest_version: d.hit.latest_version.clone(),
        license: d
            .license
            .map(|l| if l.name.is_empty() { l.id } else { l.name })
            .filter(|s| !s.is_empty()),
        client_side: Some(d.client_side),
        server_side: Some(d.server_side),
        source_url: d.source_url,
        website_url: Some(format!(
            "https://modrinth.com/{}/{}",
            modrinth_type_path(d.project_type),
            d.slug
        )),
    }
}

/// Loads project details and all versions for the project page.
pub async fn load_project(
    config: &AppConfig,
    source: CatalogSource,
    project_id: &str,
    kind: ProjectKind,
) -> Result<ProjectPage, String> {
    match source {
        CatalogSource::Modrinth => {
            let client = ModrinthClient::from_config(config).map_err(|e| err_text(config, &e))?;
            let details = client
                .get_project_details(project_id)
                .await
                .map_err(|e| err_text(config, &e))?;
            let versions = ContentProvider::get_versions(&client, project_id, None, None)
                .await
                .unwrap_or_default();
            Ok(ProjectPage {
                details: modrinth_to_project(details),
                versions,
            })
        }
        CatalogSource::CurseForge => {
            let client = CurseForgeClient::from_config(config).map_err(|e| err_text(config, &e))?;
            let id: u64 = project_id
                .parse()
                .map_err(|_| format!("Некорректный идентификатор CurseForge: {project_id}"))?;
            let cf = client
                .get_mod_details(id)
                .await
                .map_err(|e| err_text(config, &e))?;
            let body = client.get_mod_description(id).await.ok();
            let versions = client
                .get_mod_files(id, None, None)
                .await
                .map_err(|e| err_text(config, &e))?;
            let mut details = cf.hit.clone().into_project_details();
            details.project_kind = kind;
            details.body = body;
            details.website_url = cf.website_url.clone();
            details.source_url = cf.source_url.clone();
            details.categories = cf.categories.iter().map(|c| c.name.clone()).collect();
            details.gallery = cf
                .screenshots
                .iter()
                .filter(|s| !s.url.is_empty())
                .map(|s| GalleryItem {
                    url: s.thumbnail_url.clone().unwrap_or_else(|| s.url.clone()),
                    raw_url: Some(s.url.clone()),
                    title: (!s.title.is_empty()).then(|| s.title.clone()),
                    description: (!s.description.is_empty()).then(|| s.description.clone()),
                    featured: false,
                })
                .collect();
            Ok(ProjectPage { details, versions })
        }
    }
}

/// Request for the dependency plan shown in the install dialog.
#[derive(Debug, Clone)]
pub struct PlanRequest {
    pub source: CatalogSource,
    pub project_id: String,
    /// Additional `(source, project_id)` entries for batch installation planning.
    pub extra_projects: Vec<(CatalogSource, String)>,
    pub mc_version: String,
    pub loader: Option<ModLoaderKind>,
    pub kind: ProjectKind,
    pub selected_optionals: BTreeSet<String>,
    /// Instance directory; its lockfile provides the already-installed project ids.
    pub instance_dir: Option<PathBuf>,
}

/// Dependency plan plus display titles and source map for every project id mentioned in it.
#[derive(Debug, Clone)]
pub struct PlanOutcome {
    pub plan: DependencyResolutionPlan,
    pub titles: HashMap<String, String>,
    pub sources: HashMap<String, CatalogSource>,
}

/// Project ids/slugs recorded in the instance lockfile (blocking file read).
pub fn installed_project_ids(instance_dir: &Path, kind: ProjectKind) -> BTreeSet<String> {
    let lock =
        mq_mods::ModLockFile::load_or_default(&mq_mods::content_target_dir(instance_dir, kind));
    lock.entries
        .into_iter()
        .flat_map(|(slug, e)| [slug, e.project_id])
        .filter(|s| !s.is_empty())
        .collect()
}

/// Merges and deduplicates dependency plans across one or more explicitly selected projects.
///
/// Any requested root project keeps `is_dependency = false` even if another queued item depends on it.
pub fn merge_dependency_plans(
    plans: Vec<(CatalogSource, String, DependencyResolutionPlan)>,
) -> (DependencyResolutionPlan, HashMap<String, CatalogSource>) {
    use std::collections::HashSet;

    let mut explicit_roots: HashSet<String> = HashSet::with_capacity(plans.len() * 2);
    for (_, root_id, sub) in &plans {
        explicit_roots.insert(root_id.clone());
        for dl in &sub.downloads {
            if !dl.is_dependency {
                explicit_roots.insert(dl.project_id.clone());
            }
        }
    }

    let mut merged = DependencyResolutionPlan::default();
    let mut sources = HashMap::new();
    let mut seen_dl: HashSet<String> = HashSet::new();
    let mut seen_opt: HashSet<(Option<String>, Option<String>)> = HashSet::new();
    let mut seen_emb: HashSet<(Option<String>, Option<String>)> = HashSet::new();
    let mut seen_blocked: HashSet<(String, String)> = HashSet::new();

    for (source, _, sub) in plans {
        for mut dl in sub.downloads {
            sources.entry(dl.project_id.clone()).or_insert(source);
            if seen_dl.insert(dl.project_id.clone()) {
                dl.is_dependency = !explicit_roots.contains(&dl.project_id);
                merged.downloads.push(dl);
            } else if explicit_roots.contains(&dl.project_id) {
                if let Some(existing) = merged
                    .downloads
                    .iter_mut()
                    .find(|item| item.project_id == dl.project_id)
                {
                    existing.is_dependency = false;
                }
            }
        }
        for emb in sub.embedded_dependencies {
            let key = (emb.project_id.clone(), emb.file_name.clone());
            if seen_emb.insert(key) {
                merged.embedded_dependencies.push(emb);
            }
        }
        for b in sub.blocked_downloads {
            sources.entry(b.project_id.clone()).or_insert(source);
            let key = (b.project_id.clone(), b.file_id.clone());
            if seen_blocked.insert(key) {
                merged.blocked_downloads.push(b);
            }
        }
        for opt in sub.optional_dependencies {
            if let Some(pid) = &opt.project_id {
                if seen_dl.contains(pid) {
                    continue;
                }
            }
            let key = (opt.project_id.clone(), opt.file_name.clone());
            if seen_opt.insert(key) {
                merged.optional_dependencies.push(opt);
            }
        }
    }

    merged.optional_dependencies.retain(|opt| {
        opt.project_id
            .as_ref()
            .is_none_or(|pid| !seen_dl.contains(pid))
    });

    (merged, sources)
}

async fn resolve_single_plan<P>(
    provider: &P,
    project_id: &str,
    req: &PlanRequest,
    opts: &ResolveOptions,
) -> Result<DependencyResolutionPlan, String>
where
    P: mq_mods::ModCatalogProvider + ContentProvider,
{
    resolve_dependency_plan(
        provider,
        project_id,
        req.mc_version.as_str(),
        req.loader,
        opts,
    )
    .await
    .map_err(|e| e.to_string())
}

/// Resolves required + optional + incompatible dependencies for single or batch installs.
pub async fn plan_content(config: &AppConfig, req: &PlanRequest) -> Result<PlanOutcome, String> {
    let installed = match req.instance_dir.clone() {
        Some(dir) => {
            let kind = req.kind;
            tokio::task::spawn_blocking(move || installed_project_ids(&dir, kind))
                .await
                .unwrap_or_default()
        }
        None => BTreeSet::new(),
    };
    let opts = ResolveOptions::new()
        .with_kind(req.kind)
        .with_selected_optionals(req.selected_optionals.iter().cloned());
    let opts = ResolveOptions {
        installed_project_ids: installed,
        ..opts
    };

    let mut targets: Vec<(CatalogSource, String)> =
        Vec::with_capacity(1 + req.extra_projects.len());
    targets.push((req.source, req.project_id.clone()));
    for (source, project_id) in &req.extra_projects {
        if !targets
            .iter()
            .any(|(s, p)| *s == *source && p == project_id)
        {
            targets.push((*source, project_id.clone()));
        }
    }

    let mut modrinth_client: Option<ModrinthClient> = None;
    let mut curseforge_client: Option<CurseForgeClient> = None;
    let mut plans = Vec::with_capacity(targets.len());

    for (source, project_id) in targets {
        let sub = match source {
            CatalogSource::Modrinth => {
                if modrinth_client.is_none() {
                    modrinth_client = Some(
                        ModrinthClient::from_config(config).map_err(|e| err_text(config, &e))?,
                    );
                }
                let client = modrinth_client.as_ref().expect("initialized above");
                resolve_single_plan(client, &project_id, req, &opts)
                    .await
                    .map_err(|e| err_text(config, &e))?
            }
            CatalogSource::CurseForge => {
                if curseforge_client.is_none() {
                    curseforge_client = Some(
                        CurseForgeClient::from_config(config).map_err(|e| err_text(config, &e))?,
                    );
                }
                let client = curseforge_client.as_ref().expect("initialized above");
                resolve_single_plan(client, &project_id, req, &opts)
                    .await
                    .map_err(|e| err_text(config, &e))?
            }
        };
        plans.push((source, project_id, sub));
    }

    let (plan, sources) = merge_dependency_plans(plans);

    let mut ids: Vec<String> = plan
        .downloads
        .iter()
        .map(|d| d.project_id.clone())
        .collect();
    ids.extend(
        plan.optional_dependencies
            .iter()
            .filter_map(|o| o.project_id.clone()),
    );
    ids.sort();
    ids.dedup();

    let mut titles = HashMap::new();
    for id in ids.into_iter().take(48) {
        let src = sources.get(&id).copied().unwrap_or(req.source);
        let hit = match src {
            CatalogSource::Modrinth => match modrinth_client.as_ref() {
                Some(c) => ContentProvider::get_project(c, &id).await.ok(),
                None => None,
            },
            CatalogSource::CurseForge => match curseforge_client.as_ref() {
                Some(c) => ContentProvider::get_project(c, &id).await.ok(),
                None => None,
            },
        };
        if let Some(hit) = hit {
            titles.insert(id, hit.title);
        }
    }

    Ok(PlanOutcome {
        plan,
        titles,
        sources,
    })
}

/// Converts the catalog source into the lockfile source tag.
pub const fn lock_source(source: CatalogSource) -> LockSource {
    match source {
        CatalogSource::Modrinth => LockSource::Modrinth,
        CatalogSource::CurseForge => LockSource::CurseForge,
    }
}

/// Installs resolved downloads one file at a time so progress and cancellation are per file.
/// Blocked (URL-less) downloads must already be filtered out by the caller.
pub async fn run_content_job(
    ctx: &OpsCtx,
    cancel: &AtomicBool,
    instance_id: &InstanceId,
    kind: ProjectKind,
    source: CatalogSource,
    downloads: &[ResolvedModDownload],
    progress: impl Fn(usize, usize),
) -> Result<(), String> {
    let instance_dir = ctx.paths.instance_dir(instance_id.as_str());
    let total = downloads.len();
    progress(0, total);
    for (i, d) in downloads.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            return Err("Отменено".to_string());
        }
        let guard = ctx.epoch.snapshot();
        mq_mods::install_resolved_content(
            std::slice::from_ref(d),
            &instance_dir,
            kind,
            lock_source(source),
            &ctx.paths,
            &ctx.net,
            &guard,
        )
        .await
        .map_err(|e| err_text(&ctx.config, &e))?;
        progress(i + 1, total);
    }
    Ok(())
}

/// Looks for newer versions of everything recorded in the instance lock files.
/// Manually added files (no source) are skipped; blocked downloads are not offered.
pub async fn check_updates(
    config: &AppConfig,
    instance_dir: &Path,
    mc_version: &str,
    loader: mq_core::ModLoaderKind,
) -> Result<Vec<crate::catalog_flow::UpdateCandidate>, String> {
    let mut found = Vec::new();
    for kind in [
        ProjectKind::Mod,
        ProjectKind::ResourcePack,
        ProjectKind::Shader,
    ] {
        let dir = instance_dir.to_path_buf();
        let lock = tokio::task::spawn_blocking(move || {
            mq_mods::ModLockFile::load_or_default(&mq_mods::content_target_dir(&dir, kind))
        })
        .await
        .map_err(|e| e.to_string())?;
        for entry in lock.entries.values().take(200) {
            let source = match entry.source {
                LockSource::Modrinth => CatalogSource::Modrinth,
                LockSource::CurseForge => CatalogSource::CurseForge,
                LockSource::Manual => continue,
            };
            if entry.project_id.is_empty() || entry.kind != kind {
                continue;
            }
            let req = PlanRequest {
                source,
                project_id: entry.project_id.clone(),
                extra_projects: Vec::new(),
                mc_version: mc_version.to_string(),
                loader: crate::catalog_actions::loader_filter(kind, loader),
                kind,
                selected_optionals: BTreeSet::new(),
                instance_dir: None,
            };
            // A source that fails (no key, offline) just yields no candidates for its entries.
            let Ok(outcome) = plan_content(config, &req).await else {
                continue;
            };
            let root = outcome
                .plan
                .downloads
                .iter()
                .rev()
                .find(|d| d.project_id == entry.project_id)
                .or_else(|| outcome.plan.downloads.last());
            if let Some(new) = root {
                if !new.is_download_blocked()
                    && new.version_id != entry.version_id
                    && new.filename != entry.filename
                {
                    found.push(crate::catalog_flow::UpdateCandidate {
                        source,
                        kind,
                        project_id: entry.project_id.clone(),
                        old_filename: entry.filename.clone(),
                        old_version_id: entry.version_id.clone(),
                        new: new.clone(),
                    });
                }
            }
        }
    }
    Ok(found)
}

/// Replaces one installed file; the previous file is restored if the update fails.
pub async fn apply_update(
    ctx: &OpsCtx,
    instance_id: &InstanceId,
    cand: &crate::catalog_flow::UpdateCandidate,
) -> Result<(), String> {
    let dir = mq_mods::content_target_dir(&ctx.paths.instance_dir(instance_id.as_str()), cand.kind);
    let guard = ctx.epoch.snapshot();
    mq_mods::update_installed_content(
        &dir,
        &cand.old_filename,
        &cand.new,
        cand.kind,
        lock_source(cand.source),
        &ctx.paths,
        &ctx.net,
        &guard,
    )
    .await
    .map(|_| ())
    .map_err(|e| err_text(&ctx.config, &e))
}

fn is_mrpack(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".mrpack")
}

/// Keeps only the final path component, rejecting anything that could escape the staging dir.
pub fn safe_pack_file_name(name: &str) -> Option<String> {
    let n = Path::new(name).file_name()?.to_str()?.to_string();
    (!n.is_empty() && n != ".." && n != ".").then_some(n)
}

/// Builds a [`ModpackPreview`] from a local archive (no network).
pub fn preview_archive(archive: &Path) -> Result<ModpackPreview, String> {
    let name = archive
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if is_mrpack(name) {
        let idx = mq_mods::parse_mrpack_index_from_archive(archive).map_err(|e| e.to_string())?;
        let files = idx.client_files();
        let (loader, loader_version) = idx.loader_and_version();
        Ok(ModpackPreview {
            mc_version: idx.minecraft_version().unwrap_or_default().to_string(),
            loader,
            loader_version,
            file_count: files.len(),
            total_size: files.iter().map(|f| f.file_size).sum(),
            blocked: Vec::new(),
            archive_path: archive.to_path_buf(),
        })
    } else {
        let m = mq_mods::parse_curseforge_manifest_from_zip(archive).map_err(|e| e.to_string())?;
        let (loader, loader_version) = m.loader_and_version();
        Ok(ModpackPreview {
            mc_version: m.minecraft.version.clone(),
            loader,
            loader_version,
            file_count: m.files.iter().filter(|f| f.required).count(),
            total_size: 0,
            blocked: Vec::new(),
            archive_path: archive.to_path_buf(),
        })
    }
}

/// Downloads the modpack archive of `version` into the staging dir and parses its manifest.
pub async fn prepare_modpack(
    ctx: &OpsCtx,
    version: &ProjectVersion,
) -> Result<ModpackPreview, String> {
    let file = version
        .primary_file()
        .ok_or_else(|| "У версии нет файлов".to_string())?;
    let url = file.download_url().ok_or_else(|| {
        "Автор запретил скачивание этой сборки сторонними лаунчерами. Скачайте её вручную и используйте «Импорт»."
            .to_string()
    })?;
    if !mq_mods::is_allowed_content_download_url(url) {
        return Err("Адрес скачивания не входит в список разрешённых".to_string());
    }
    if file.size > PACK_MAX_BYTES {
        return Err("Файл сборки слишком большой".to_string());
    }
    let name = safe_pack_file_name(&file.filename).ok_or("Недопустимое имя файла сборки")?;
    let dir = ctx.paths.temp_dir().join("catalog-packs");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| e.to_string())?;
    let dest = dir.join(&name);
    let mut spec = DownloadSpec::new(name.clone(), url.to_string(), dest.clone());
    spec.expected_sha1 = file.sha1;
    spec.expected_sha512 = file.sha512;
    spec.expected_size = (file.size > 0).then_some(file.size);
    let guard = ctx.epoch.snapshot();
    ctx.net
        .download_file(&spec, &guard)
        .await
        .map_err(|e| err_text(&ctx.config, &e))?;
    let dest2 = dest.clone();
    tokio::task::spawn_blocking(move || preview_archive(&dest2))
        .await
        .map_err(|e| e.to_string())?
}

/// Creates the instance for a modpack and installs it. Returns the new instance id and the
/// files that must be downloaded manually. On failure the half-created instance is removed.
pub async fn run_modpack_job(
    ctx: &OpsCtx,
    cancel: &AtomicBool,
    instance_name: &str,
    preview: &ModpackPreview,
) -> Result<(InstanceId, Vec<BlockedDownload>), String> {
    if cancel.load(Ordering::SeqCst) {
        return Err("Отменено".to_string());
    }
    let mgr = mq_instances::InstanceManager::new(ctx.paths.clone());
    let manifest = mgr
        .create_instance(
            instance_name,
            &preview.mc_version,
            preview.loader,
            preview.loader_version.clone(),
        )
        .map_err(|e| e.to_string())?;
    let id = manifest.id.clone();
    let dir = ctx.paths.instance_dir(id.as_str());
    let guard = ctx.epoch.snapshot();
    let archive = preview.archive_path.clone();
    let name = archive
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();

    let result: Result<Vec<BlockedDownload>, String> = async {
        if is_mrpack(&name) {
            mq_mods::install_mrpack(
                &archive,
                &dir,
                &ctx.paths,
                &ctx.net,
                &ctx.pool,
                &guard,
                Some(&ctx.bus),
            )
            .await
            .map(|_| Vec::new())
            .map_err(|e| err_text(&ctx.config, &e))
        } else {
            let cf = CurseForgeClient::from_config(&ctx.config).ok();
            mq_mods::install_curseforge_modpack(
                &archive,
                &dir,
                &ctx.paths,
                &ctx.net,
                cf.as_ref(),
                &ctx.pool,
                &guard,
            )
            .await
            .map(|o| o.blocked_downloads)
            .map_err(|e| err_text(&ctx.config, &e))
        }
    }
    .await;

    match result {
        Ok(blocked) if !cancel.load(Ordering::SeqCst) => Ok((id, blocked)),
        Ok(_) => {
            let _ = mgr.delete_instance(id.as_str());
            Err("Отменено".to_string())
        }
        Err(e) => {
            let _ = mgr.delete_instance(id.as_str());
            Err(e)
        }
    }
}

/// Shared watcher handle for the manual-download dialog.
pub type SharedWatcher = Arc<parking_lot::Mutex<BlockedDownloadsWatcher>>;

/// One blocking polling round; returns the remaining list and the names completed this round.
/// `dropped` are files dragged into the window.
pub fn poll_watcher(
    watcher: &SharedWatcher,
    dropped: &[PathBuf],
) -> (Vec<BlockedDownload>, Vec<String>) {
    let mut w = watcher.lock();
    let mut done: Vec<PathBuf> = Vec::new();
    for p in dropped {
        if let Ok(Some(dest)) = w.ingest_dropped_file(p) {
            done.push(dest);
        }
    }
    if let Ok(mut found) = w.poll_once() {
        done.append(&mut found);
    }
    let names = done
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_string))
        .collect();
    (w.pending_downloads().to_vec(), names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_file_names_cannot_escape() {
        assert_eq!(
            safe_pack_file_name("pack.mrpack").as_deref(),
            Some("pack.mrpack")
        );
        assert_eq!(
            safe_pack_file_name("../../etc/passwd").as_deref(),
            Some("passwd")
        );
        assert_eq!(safe_pack_file_name("..").as_deref(), None);
        assert_eq!(safe_pack_file_name("").as_deref(), None);
    }

    #[test]
    fn modrinth_type_paths() {
        assert_eq!(modrinth_type_path(ProjectKind::Shader), "shader");
        assert_eq!(modrinth_type_path(ProjectKind::Modpack), "modpack");
    }

    #[test]
    fn lock_source_mapping() {
        assert_eq!(lock_source(CatalogSource::Modrinth), LockSource::Modrinth);
        assert_eq!(
            lock_source(CatalogSource::CurseForge),
            LockSource::CurseForge
        );
    }

    #[test]
    fn preview_of_mrpack_counts_client_files() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.mrpack");
        let f = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(f);
        zip.start_file(
            "modrinth.index.json",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        let idx = serde_json::json!({
            "formatVersion": 1, "game": "minecraft", "versionId": "1", "name": "N",
            "dependencies": {"minecraft": "1.21.4", "fabric-loader": "0.16.0"},
            "files": [
              {"path":"mods/a.jar","hashes":{"sha1":"SHA1","sha512":"SHA512"},"downloads":["https://cdn.modrinth.com/a.jar"],"fileSize":100},
              {"path":"mods/b.jar","hashes":{"sha1":"SHA1","sha512":"SHA512"},"downloads":["https://cdn.modrinth.com/b.jar"],"fileSize":50,"env":{"client":"unsupported","server":"required"}}
            ]
        });
        let raw = idx
            .to_string()
            .replace("SHA512", &"0".repeat(128))
            .replace("SHA1", &"0".repeat(40));
        zip.write_all(raw.as_bytes()).unwrap();
        zip.finish().unwrap();
        let p = preview_archive(&path).unwrap();
        assert_eq!(p.mc_version, "1.21.4");
        assert_eq!(p.loader, ModLoaderKind::Fabric);
        assert_eq!(p.file_count, 1, "server-only file is skipped");
        assert_eq!(p.total_size, 100);
    }

    #[test]
    fn merge_dependency_plans_deduplicates_shared_dependencies_and_marks_roots() {
        let dl = |project_id: &str, is_dependency: bool| mq_mods::ResolvedModDownload {
            project_id: project_id.into(),
            version_id: format!("v-{project_id}"),
            filename: format!("{project_id}.jar"),
            url: format!("https://cdn.example.test/{project_id}.jar"),
            size: 10,
            sha1: None,
            sha512: None,
            is_dependency,
        };
        let opt = |project_id: &str| mq_mods::OptionalDependency {
            required_by_project_id: "mod-a".into(),
            project_id: Some(project_id.into()),
            version_id: Some("v1".into()),
            file_name: Some(format!("{project_id}.jar")),
        };

        let plan_a = DependencyResolutionPlan {
            downloads: vec![dl("fabric-api", true), dl("mod-a", false)],
            optional_dependencies: vec![opt("mod-b"), opt("extra-opt")],
            ..Default::default()
        };
        let plan_b = DependencyResolutionPlan {
            downloads: vec![
                dl("fabric-api", true),
                dl("mod-a", true),
                dl("mod-b", false),
            ],
            optional_dependencies: vec![opt("extra-opt")],
            ..Default::default()
        };

        let (merged, sources) = merge_dependency_plans(vec![
            (CatalogSource::Modrinth, "mod-a".into(), plan_a),
            (CatalogSource::CurseForge, "mod-b".into(), plan_b),
        ]);

        let ids: Vec<_> = merged
            .downloads
            .iter()
            .map(|d| (d.project_id.as_str(), d.is_dependency))
            .collect();
        assert_eq!(
            ids,
            [("fabric-api", true), ("mod-a", false), ("mod-b", false),]
        );
        assert_eq!(sources.get("fabric-api"), Some(&CatalogSource::Modrinth));
        assert_eq!(sources.get("mod-b"), Some(&CatalogSource::CurseForge));
        // mod-b became a downloaded root so it must not remain in optional_dependencies;
        // extra-opt appears only once.
        let opt_ids: Vec<_> = merged
            .optional_dependencies
            .iter()
            .filter_map(|o| o.project_id.as_deref())
            .collect();
        assert_eq!(opt_ids, ["extra-opt"]);
    }
}
