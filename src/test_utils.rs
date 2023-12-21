use log::LevelFilter;

pub fn init_test_log() {
    // Ignore errors because other tests in the same binary may have already initialized the logger
    let _ = env_logger::Builder::new()
        .filter(Some("inlyne"), LevelFilter::Trace)
        .filter(None, LevelFilter::Warn)
        .is_test(true)
        .try_init();
}
