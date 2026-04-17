use crate::error::Result;
use crate::persistence::atomic::write_atomic;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Notify;
use tokio::time::{sleep_until, Duration, Instant};

enum Msg {
    Queue {
        path: PathBuf,
        value: serde_json::Value,
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

    pub fn queue<T: Serialize>(&self, path: PathBuf, value: &T) -> Result<()> {
        let v = serde_json::to_value(value)?;
        let deadline = Instant::now() + self.debounce;
        self.tx
            .send(Msg::Queue {
                path,
                value: v,
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
    let mut pending: HashMap<PathBuf, (Instant, serde_json::Value)> = HashMap::new();
    loop {
        let next_deadline = pending.values().map(|(d, _)| *d).min();
        tokio::select! {
            Some(msg) = rx.recv() => {
                match msg {
                    Msg::Queue { path, value, deadline } => {
                        pending.insert(path, (deadline, value));
                    }
                    Msg::Flush => {
                        for (path, (_, value)) in pending.drain() {
                            let _ = tokio::task::spawn_blocking(move || {
                                let _ = write_atomic(&path, &value);
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
                    if let Some((_, value)) = pending.remove(&path) {
                        let p = path.clone();
                        let v = value.clone();
                        let _ = tokio::task::spawn_blocking(move || {
                            let _ = write_atomic(&p, &v);
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
}
