use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use notify::{Error, RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcher};
use notify_debouncer_full::{
    new_debouncer, DebounceEventResult, DebouncedEvent, Debouncer, FileIdMap,
};
use tokio::{
    runtime::Handle,
    sync::mpsc::{channel, Receiver},
};

use crate::TX;

async fn broadcast() {
    let tx = TX.get().unwrap();
    let _ = tx.send(());
}

pub struct Watcher {
    debouncer: Debouncer<RecommendedWatcher, FileIdMap>,
    rx: Receiver<Result<Vec<DebouncedEvent>, Vec<notify::Error>>>,
}

pub(crate) async fn create_watcher() -> Result<Watcher, String> {
    let rt = Handle::current();
    let (tx, rx) = channel::<Result<Vec<DebouncedEvent>, Vec<Error>>>(16);
    new_debouncer(
        Duration::from_millis(200),
        None,
        move |result: DebounceEventResult| {
            let tx = tx.clone();
            rt.spawn(async move {
                if let Err(err) = tx.send(result).await {
                    log::error!("Failed to send event result: {}", err);
                }
            });
        },
    )
    .map(|debouncer| Watcher { debouncer, rx })
    .map_err(|e| e.to_string())
}

pub async fn watch(root_path: PathBuf, mut watcher: Watcher) {
    watcher
        .debouncer
        .watcher()
        .watch(&root_path, RecursiveMode::Recursive)
        .unwrap();
    watcher
        .debouncer
        .cache()
        .add_root(&root_path, RecursiveMode::Recursive);

    while let Some(result) = watcher.rx.recv().await {
        let mut files_changed = false;
        match result {
            Ok(events) => {
                for e in events {
                    use notify::EventKind::*;
                    match e.event.kind {
                        Create(_) => {
                            let path = e.event.paths[0].to_str().unwrap();
                            log::debug!("[CREATE] {}", path);
                            files_changed = true;
                        }
                        Modify(kind) => {
                            use notify::event::ModifyKind::*;
                            match kind {
                                Name(kind) => {
                                    use notify::event::RenameMode::*;
                                    if let Both = kind {
                                        let source_name = &e.event.paths[0];
                                        let target_name = &e.event.paths[1];
                                        log::debug!(
                                            "[RENAME] {} -> {}",
                                            strip_prefix(source_name, &root_path),
                                            strip_prefix(target_name, &root_path)
                                        );
                                        files_changed = true;
                                    }
                                }
                                _ => {
                                    let paths = e.event.paths[0].to_str().unwrap();
                                    log::debug!("[UPDATE] {}", paths);
                                    files_changed = true;
                                }
                            }
                        }
                        Remove(_) => {
                            let paths = e.event.paths[0].to_str().unwrap();
                            log::debug!("[REMOVE] {}", paths);
                            files_changed = true;
                        }
                        _ => {}
                    }
                }
            }
            Err(errors) => {
                for err in errors {
                    log::error!("{}", err);
                }
            }
        }
        if files_changed {
            broadcast().await;
        }
    }
}

fn strip_prefix(path: &Path, prefix: &PathBuf) -> String {
    path.strip_prefix(prefix)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string()
}
