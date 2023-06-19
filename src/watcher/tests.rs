use std::{fs, path::Path, sync::mpsc, time::Duration};

use super::{Callback, Watcher};

impl Callback for mpsc::Sender<()> {
    fn update(&self) {
        self.send(()).unwrap();
    }
}

const SHORT_DURATION: Duration = Duration::from_millis(50);
const LONG_DURATION: Duration = Duration::from_millis(200);

fn short_sleep() {
    std::thread::sleep(SHORT_DURATION);
}

fn long_sleep() {
    std::thread::sleep(LONG_DURATION);
}

fn touch(file: &Path) {
    let now = filetime::FileTime::now();
    filetime::set_file_mtime(file, now).unwrap();
}

macro_rules! assert_no_message {
    ($callback:ident) => {
        assert!($callback.recv_timeout(SHORT_DURATION).is_err());
    };
}

macro_rules! assert_at_least_one_message {
    ($callback:ident) => {
        assert!($callback.recv_timeout(LONG_DURATION).is_ok());
        while $callback.try_recv().is_ok() {}
    };
}

// Unfortunately this needs to be littered with sleeps to work :/
#[test]
fn the_gauntlet() {
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
    short_sleep();

    // Sanity check watching
    touch(&main_file);
    assert_at_least_one_message!(callback_rx);

    // Updating a file follows the new file and not the old one
    watcher.update_path(&rel_file);
    assert_at_least_one_message!(callback_rx);
    touch(&main_file);
    assert_no_message!(callback_rx);
    touch(&rel_file);
    assert_at_least_one_message!(callback_rx);

    // We can slowly swap out the file and it will only follow the file it's supposed to
    fs::rename(&rel_file, &swapped_out_file).unwrap();
    touch(&swapped_out_file);
    assert_no_message!(callback_rx);
    // The "slowly" part of this (give the watcher time to fail and start polling)
    long_sleep();
    fs::rename(&swapped_in_file, &rel_file).unwrap();
    assert_at_least_one_message!(callback_rx);
    fs::remove_file(&swapped_out_file).unwrap();
    assert_no_message!(callback_rx);
    touch(&rel_file);
    assert_at_least_one_message!(callback_rx);
}
