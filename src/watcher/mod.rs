use std::{sync::mpsc::channel, time::Duration};

use crate::{Inlyne, InlyneEvent};

use notify::{
    event::{EventKind, ModifyKind},
    RecommendedWatcher, RecursiveMode, Watcher,
};

// TODO: doesn't follow watching file after file changes. Need to coordinate that
pub fn spawn_watcher(inlyne: &Inlyne) {
    // Create a channel to receive the events.
    let (watch_tx, watch_rx) = channel();

    // Create a watcher object, delivering raw events.
    // The notification back-end is selected based on the platform.
    let mut watcher = RecommendedWatcher::new(watch_tx, notify::Config::default()).unwrap();

    // Add the file path to be watched.
    let event_proxy = inlyne.event_loop.create_proxy();
    let file_path = inlyne.opts.file_path.clone();
    std::thread::spawn(move || {
        watcher
            .watch(&file_path, RecursiveMode::NonRecursive)
            .unwrap();

        loop {
            let event = match watch_rx.recv() {
                Ok(Ok(event)) => event,
                Ok(Err(err)) => {
                    log::warn!("File watcher error: {}", err);
                    continue;
                }
                Err(err) => {
                    log::warn!("File watcher channel dropped unexpectedly: {}", err);
                    break;
                }
            };

            log::debug!("File event: {:#?}", event);
            match event.kind {
                EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_)) => {
                    // Some editors may remove/rename the file as a part of saving.
                    // Reregister file watching in this case
                    log::debug!("File may have been renamed/removed. Falling back to polling");

                    let mut delay = Duration::from_millis(10);
                    loop {
                        std::thread::sleep(delay);
                        delay = Duration::from_millis(100);

                        let _ = watcher.unwatch(&file_path);
                        if watcher
                            .watch(&file_path, RecursiveMode::NonRecursive)
                            .is_ok()
                        {
                            log::debug!("Sucessfully re-registered file watcher");
                            let _ = event_proxy.send_event(InlyneEvent::FileReload);
                            break;
                        }
                    }
                }
                EventKind::Modify(_) => {
                    log::debug!("Reloading file");
                    let _ = event_proxy.send_event(InlyneEvent::FileReload);
                }
                _ => {}
            }
        }
    });
}
