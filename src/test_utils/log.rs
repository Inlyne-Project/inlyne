use tracing_subscriber::prelude::*;

pub fn init() {
    let filter = tracing_subscriber::filter::Targets::new()
        .with_default(tracing_subscriber::filter::LevelFilter::WARN)
        .with_target("inlyne", tracing_subscriber::filter::LevelFilter::TRACE);
    // Ignore errors because other tests in the same binary may have already initialized the logger
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .compact()
                .with_test_writer(),
        )
        .try_init();
}
