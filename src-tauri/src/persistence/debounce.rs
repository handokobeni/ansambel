use crate::error::Result;
use crate::persistence::atomic::write_atomic;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Notify;
use tokio::time::{sleep_until, Duration, Instant};

/// Closure that performs the actual write at flush time. Each queue
/// replaces any pending closure for the same path — last write wins.
type WriteFn = Box<dyn FnOnce() + Send + 'static>;

enum Msg {
    Queue {
        path: PathBuf,
        write_fn: WriteFn,
        deadline: Instant,
    },
    Flush,
}

#[derive(Clone)]
pub struct DebouncedWriter {
    tx: mpsc::UnboundedSender<Msg>,
    flushed: Arc<Notify>,
    debounce: Duration,
}

impl DebouncedWriter {
    pub fn new(debounce: Duration) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let flushed = Arc::new(Notify::new());
        let flushed_for_task = flushed.clone();
        tokio::spawn(worker(rx, flushed_for_task));
        Self {
            tx,
            flushed,
            debounce,
        }
    }

    /// Queues a value to be serialized to JSON and written atomically at the
    /// debounce deadline. Backwards-compatible shorthand around
    /// `queue_with(path, move || write_atomic(&path, &value))`.
    pub fn queue<T: Serialize>(&self, path: PathBuf, value: &T) -> Result<()> {
        let v = serde_json::to_value(value)?;
        let path_for_writer = path.clone();
        self.queue_with(path, move || {
            let _ = write_atomic(&path_for_writer, &v);
        })
    }

    /// Queues an arbitrary write closure. Useful when the on-disk format
    /// isn't a single serde value (e.g. JSONL message files where each
    /// flush rewrites a multi-line file). Replaces any pending closure for
    /// the same path so bursts collapse into a single write.
    pub fn queue_with<F>(&self, path: PathBuf, write_fn: F) -> Result<()>
    where
        F: FnOnce() + Send + 'static,
    {
        let deadline = Instant::now() + self.debounce;
        self.tx
            .send(Msg::Queue {
                path,
                write_fn: Box::new(write_fn),
                deadline,
            })
            .map_err(|e| crate::error::AppError::Other(format!("debouncer closed: {}", e)))?;
        Ok(())
    }

    pub async fn flush_all(&self) {
        let _ = self.tx.send(Msg::Flush);
        self.flushed.notified().await;
    }
}

async fn worker(mut rx: mpsc::UnboundedReceiver<Msg>, flushed: Arc<Notify>) {
    let mut pending: HashMap<PathBuf, (Instant, WriteFn)> = HashMap::new();
    loop {
        let next_deadline = pending.values().map(|(d, _)| *d).min();
        tokio::select! {
            Some(msg) = rx.recv() => {
                match msg {
                    Msg::Queue { path, write_fn, deadline } => {
                        pending.insert(path, (deadline, write_fn));
                    }
                    Msg::Flush => {
                        for (_path, (_, write_fn)) in pending.drain() {
                            let _ = tokio::task::spawn_blocking(move || {
                                write_fn();
                            }).await;
                        }
                        flushed.notify_waiters();
                    }
                }
            }
            _ = async {
                if let Some(d) = next_deadline { sleep_until(d).await; }
                else { std::future::pending::<()>().await; }
            } => {
                let now = Instant::now();
                let ready: Vec<PathBuf> = pending.iter()
                    .filter(|(_, (d, _))| *d <= now)
                    .map(|(p, _)| p.clone())
                    .collect();
                for path in ready {
                    if let Some((_, write_fn)) = pending.remove(&path) {
                        let _ = tokio::task::spawn_blocking(move || {
                            write_fn();
                        }).await;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::time::{sleep, Duration};

    #[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
    struct S {
        v: u32,
    }

    #[tokio::test]
    async fn single_queue_writes_after_debounce() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("x.json");

        let writer = DebouncedWriter::new(Duration::from_millis(50));
        writer.queue(path.clone(), &S { v: 1 }).unwrap();

        sleep(Duration::from_millis(200)).await;
        let loaded: S = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded, S { v: 1 });
    }

    #[tokio::test]
    async fn multiple_queues_collapse_to_one_write_with_latest_value() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("x.json");

        let writer = DebouncedWriter::new(Duration::from_millis(100));
        for i in 1..=5 {
            writer.queue(path.clone(), &S { v: i }).unwrap();
            sleep(Duration::from_millis(10)).await;
        }
        sleep(Duration::from_millis(300)).await;

        let loaded: S = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded, S { v: 5 }, "latest queued value wins");
    }

    #[tokio::test]
    async fn flush_writes_immediately() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("x.json");

        let writer = DebouncedWriter::new(Duration::from_millis(500));
        writer.queue(path.clone(), &S { v: 42 }).unwrap();
        writer.flush_all().await;

        let loaded: S = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded, S { v: 42 });
    }

    #[tokio::test]
    async fn different_paths_are_independent() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.json");
        let b = tmp.path().join("b.json");

        let writer = DebouncedWriter::new(Duration::from_millis(50));
        writer.queue(a.clone(), &S { v: 1 }).unwrap();
        writer.queue(b.clone(), &S { v: 2 }).unwrap();

        sleep(Duration::from_millis(200)).await;

        let la: S = serde_json::from_str(&std::fs::read_to_string(&a).unwrap()).unwrap();
        let lb: S = serde_json::from_str(&std::fs::read_to_string(&b).unwrap()).unwrap();
        assert_eq!(la, S { v: 1 });
        assert_eq!(lb, S { v: 2 });
    }

    #[tokio::test]
    async fn queue_with_runs_custom_closure_at_flush() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("custom.txt");
        let path_for_writer = path.clone();

        let writer = DebouncedWriter::new(Duration::from_millis(50));
        writer
            .queue_with(path.clone(), move || {
                std::fs::write(&path_for_writer, b"hello jsonl").unwrap();
            })
            .unwrap();
        writer.flush_all().await;

        assert_eq!(std::fs::read(&path).unwrap(), b"hello jsonl");
    }

    #[tokio::test]
    async fn queue_with_collapses_burst_to_one_call_per_path() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("counter.txt");

        // Use an Arc<AtomicUsize> to count how many closures actually run.
        let counter = Arc::new(AtomicUsize::new(0));
        let writer = DebouncedWriter::new(Duration::from_millis(80));

        for _ in 0..10 {
            let counter = counter.clone();
            let path_for_writer = path.clone();
            writer
                .queue_with(path.clone(), move || {
                    counter.fetch_add(1, Ordering::SeqCst);
                    std::fs::write(&path_for_writer, b"x").unwrap();
                })
                .unwrap();
        }
        writer.flush_all().await;

        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "10 queues for the same path should collapse to a single closure invocation"
        );
    }
}
