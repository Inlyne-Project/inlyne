use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use super::{Callback, Watcher};

use tempfile::TempDir;

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
            delay: Duration::from_millis(75),
            short_timeout: Duration::from_millis(25),
            long_timeout: Duration::from_millis(1_500),
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

fn init_test_env() -> (TestEnv, TempDir) {
    // Create our dummy test env
    let temp_dir = tempfile::Builder::new()
        .prefix("inlyne-tests-")
        .tempdir()
        .unwrap();
    let base = temp_dir.path();
    let main_file = base.join("main.md");
    let rel_file = base.join("rel.md");
    fs::write(&main_file, "# Main\n\n[rel](./rel.md)").unwrap();
    fs::write(&rel_file, "# Rel").unwrap();

    // Setup our watcher
    let (callback_tx, callback_rx) = mpsc::channel();
    let watcher = Watcher::spawn_inner(callback_tx, main_file.clone());

    let test_env = TestEnv {
        base_dir: temp_dir.path().to_owned(),
        main_file,
        rel_file,
        watcher,
        callback_rx,
    };

    (test_env, temp_dir)
}

struct TestEnv {
    base_dir: PathBuf,
    main_file: PathBuf,
    rel_file: PathBuf,
    watcher: Watcher,
    callback_rx: mpsc::Receiver<()>,
}

macro_rules! gen_watcher_test {
    ( $( ($test_name:ident, $test_fn:ident) ),* $(,)? ) => {
        $(
            #[test]
            fn $test_name() {
                $crate::test_utils::log::init();

                // Give the test a few chances
                let mut last_panic = None;
                let mut delays = Delays::new();
                for _ in 0..4 {
                    let result = std::panic::catch_unwind(|| {
                        let (test_env, _temp_dir) = init_test_env();

                        // Give the watcher time to get comfy :)
                        delays.delay();
                        // For some reason it looks like MacOS gets a create event even though the
                        // watcher gets registered after the file is already created. Drain any
                        // initial notifications to start
                        while test_env.callback_rx.recv_timeout(delays.short_timeout).is_ok() {}

                        $test_fn(test_env, delays.clone())
                    });
                    let Err(panic) = result else {
                        return;
                    };
                    last_panic = Some(panic);
                    delays.increase_delays();
                }

                std::panic::resume_unwind(last_panic.unwrap());
            }
        )*
    }
}

gen_watcher_test!(
    (sanity, sanity_fn),
    (update_moves_watcher, update_moves_watcher_fn),
    (slowly_swap_file, slowly_swap_file_fn),
);

fn sanity_fn(
    TestEnv {
        main_file,
        callback_rx,
        ..
    }: TestEnv,
    delays: Delays,
) {
    // Sanity check watching
    touch(&main_file);
    delays.assert_at_least_one_message(&callback_rx);
}

fn update_moves_watcher_fn(
    TestEnv {
        main_file,
        rel_file,
        watcher,
        callback_rx,
        ..
    }: TestEnv,
    delays: Delays,
) {
    // Updating a file follows the new file and not the old one
    watcher.update_file(&rel_file, fs::read_to_string(&rel_file).unwrap());
    delays.assert_at_least_one_message(&callback_rx);
    touch(&main_file);
    delays.assert_no_message(&callback_rx);
    touch(&rel_file);
    delays.assert_at_least_one_message(&callback_rx);
}

fn slowly_swap_file_fn(
    TestEnv {
        base_dir,
        callback_rx,
        main_file,
        ..
    }: TestEnv,
    delays: Delays,
) {
    let swapped_in_file = base_dir.join("swap_me_in.md");
    let swapped_out_file = base_dir.join("swap_out_to_me.md");
    fs::write(&swapped_in_file, "# Swapped").unwrap();

    // We can slowly swap out the file and it will only follow the file it's supposed to
    fs::rename(&main_file, &swapped_out_file).unwrap();
    touch(&swapped_out_file);
    delays.assert_no_message(&callback_rx);
    // The "slowly" part of this (give the watcher time to fail and start polling)
    delays.delay();
    fs::rename(&swapped_in_file, &main_file).unwrap();
    delays.assert_at_least_one_message(&callback_rx);
    fs::remove_file(&swapped_out_file).unwrap();
    delays.assert_no_message(&callback_rx);
    touch(&main_file);
    delays.assert_at_least_one_message(&callback_rx);
}
