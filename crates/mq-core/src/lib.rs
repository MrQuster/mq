//! `mq-core`: Foundational types, unified errors, redacted secrets, strong digests/IDs,
//! XDG directory layout, TOML configuration, and the bounded lock-free `EventBus`.

pub mod config;
pub mod error;
pub mod events;
pub mod ids;
pub mod paths;
pub mod secret;

pub use config::{AppConfig, GuiBackend, ENV_CF_API_KEY, ENV_MS_CLIENT_ID};
pub use error::{MqError, MqResult};
pub use events::{
    CoreCpuLoad, CoreLoad, CpuSnapshot, Event, EventBus, GameLogEntry, GameLogLevel, PhaseMetrics,
    PhaseProgress, ProgressSnapshot, DEFAULT_EVENT_BUS_CAPACITY,
};
pub use ids::{
    AccountId, InstanceId, ModLoaderKind, Sha1Digest, Sha1Hash, Sha512Digest, Sha512Hash,
};
pub use paths::{resolve_safe_path, MqPaths};
pub use secret::{redact_text, SecretString, REDACTED_PLACEHOLDER};

#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;

    #[test]
    fn concurrent_producers_never_deadlock_bounded_event_bus() {
        let bus = EventBus::with_capacity(64);
        let mut handles = Vec::new();

        for worker_id in 0..8usize {
            let worker_bus = bus.clone();
            handles.push(thread::spawn(move || {
                for step in 0..500u64 {
                    worker_bus.publish(Event::Progress(PhaseProgress {
                        phase: format!("worker-{worker_id}"),
                        completed_items: step,
                        total_items: 500,
                        bytes_done: step * 1024,
                        bytes_total: 500 * 1024,
                        files_per_sec: 250.0,
                        mib_per_sec: 12.5,
                        active_tasks: 8,
                    }));
                }
            }));
        }

        for h in handles {
            h.join().expect("producer thread should finish cleanly");
        }

        let drained = bus.drain_all();
        assert!(drained.len() <= 64);
        assert!(!drained.is_empty());
        assert_eq!(drained.len() as u64 + bus.dropped_count(), 4000);
    }

    #[test]
    fn xdg_default_resolves_in_current_environment() {
        let paths = MqPaths::xdg_default().expect("resolve xdg_default");
        assert!(paths.data_dir.ends_with("mq"));
        assert!(paths.config_dir.ends_with("mq"));
        assert!(paths.cache_dir.ends_with("mq"));
        assert_eq!(
            paths.authlib_injector_jar(),
            paths.data_dir.join("cache").join("authlib-injector.jar")
        );
    }
}
