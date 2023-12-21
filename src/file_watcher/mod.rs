#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use crate::InlyneEvent;

use notify::event::{EventKind, ModifyKind};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher as _};
use notify_debouncer_full::{
    new_debouncer, DebounceEventHandler, DebounceEventResult, Debouncer, FileIdMap,
};
use winit::event_loop::EventLoopProxy;

trait Callback: Send + 'static {
    fn file_reload(&self);
    fn file_change(&self, contents: String);
}

impl Callback for EventLoopProxy<InlyneEvent> {
    fn file_reload(&self) {
        let _ = self.send_event(InlyneEvent::FileReload);
    }

    fn file_change(&self, contents: String) {
        let _ = self.send_event(InlyneEvent::FileChange { contents });
    }
}

enum WatcherMsg {
    // Sent by the registered file watcher
    Notify(notify::Result<Event>),
    // Sent by the event loop
    FileChange { new_path: PathBuf, contents: String },
}

struct MsgHandler(mpsc::Sender<WatcherMsg>);

impl DebounceEventHandler for MsgHandler {
    fn handle_event(&mut self, debounced_event: DebounceEventResult) {
        log::debug!("Received debounced file events: {:#?}", debounced_event);

        match debounced_event {
            Ok(events) => {
                let mut selected_event: Option<Event> = None;

                // select the most interesting event
                // Rename/Remove is more interesting than changing the contents
                for ev in events {
                    match ev.event.kind {
                        EventKind::Modify(ModifyKind::Name(_)) | EventKind::Remove(_) => {
                            let _ = selected_event.insert(ev.event);
                        }
                        EventKind::Create(_) | EventKind::Modify(_) => {
                            let _ = selected_event.get_or_insert(ev.event);
                        }
                        _ => {}
                    }
                }

                if let Some(event) = selected_event {
                    let msg = WatcherMsg::Notify(Ok(event));
                    let _ = self.0.send(msg);
                } else {
                    log::trace!("Ignoring events")
                }
            }
            Err(err) => {
                // log all errors
                for e in err {
                    let msg = WatcherMsg::Notify(Err(e));
                    let _ = self.0.send(msg);
                }
            }
        }
    }
}

pub struct Watcher(mpsc::Sender<WatcherMsg>);

impl Watcher {
    pub fn spawn(event_proxy: EventLoopProxy<InlyneEvent>, file_path: PathBuf) -> Self {
        Self::spawn_inner(event_proxy, file_path)
    }

    fn spawn_inner<C: Callback>(reload_callback: C, file_path: PathBuf) -> Self {
        let (msg_tx, msg_rx) = mpsc::channel();
        let watcher = Self(msg_tx.clone());

        let notify_watcher =
            new_debouncer(Duration::from_millis(10), None, MsgHandler(msg_tx)).unwrap();

        std::thread::spawn(move || {
            endlessly_handle_messages(notify_watcher, msg_rx, reload_callback, file_path);
        });

        watcher
    }

    pub fn update_file(&self, new_path: &Path, contents: String) {
        let msg = WatcherMsg::FileChange {
            new_path: new_path.to_owned(),
            contents,
        };
        let _ = self.0.send(msg);
    }
}

fn endlessly_handle_messages<C: Callback>(
    mut watcher: Debouncer<RecommendedWatcher, FileIdMap>,
    msg_rx: mpsc::Receiver<WatcherMsg>,
    reload_callback: C,
    mut file_path: PathBuf,
) {
    let watcher = watcher.watcher();
    watcher
        .watch(&file_path, RecursiveMode::NonRecursive)
        .unwrap();

    let poll_registering_watcher = |watcher: &mut RecommendedWatcher, file_path: &Path| loop {
        std::thread::sleep(Duration::from_millis(15));

        let _ = watcher.unwatch(file_path);
        if watcher
            .watch(file_path, RecursiveMode::NonRecursive)
            .is_ok()
        {
            break;
        }
    };

    while let Ok(msg) = msg_rx.recv() {
        match msg {
            WatcherMsg::Notify(Ok(event)) => {
                log::debug!("Processing file event: {:#?}", event);

                if matches!(
                    event.kind,
                    EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_))
                ) {
                    log::debug!("File may have been renamed/removed. Falling back to polling");
                    poll_registering_watcher(watcher, &file_path);
                    log::debug!("Successfully re-registered file watcher");
                    reload_callback.file_reload();
                } else if matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                    log::debug!("Reloading file");
                    reload_callback.file_reload();
                }
            }
            WatcherMsg::Notify(Err(err)) => log::warn!("File watcher error: {}", err),
            WatcherMsg::FileChange { new_path, contents } => {
                log::info!("Updating file watcher path: {}", new_path.display());
                let _ = watcher.unwatch(&file_path);
                poll_registering_watcher(watcher, &new_path);
                file_path = new_path;
                reload_callback.file_change(contents);
            }
        }
    }

    log::warn!("File watcher channel dropped unexpectedly");
}
