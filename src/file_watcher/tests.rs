use std::fs;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use super::{Callback, Watcher};
use crate::test_utils::init_test_log;

impl Callback for mpsc::Sender<()> {
    fn file_reload(&self) {
        self.send(()).unwrap();
    }

    fn file_change(&self, _: String) {
        self.send(()).unwrap();
    }
}

fn touch(file: &Path) {
    let now = filetime::FileTime::now();
    filetime::set_file_mtime(file, now).unwrap();
}

#[derive(Clone)]
struct Delays {
    delay: Duration,
    short_timeout: Duration,
    long_timeout: Duration,
}

impl Delays {
    fn new() -> Self {
        Self {
            delay: Duration::from_millis(100),
            short_timeout: Duration::from_millis(50),
            long_timeout: Duration::from_millis(2_000),
        }
    }

    fn increase_delays(&mut self) {
        self.delay *= 2;
        self.short_timeout *= 2;
        self.long_timeout += Duration::from_secs(1);
    }

    fn delay(&self) {
        std::thread::sleep(self.delay);
    }

    #[track_caller]
    fn assert_no_message(&self, callback: &mpsc::Receiver<()>) {
        assert!(callback.recv_timeout(self.short_timeout).is_err());
    }

    #[track_caller]
    fn assert_at_least_one_message(&self, callback: &mpsc::Receiver<()>) {
        assert!(callback.recv_timeout(self.long_timeout).is_ok());
        while callback.recv_timeout(self.short_timeout).is_ok() {}
    }
}

#[test]
fn the_gauntlet() {
    init_test_log();

    // This test can be flaky, so give it a few chances to succeed
    let mut last_panic = None;
    let mut delays = Delays::new();
    for _ in 0..3 {
        let result = std::panic::catch_unwind(|| the_gauntlet_flaky(delays.clone()));
        let Err(panic) = result else {
            return;
        };
        last_panic = Some(panic);
        delays.increase_delays();
    }

    std::panic::resume_unwind(last_panic.unwrap());
}

// Unfortunately this needs to be littered with sleeps/timeouts to work right :/
fn the_gauntlet_flaky(delays: Delays) {
    // Create our dummy test env
    let temp_dir = tempfile::Builder::new()
        .prefix("inlyne-tests-")
        .tempdir()
        .unwrap();
    let base = temp_dir.path();
    let main_file = base.join("main.md");
    let rel_file = base.join("rel.md");
    let swapped_in_file = base.join("swap_me_in.md");
    let swapped_out_file = base.join("swap_out_to_me.md");
    fs::write(&main_file, "# Main\n\n[rel](./rel.md)").unwrap();
    fs::write(&rel_file, "# Rel").unwrap();
    fs::write(&swapped_in_file, "# Swapped").unwrap();

    // Setup our watcher
    let (callback_tx, callback_rx) = mpsc::channel::<()>();
    let watcher = Watcher::spawn_inner(callback_tx, main_file.clone());

    // Give the watcher time to get comfy :)
    delays.delay();

    // Sanity check watching
    touch(&main_file);
    delays.assert_at_least_one_message(&callback_rx);

    // Updating a file follows the new file and not the old one
    watcher.update_file(&rel_file, fs::read_to_string(&rel_file).unwrap());
    delays.assert_at_least_one_message(&callback_rx);
    touch(&main_file);
    delays.assert_no_message(&callback_rx);
    touch(&rel_file);
    delays.assert_at_least_one_message(&callback_rx);

    // We can slowly swap out the file and it will only follow the file it's supposed to
    fs::rename(&rel_file, &swapped_out_file).unwrap();
    touch(&swapped_out_file);
    delays.assert_no_message(&callback_rx);
    // The "slowly" part of this (give the watcher time to fail and start polling)
    delays.delay();
    fs::rename(&swapped_in_file, &rel_file).unwrap();
    delays.assert_at_least_one_message(&callback_rx);
    fs::remove_file(&swapped_out_file).unwrap();
    delays.assert_no_message(&callback_rx);
    touch(&rel_file);
    delays.assert_at_least_one_message(&callback_rx);
}
