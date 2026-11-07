//! `mq-install`: Content-Addressable Storage (CAS), safe natives extraction,
//! game installation DAG pipeline, and multi-core deep file verification (`mq verify`).

#![deny(clippy::await_holding_lock)]

pub mod cas;
pub mod natives;
pub mod pipeline;
pub mod state;
pub mod verify;

pub use cas::{CasStore, LinkMethod, CAS_STREAM_CHUNK_SIZE};
pub use natives::{extract_natives_archives, NATIVES_CHUNK_SIZE};
pub use pipeline::{build_game_install_dag_plan, install_game, InstallRequest, InstallSummary};
pub use state::{
    available_disk_space, check_free_disk_space, InstallClientJarInfo, InstallError,
    InstallJavaInfo, InstallStage, InstallState, InstallStatus, InstanceInstallLock,
    INSTALL_STATE_FILENAME, INSTALL_STATE_SCHEMA_VERSION,
};
pub use verify::{
    collect_instance_verify_targets, verify_files_parallel, VerifyFileTarget, VerifyReport,
};
