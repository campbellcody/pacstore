use log::{LevelFilter, Log, Metadata, Record};

struct StderrLogger;
impl Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.target().starts_with("pacstore") && metadata.level() <= log::max_level()
    }
    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!("{} {}: {}", record.level(), record.target(), record.args());
        }
    }
    fn flush(&self) {}
}

pub fn init() {
    static LOGGER: StderrLogger = StderrLogger;
    let level = std::env::var("PACSTORE_LOG")
        .ok()
        .and_then(|value| value.parse::<LevelFilter>().ok())
        .unwrap_or(LevelFilter::Info);
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(level);
    }
}
