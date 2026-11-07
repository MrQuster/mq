//! Child JVM process runner with non-blocking asynchronous `stdout`/`stderr` line capture,
//! automatic secret token redaction (`[REDACTED]`), `GameLogLevel` classification, and
//! `EventBus` streaming.

use std::fs;
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;

use mq_core::{redact_text, Event, EventBus, GameLogEntry, GameLogLevel, MqError, MqResult};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::command::BuiltLaunchCommand;

/// Spawns the configured Minecraft JVM (or wrapper) child process, streams `stdout` and `stderr`
/// lines asynchronously without blocking, scrubs any sensitive tokens in `secrets` using
/// [`redact_text`], classifies log severity via [`GameLogLevel::from_line`], and publishes
/// [`Event::GameLog`] entries to `event_bus`.
pub async fn spawn_and_stream_game(
    cmd: &BuiltLaunchCommand,
    instance_id: &str,
    secrets: Vec<String>,
    event_bus: Option<EventBus>,
) -> MqResult<ExitStatus> {
    if !cmd.working_dir.as_os_str().is_empty() {
        fs::create_dir_all(&cmd.working_dir)?;
    }

    let mut command = Command::new(&cmd.program);
    command
        .args(&cmd.args)
        .envs(&cmd.env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if !cmd.working_dir.as_os_str().is_empty() {
        command.current_dir(&cmd.working_dir);
    }

    #[cfg(target_os = "windows")]
    {
        // 0x08000000 = CREATE_NO_WINDOW: prevent console window popup for Minecraft javaw/java
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }

    let mut child = command.spawn().map_err(|err| {
        MqError::Process(format!(
            "failed to spawn game process '{}': {err}",
            cmd.program.display()
        ))
    })?;

    let shared_secrets = Arc::new(secrets);
    let instance_slug = instance_id.to_owned();

    let stdout_task = child.stdout.take().map(|stdout| {
        let reader = BufReader::new(stdout);
        let inst = instance_slug.clone();
        let sec = Arc::clone(&shared_secrets);
        let bus = event_bus.clone();
        tokio::spawn(async move {
            stream_log_lines(reader, inst, sec, bus).await;
        })
    });

    let stderr_task = child.stderr.take().map(|stderr| {
        let reader = BufReader::new(stderr);
        let inst = instance_slug;
        let sec = Arc::clone(&shared_secrets);
        let bus = event_bus;
        tokio::spawn(async move {
            stream_log_lines(reader, inst, sec, bus).await;
        })
    });

    let status = child.wait().await.map_err(|err| {
        MqError::Process(format!(
            "error waiting for game process '{}': {err}",
            cmd.program.display()
        ))
    })?;

    if let Some(handle) = stdout_task {
        let _ = handle.await;
    }
    if let Some(handle) = stderr_task {
        let _ = handle.await;
    }

    Ok(status)
}

async fn stream_log_lines<R>(
    reader: R,
    instance_id: String,
    secrets: Arc<Vec<String>>,
    event_bus: Option<EventBus>,
) where
    R: AsyncBufRead + Unpin,
{
    let secret_refs: Vec<&str> = secrets.iter().map(String::as_str).collect();
    let mut lines = reader.lines();

    while let Ok(Some(raw_line)) = lines.next_line().await {
        let redacted = redact_text(&raw_line, &secret_refs);
        let level = GameLogLevel::from_line(&redacted);
        let mut entry = GameLogEntry::from_raw_line(instance_id.clone(), redacted);
        entry.level = level;

        if let Some(ref bus) = event_bus {
            bus.publish(Event::GameLog(entry));
        }
    }
}

/// Terminates a game process and all of its descendant child processes.
/// On Windows, executes `taskkill /F /T /PID <pid>`.
/// On Unix, terminates the process via SIGKILL.
pub fn kill_process_tree(pid: u32) -> Result<(), std::io::Error> {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .output();
        Ok(())
    }
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .output();
        Ok(())
    }
}
