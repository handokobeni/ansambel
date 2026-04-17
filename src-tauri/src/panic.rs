use std::path::PathBuf;

pub fn install_hook(data_dir: PathBuf) {
    let crash_dir = data_dir.join("logs").join("crashes");
    let _ = std::fs::create_dir_all(&crash_dir);

    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown".into());
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .unwrap_or("<non-string payload>");
        let backtrace = std::backtrace::Backtrace::capture();

        tracing::error!(location = %location, message = %message, "PANIC");

        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let crash_file = crash_dir.join(format!("crash-{}.txt", ts));
        let content = format!(
            "Panic at {}\nMessage: {}\n\nBacktrace:\n{:?}\n",
            location, message, backtrace
        );
        let _ = std::fs::write(&crash_file, content);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_hook_does_not_panic() {
        let tmp = tempfile::tempdir().unwrap();
        install_hook(tmp.path().to_path_buf());
        // Trigger a catch_unwind to verify hook is callable without side effects we care about in test.
        let r = std::panic::catch_unwind(|| {
            panic!("simulated panic for test");
        });
        assert!(r.is_err());

        // crash log file should exist
        let crashes = tmp.path().join("logs/crashes");
        assert!(crashes.is_dir());
        let entries: Vec<_> = std::fs::read_dir(&crashes).unwrap().collect();
        assert!(!entries.is_empty(), "at least one crash log written");
    }
}
