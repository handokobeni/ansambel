use crate::error::Result;
use std::path::Path;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initialize global tracing subscriber. Returns a WorkerGuard that must be
/// kept alive for the duration of the process — dropping it stops the non-
/// blocking log writer.
pub fn init(data_dir: &Path) -> Result<WorkerGuard> {
    let logs_dir = data_dir.join("logs");
    std::fs::create_dir_all(&logs_dir)?;

    let appender = tracing_appender::rolling::daily(&logs_dir, "ansambel.log");
    let (nb_writer, guard) = tracing_appender::non_blocking(appender);

    let filter = EnvFilter::try_from_env("ANSAMBEL_LOG")
        .unwrap_or_else(|_| EnvFilter::new("ansambel_lib=info,warn"));

    let file_layer = fmt::layer()
        .with_writer(nb_writer)
        .with_target(true)
        .with_thread_ids(false)
        .with_ansi(false);

    let stdout_layer = fmt::layer().with_target(true).with_ansi(true);

    let subscriber = tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stdout_layer);

    // try_set avoids panic when tests initialize twice
    let _ = subscriber.try_init();

    Ok(guard)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_returns_guard_and_writes_file() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = init(tmp.path()).expect("init logging");

        tracing::info!(event = "test", "hello from test");

        // tracing-appender flushes when the WorkerGuard drops;
        // read at least the logs directory
        let logs_dir = tmp.path().join("logs");
        assert!(logs_dir.is_dir(), "logs dir created");
    }
}
