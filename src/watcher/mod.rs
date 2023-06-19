use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};

use crate::InlyneEvent;

use notify::{
    event::{EventKind, ModifyKind},
    Event, EventHandler, RecommendedWatcher, RecursiveMode, Watcher as _,
};
use winit::event_loop::EventLoopProxy;

#[cfg(test)]
mod tests;

trait Callback: Send + 'static {
    fn update(&self);
}

impl Callback for EventLoopProxy<InlyneEvent> {
    fn update(&self) {
        let _ = self.send_event(InlyneEvent::FileReload);
    }
}

enum WatcherMsg {
    // Sent by the registered file watcher
    Notify(notify::Result<Event>),
    // Sent by the event loop
    FileChange(PathBuf),
}

struct MsgHandler(mpsc::Sender<WatcherMsg>);

impl EventHandler for MsgHandler {
    fn handle_event(&mut self, event: notify::Result<Event>) {
        let msg = WatcherMsg::Notify(event);
        let _ = self.0.send(msg);
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
            RecommendedWatcher::new(MsgHandler(msg_tx), notify::Config::default()).unwrap();

        std::thread::spawn(move || {
            endlessly_handle_messages(notify_watcher, msg_rx, reload_callback, file_path);
        });

        watcher
    }

    pub fn update_path(&self, new_path: &Path) {
        let msg = WatcherMsg::FileChange(new_path.to_owned());
        let _ = self.0.send(msg);
    }
}

fn endlessly_handle_messages<C: Callback>(
    mut watcher: RecommendedWatcher,
    msg_rx: mpsc::Receiver<WatcherMsg>,
    reload_callback: C,
    mut file_path: PathBuf,
) {
    watcher
        .watch(&file_path, RecursiveMode::NonRecursive)
        .unwrap();

    let poll_registering_watcher = |watcher: &mut RecommendedWatcher, file_path: &Path| {
        let mut delay = Duration::from_millis(10);

        loop {
            std::thread::sleep(delay);
            delay = Duration::from_millis(100);

            let _ = watcher.unwatch(file_path);
            if watcher
                .watch(file_path, RecursiveMode::NonRecursive)
                .is_ok()
            {
                break;
            }
        }
    };

    while let Ok(msg) = msg_rx.recv() {
        match msg {
            WatcherMsg::Notify(Ok(event)) => {
                log::debug!("File event: {:#?}", event);

                if matches!(
                    event.kind,
                    EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_))
                ) {
                    log::debug!("File may have been renamed/removed. Falling back to polling");
                    poll_registering_watcher(&mut watcher, &file_path);
                    log::debug!("Sucessfully re-registered file watcher");
                    reload_callback.update();
                } else if matches!(event.kind, EventKind::Modify(_)) {
                    log::debug!("Reloading file");
                    reload_callback.update();
                }
            }
            WatcherMsg::Notify(Err(err)) => log::warn!("File watcher error: {}", err),
            WatcherMsg::FileChange(new_path) => {
                log::info!("Updating file watcher path: {}", new_path.display());
                let _ = watcher.unwatch(&file_path);
                poll_registering_watcher(&mut watcher, &new_path);
                file_path = new_path;
                reload_callback.update();
            }
        }
    }

    log::warn!("File watcher channel dropped unexpectedly");
}
