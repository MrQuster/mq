#![deny(clippy::await_holding_lock)]
//! UI-thread actions of the catalog flows: they only mutate state and enqueue worker commands.

use std::collections::BTreeSet;

use mq_core::{InstanceId, ModLoaderKind};
use mq_mods::{CatalogHit, ProjectKind, ProjectVersion};

use crate::catalog_flow::{InstallDialog, InstallTarget, RetrySpec};
use crate::catalog_ops::PlanRequest;
use crate::state::{UiState, UiWorkerCommand};

/// Loader filter for a content kind: only mods (and modpacks) depend on the loader.
pub fn loader_filter(kind: ProjectKind, loader: ModLoaderKind) -> Option<ModLoaderKind> {
    match kind {
        ProjectKind::Mod | ProjectKind::Modpack if loader != ModLoaderKind::Vanilla => Some(loader),
        _ => None,
    }
}

impl UiState {
    /// Opens the project page and starts loading its details and versions.
    pub fn open_project(&mut self, hit: CatalogHit) {
        let (source, project_id, kind) = (hit.source, hit.project_id.clone(), hit.project_kind);
        let request_id = self.project.begin(hit);
        let _ = self.cmd_sender.send(UiWorkerCommand::LoadProject {
            request_id,
            source,
            project_id,
            kind,
        });
    }

    /// Opens the install dialog for mods / resource packs / shaders. The default target is the
    /// currently open instance.
    pub fn open_install_content(&mut self, hit: CatalogHit) {
        let Some(id) = self.selected_instance_id.clone() else {
            self.status_message = Some(("Сначала выберите инстанс".to_string(), true));
            return;
        };
        self.install = Some(InstallDialog::new(hit, None, InstallTarget::Existing(id)));
        self.replan_install();
    }

    /// Opens the confirmation dialog for creating a new instance from a modpack version.
    pub fn open_install_modpack(&mut self, hit: CatalogHit, version: ProjectVersion) {
        let name = hit.title.clone();
        self.install = Some(InstallDialog::new(
            hit,
            Some(version),
            InstallTarget::NewInstance { name },
        ));
        self.replan_install();
    }

    /// Starts importing a local `.mrpack` / CurseForge `.zip`: shows the confirmation dialog.
    pub fn import_local_pack(&mut self, path: std::path::PathBuf) {
        let lower = path.to_string_lossy().to_ascii_lowercase();
        if !(lower.ends_with(".mrpack") || lower.ends_with(".zip")) {
            self.status_message = Some((
                "Поддерживаются файлы .mrpack и .zip (CurseForge)".to_string(),
                true,
            ));
            return;
        }
        let title = path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("Сборка")
            .to_string();
        let hit = CatalogHit {
            source: mq_mods::CatalogSource::Modrinth,
            project_id: String::new(),
            slug: title.clone(),
            title: title.clone(),
            description: String::new(),
            author: "Локальный файл".to_string(),
            downloads: 0,
            icon_url: None,
            project_kind: ProjectKind::Modpack,
            loaders: Vec::new(),
            game_versions: Vec::new(),
            latest_version: None,
        };
        self.install = Some(InstallDialog::new(
            hit,
            None,
            InstallTarget::NewInstance { name: title },
        ));
        self.install_seq += 1;
        let request_id = self.install_seq;
        if let Some(d) = self.install.as_mut() {
            d.request_id = request_id;
            d.loading = true;
        }
        let _ = self
            .cmd_sender
            .send(UiWorkerCommand::PreviewLocalPack { request_id, path });
    }

    /// Starts looking for newer versions of the files installed in `id`.
    pub fn check_updates(&mut self, id: InstanceId) {
        let Some(inst) = self.instances.iter().find(|i| i.id == id) else {
            return;
        };
        let (mc_version, loader) = (inst.mc_version.clone(), inst.loader);
        self.updates.request_id += 1;
        self.updates.instance = Some(id.clone());
        self.updates.checking = true;
        self.updates.checked = false;
        self.updates.items.clear();
        self.updates.error = None;
        let _ = self.cmd_sender.send(UiWorkerCommand::CheckUpdates {
            request_id: self.updates.request_id,
            instance_id: id,
            mc_version,
            loader,
        });
    }

    /// Updates one file from the last check; progress shows up in the downloads queue.
    pub fn apply_update(&mut self, index: usize) {
        let Some(id) = self.updates.instance.clone() else {
            return;
        };
        if index >= self.updates.items.len() {
            return;
        }
        let cand = self.updates.items.remove(index);
        let (job_id, _cancel) = self.jobs.push(
            format!("Обновление: {}", cand.new.filename),
            format!("Инстанс «{id}»"),
            1,
            None,
        );
        let _ = self.cmd_sender.send(UiWorkerCommand::ApplyUpdate {
            job_id,
            instance_id: id,
            candidate: Box::new(cand),
        });
    }

    /// Changes the target instance of the open dialog and recomputes the plan.
    pub fn set_install_target(&mut self, id: InstanceId) {
        if let Some(d) = self.install.as_mut() {
            d.target = InstallTarget::Existing(id);
            d.selected_optionals.clear();
        }
        self.replan_install();
    }

    /// Recomputes the dependency plan (or modpack preview) for the open dialog.
    pub fn replan_install(&mut self) {
        self.install_seq += 1;
        let request_id = self.install_seq;
        let Some(d) = self.install.as_mut() else {
            return;
        };
        d.request_id = request_id;
        d.loading = true;
        d.error = None;
        d.plan = None;

        if d.is_modpack() {
            d.preview = None;
            d.confirm_modpack = false;
            let Some(version) = d.version.clone() else {
                d.loading = false;
                d.error = Some("Не выбрана версия сборки".to_string());
                return;
            };
            let _ = self.cmd_sender.send(UiWorkerCommand::PrepareModpack {
                request_id,
                version,
            });
            return;
        }

        let InstallTarget::Existing(id) = d.target.clone() else {
            d.loading = false;
            d.error = Some("Не выбран инстанс".to_string());
            return;
        };
        let (kind, source, project_id) =
            (d.hit.project_kind, d.hit.source, d.hit.project_id.clone());
        let selected: BTreeSet<String> = d.selected_optionals.clone();
        let Some(inst) = self.instances.iter().find(|i| i.id == id) else {
            d.loading = false;
            d.error = Some("Инстанс не найден".to_string());
            return;
        };
        let req = PlanRequest {
            source,
            project_id,
            mc_version: inst.mc_version.clone(),
            loader: loader_filter(kind, inst.loader),
            kind,
            selected_optionals: selected,
            instance_dir: Some(self.paths.instance_dir(id.as_str())),
        };
        let _ = self
            .cmd_sender
            .send(UiWorkerCommand::PlanInstall { request_id, req });
    }

    /// Confirms the dialog: queues a job (and opens the manual dialog for blocked files).
    pub fn confirm_install(&mut self) {
        let Some(d) = self.install.as_ref() else {
            return;
        };
        if !d.can_confirm() {
            return;
        }
        let d = self.install.take().expect("checked above");

        if d.is_modpack() {
            let (Some(preview), InstallTarget::NewInstance { name }) = (d.preview, d.target) else {
                return;
            };
            let name = if name.trim().is_empty() {
                d.hit.title.clone()
            } else {
                name
            };
            let retry = RetrySpec::Modpack {
                instance_name: name.clone(),
                preview: preview.clone(),
            };
            let (job_id, cancel) = self.jobs.push(
                d.hit.title.clone(),
                format!("Новый инстанс «{name}»"),
                preview.file_count,
                Some(retry),
            );
            let _ = self.cmd_sender.send(UiWorkerCommand::StartModpackJob {
                job_id,
                cancel,
                instance_name: name,
                preview,
            });
            return;
        }

        let (Some(plan), InstallTarget::Existing(instance_id)) = (d.plan, d.target) else {
            return;
        };
        let kind = d.hit.project_kind;
        let (blocked_dl, downloads): (Vec<_>, Vec<_>) = plan
            .downloads
            .into_iter()
            .partition(mq_mods::ResolvedModDownload::is_download_blocked);
        let blocked = plan.blocked_downloads;
        if downloads.is_empty() {
            // Nothing can be fetched automatically: go straight to the manual dialog.
            if !blocked.is_empty() || !blocked_dl.is_empty() {
                self.open_manual_dialog(&instance_id, blocked);
            }
            return;
        }
        let retry = RetrySpec::Content {
            instance_id: instance_id.clone(),
            kind,
            source: d.hit.source,
            downloads: downloads.clone(),
            blocked: blocked.clone(),
        };
        let (job_id, cancel) = self.jobs.push(
            d.hit.title.clone(),
            format!("Инстанс «{instance_id}»"),
            downloads.len(),
            Some(retry),
        );
        let _ = self.cmd_sender.send(UiWorkerCommand::StartContentJob {
            job_id,
            cancel,
            instance_id,
            kind,
            source: d.hit.source,
            downloads,
            blocked,
        });
    }

    /// Requests cancellation of a running job.
    pub fn cancel_job(&mut self, job_id: u64) {
        if let Some(j) = self.jobs.get_mut(job_id) {
            j.request_cancel();
        }
    }

    /// Restarts a failed or cancelled job.
    pub fn retry_job(&mut self, job_id: u64) {
        let Some((cancel, spec)) = self.jobs.restart(job_id) else {
            return;
        };
        let cmd = match spec {
            RetrySpec::Content {
                instance_id,
                kind,
                source,
                downloads,
                blocked,
            } => UiWorkerCommand::StartContentJob {
                job_id,
                cancel,
                instance_id,
                kind,
                source,
                downloads,
                blocked,
            },
            RetrySpec::Modpack {
                instance_name,
                preview,
            } => UiWorkerCommand::StartModpackJob {
                job_id,
                cancel,
                instance_name,
                preview,
            },
        };
        let _ = self.cmd_sender.send(cmd);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog_flow::JobStatus;
    use mq_mods::{CatalogSource, DependencyResolutionPlan, ResolvedModDownload};
    use mq_sched::ComputePool;
    use std::sync::Arc;

    fn state() -> (UiState, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let paths = mq_core::MqPaths::with_root(tmp.path().to_path_buf());
        let st = UiState::new(
            paths,
            mq_core::AppConfig::default(),
            Arc::new(ComputePool::new(1)),
        )
        .unwrap();
        (st, tmp)
    }

    #[test]
    fn import_rejects_unknown_extension_and_opens_dialog_for_packs() {
        let (mut st, _t) = state();
        st.import_local_pack("/tmp/evil.exe".into());
        assert!(st.install.is_none());
        assert!(st.status_message.as_ref().is_some_and(|m| m.1));
        st.import_local_pack("/tmp/Pack.mrpack".into());
        let d = st.install.as_ref().expect("dialog");
        assert!(d.loading && d.is_modpack());
        assert_eq!(
            d.target,
            InstallTarget::NewInstance {
                name: "Pack".into()
            }
        );
    }

    fn hit(kind: ProjectKind) -> CatalogHit {
        CatalogHit {
            source: CatalogSource::Modrinth,
            project_id: "AANobbMI".into(),
            slug: "sodium".into(),
            title: "Sodium".into(),
            description: String::new(),
            author: String::new(),
            downloads: 0,
            icon_url: None,
            project_kind: kind,
            loaders: vec![],
            game_versions: vec![],
            latest_version: None,
        }
    }

    fn dl(name: &str, url: &str) -> ResolvedModDownload {
        ResolvedModDownload {
            project_id: name.into(),
            version_id: "v".into(),
            filename: format!("{name}.jar"),
            url: url.into(),
            size: 10,
            sha1: None,
            sha512: None,
            is_dependency: false,
        }
    }

    #[test]
    fn loader_filter_only_for_mods_and_packs() {
        assert_eq!(
            loader_filter(ProjectKind::Mod, ModLoaderKind::Fabric),
            Some(ModLoaderKind::Fabric)
        );
        assert_eq!(
            loader_filter(ProjectKind::Shader, ModLoaderKind::Fabric),
            None
        );
        assert_eq!(
            loader_filter(ProjectKind::Mod, ModLoaderKind::Vanilla),
            None
        );
    }

    #[test]
    fn open_install_without_instance_reports_error() {
        let (mut st, _t) = state();
        st.selected_instance_id = None;
        st.open_install_content(hit(ProjectKind::Mod));
        assert!(st.install.is_none());
        assert!(st.status_message.as_ref().is_some_and(|(_, err)| *err));
    }

    #[test]
    fn install_flow_sends_plan_then_job_and_skips_blocked() {
        let (mut st, _t) = state();
        let inst = st
            .instance_mgr
            .create_instance("Test", "1.21.1", ModLoaderKind::Fabric, None)
            .unwrap();
        st.instances = st.instance_mgr.list_instances().unwrap();
        st.selected_instance_id = Some(inst.id.clone());

        st.open_install_content(hit(ProjectKind::Mod));
        match st.cmd_receiver.try_recv().unwrap() {
            UiWorkerCommand::PlanInstall { req, request_id } => {
                assert_eq!(req.mc_version, "1.21.1");
                assert_eq!(req.loader, Some(ModLoaderKind::Fabric));
                assert_eq!(request_id, st.install.as_ref().unwrap().request_id);
            }
            _ => panic!("expected PlanInstall"),
        }
        assert!(
            !st.install.as_ref().unwrap().can_confirm(),
            "plan not ready"
        );

        // plan arrives: one downloadable file, one blocked
        let d = st.install.as_mut().unwrap();
        d.loading = false;
        d.plan = Some(DependencyResolutionPlan {
            downloads: vec![
                dl("dep", "https://cdn.modrinth.com/dep.jar"),
                dl("blocked", ""),
            ],
            ..Default::default()
        });
        st.confirm_install();
        assert!(st.install.is_none());
        match st.cmd_receiver.try_recv().unwrap() {
            UiWorkerCommand::StartContentJob {
                downloads,
                instance_id,
                ..
            } => {
                assert_eq!(downloads.len(), 1);
                assert_eq!(downloads[0].project_id, "dep");
                assert_eq!(instance_id, inst.id);
            }
            _ => panic!("expected StartContentJob"),
        }
        assert_eq!(st.jobs.jobs.len(), 1);
        assert_eq!(st.jobs.jobs[0].total_files, 1);
    }

    #[test]
    fn cancel_and_retry_job() {
        let (mut st, _t) = state();
        let inst = st
            .instance_mgr
            .create_instance("Test", "1.21.1", ModLoaderKind::Fabric, None)
            .unwrap();
        let spec = RetrySpec::Content {
            instance_id: inst.id.clone(),
            kind: ProjectKind::Mod,
            source: CatalogSource::Modrinth,
            downloads: vec![dl("a", "https://cdn.modrinth.com/a.jar")],
            blocked: vec![],
        };
        let (id, _) = st.jobs.push("A".into(), "t".into(), 1, Some(spec));
        st.cancel_job(id);
        st.finish_job(id, Err("Отменено".into()));
        assert_eq!(st.jobs.jobs[0].status, JobStatus::Cancelled);
        st.retry_job(id);
        assert_eq!(st.jobs.jobs[0].status, JobStatus::Running);
        assert!(matches!(
            st.cmd_receiver.try_recv().unwrap(),
            UiWorkerCommand::StartContentJob { .. }
        ));
    }

    #[test]
    fn all_blocked_opens_manual_dialog_without_job() {
        let (mut st, _t) = state();
        let inst = st
            .instance_mgr
            .create_instance("Test", "1.21.1", ModLoaderKind::Fabric, None)
            .unwrap();
        st.instances = st.instance_mgr.list_instances().unwrap();
        st.selected_instance_id = Some(inst.id.clone());
        st.open_install_content(hit(ProjectKind::Mod));
        let _ = st.cmd_receiver.try_recv();
        let blocked = mq_mods::BlockedDownload {
            project_id: "1".into(),
            file_id: "2".into(),
            project_title: "T".into(),
            filename: "x.jar".into(),
            webpage_url: "https://www.curseforge.com/minecraft/mc-mods/x/download/2".into(),
            expected_size: 0,
            expected_sha1: None,
            expected_fingerprint: None,
            target_rel_path: "mods/x.jar".into(),
        };
        let d = st.install.as_mut().unwrap();
        d.loading = false;
        d.plan = Some(DependencyResolutionPlan {
            downloads: vec![dl("x", "")],
            blocked_downloads: vec![blocked],
            ..Default::default()
        });
        st.confirm_install();
        assert!(st.jobs.jobs.is_empty());
        assert!(st.manual.open);
        assert_eq!(st.manual.remaining(), 1);
        assert!(st.manual_watcher.is_some());
    }
}
