//! Synthetic catalog scenes for `mq ui-snapshot --demo` (no network, no personal data).

use mq_mods::{
    BlockedDownload, CatalogSourceFilter, DependencyKind, DependencyResolutionPlan,
    ModDependencyRef, ProjectKind, ProjectVersion, ProjectVersionFile, ResolvedModDownload,
};

use crate::catalog_flow::{InstallDialog, InstallTarget, ModpackPreview, ProjectPage, RetrySpec};
use crate::catalog_state::{CatalogErrorKind, CatalogSourceError};
use crate::state::UiState;

fn version(id: &str, num: &str, mc: &str) -> ProjectVersion {
    ProjectVersion {
        id: id.into(),
        project_id: "AANobbMI".into(),
        name: format!("Release {num}"),
        version_number: num.into(),
        game_versions: vec![mc.into()],
        loaders: vec!["fabric".into()],
        files: vec![ProjectVersionFile {
            url: "https://cdn.modrinth.com/demo.jar".into(),
            filename: format!("demo-{num}.jar"),
            size: 1_420_180,
            sha1: None,
            sha512: None,
            primary: true,
        }],
        dependencies: vec![ModDependencyRef {
            project_id: Some("P7dR8mSH".into()),
            version_id: None,
            file_name: None,
            kind: DependencyKind::Required,
        }],
    }
}

fn dl(name: &str, size: u64, dep: bool, url: &str) -> ResolvedModDownload {
    ResolvedModDownload {
        project_id: name.into(),
        version_id: "v1".into(),
        filename: name.into(),
        url: url.into(),
        size,
        sha1: None,
        sha512: None,
        is_dependency: dep,
    }
}

fn blocked(name: &str) -> BlockedDownload {
    BlockedDownload {
        project_id: "1".into(),
        file_id: "2".into(),
        project_title: "Demo Mod".into(),
        filename: name.into(),
        webpage_url: "https://www.curseforge.com/minecraft/mc-mods/demo/files/2".into(),
        expected_size: 900_000,
        expected_sha1: None,
        expected_fingerprint: None,
        target_rel_path: format!("mods/{name}"),
    }
}

/// Applies a named scene on top of the demo state. Unknown names leave the state untouched.
pub fn apply_demo_scene(state: &mut UiState, scene: &str) {
    let first = state.catalog.hits.first().cloned();
    match scene {
        "catalog-nokey" => {
            state.catalog.source = CatalogSourceFilter::CurseForge;
            state.catalog.hits.clear();
        }
        "catalog-empty" => {
            state.catalog.hits.clear();
            state.catalog.query = "zzzzzz".into();
        }
        "catalog-error" => {
            state.catalog.hits.clear();
            state.catalog.errors = vec![CatalogSourceError {
                source: mq_mods::CatalogSource::Modrinth,
                kind: CatalogErrorKind::Network,
                message: String::new(),
            }];
        }
        "catalog-project" => {
            let Some(hit) = first else { return };
            let mut details = hit.clone().into_project_details();
            details.body = Some(
                "# Sodium\nСовременный движок рендеринга.\n- Повышает FPS\n- Совместим с Iris\n\n> Работает на Fabric и Quilt."
                    .into(),
            );
            state.project.begin(hit);
            let page = ProjectPage {
                details,
                versions: vec![
                    version("v3", "0.5.11", "1.21.4"),
                    version("v2", "0.5.9", "1.21"),
                    version("v1", "0.5.8", "1.20.6"),
                ],
            };
            let id = state.project.request_id;
            state.project.apply(id, Ok(page));
        }
        "catalog-install" | "catalog-manual" => {
            let Some(hit) = first else { return };
            let Some(inst) = state.selected_instance_id.clone() else {
                return;
            };
            let mut d = InstallDialog::new(hit, None, InstallTarget::Existing(inst.clone()));
            d.plan = Some(DependencyResolutionPlan {
                downloads: vec![
                    dl(
                        "fabric-api-0.102.0.jar",
                        2_150_000,
                        true,
                        "https://cdn.modrinth.com/a.jar",
                    ),
                    dl(
                        "sodium-0.5.11.jar",
                        1_420_180,
                        false,
                        "https://cdn.modrinth.com/b.jar",
                    ),
                ],
                blocked_downloads: vec![blocked("restricted-mod-1.0.jar")],
                ..DependencyResolutionPlan::default()
            });
            state.install = Some(d);
            if scene == "catalog-manual" {
                state.install = None;
                state.open_manual_dialog(
                    &inst,
                    vec![
                        blocked("restricted-mod-1.0.jar"),
                        blocked("another-mod-2.1.jar"),
                    ],
                );
                state.manual.watch_dir_label = "~/Downloads".into();
            }
        }
        "catalog-pack" => {
            let Some(mut hit) = state
                .catalog
                .hits
                .iter()
                .find(|h| h.project_kind == ProjectKind::Modpack)
                .cloned()
            else {
                return;
            };
            hit.title = "Fabulously Optimized".into();
            let mut d = InstallDialog::new(
                hit,
                None,
                InstallTarget::NewInstance {
                    name: "Fabulously Optimized".into(),
                },
            );
            d.preview = Some(ModpackPreview {
                mc_version: "1.21.4".into(),
                loader: mq_core::ModLoaderKind::Fabric,
                loader_version: Some("0.16.9".into()),
                file_count: 64,
                total_size: 184_320_000,
                blocked: vec![blocked("restricted-mod-1.0.jar")],
                archive_path: "demo.mrpack".into(),
            });
            d.confirm_modpack = true;
            state.install = Some(d);
        }
        "catalog-jobs" => {
            let spec = RetrySpec::Modpack {
                instance_name: "Demo".into(),
                preview: ModpackPreview {
                    mc_version: "1.21.4".into(),
                    loader: mq_core::ModLoaderKind::Fabric,
                    loader_version: None,
                    file_count: 64,
                    total_size: 0,
                    blocked: Vec::new(),
                    archive_path: "demo.mrpack".into(),
                },
            };
            let (a, _) = state.jobs.push(
                "Fabulously Optimized".into(),
                "Новый инстанс «Fabulously Optimized»".into(),
                64,
                Some(spec.clone()),
            );
            state.jobs.progress(a, 37, 64);
            let (b, _) = state.jobs.push(
                "Sodium".into(),
                "Инстанс «fabric-optimization»".into(),
                2,
                None,
            );
            state.jobs.finish(b, Ok(()));
            let (c, _) = state.jobs.push(
                "Iris Shaders".into(),
                "Инстанс «fabric-optimization»".into(),
                2,
                Some(spec),
            );
            state.jobs.finish(c, Err("нет связи с сервером".into()));
        }
        _ => {}
    }
}
