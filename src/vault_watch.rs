//! A bounded, demand-driven worker: filesystem polling never runs inside a paint pass.
use crate::storage;
use std::{collections::HashSet, path::PathBuf, sync::mpsc};

pub type Snapshot = HashSet<(PathBuf, u128)>;
pub struct SnapshotResult {
    pub root: PathBuf,
    pub epoch: u64,
    pub result: Result<Snapshot, String>,
}
pub struct SnapshotWorker {
    requests: mpsc::Sender<(PathBuf, u64)>,
    results: mpsc::Receiver<SnapshotResult>,
    busy: bool,
}

impl Default for SnapshotWorker {
    fn default() -> Self {
        let (requests, inbox) = mpsc::channel::<(PathBuf, u64)>();
        let (outbox, results) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok((root, epoch)) = inbox.recv() {
                let result = storage::vault_snapshot(&root).map_err(|error| error.to_string());
                if outbox
                    .send(SnapshotResult {
                        root,
                        epoch,
                        result,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            requests,
            results,
            busy: false,
        }
    }
}

impl SnapshotWorker {
    pub fn request(&mut self, root: PathBuf, epoch: u64) {
        if !self.busy {
            self.busy = self.requests.send((root, epoch)).is_ok();
        }
    }
    pub fn poll(&mut self) -> Option<SnapshotResult> {
        let result = self.results.try_recv().ok()?;
        self.busy = false;
        Some(result)
    }
}
