//! Kernel logger implementation backed by the `log` crate.
//! Directs all diagnostics and kernel events to the serial console.

use log::{Level, LevelFilter, Metadata, Record};

struct KernelLogger;

static LOGGER: KernelLogger = KernelLogger;

/// Initializes the global kernel logger.
pub fn init() {
    log::set_logger(&LOGGER)
        .map(|()| log::set_max_level(LevelFilter::Trace))
        .expect("Failed to initialize kernel logger");
}

impl log::Log for KernelLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let level_tag = match record.level() {
                Level::Error => "\x1b[1;31m[ERROR]\x1b[0m",
                Level::Warn => "\x1b[1;33m[WARN ]\x1b[0m",
                Level::Info => "\x1b[1;36m[INFO ]\x1b[0m",
                Level::Debug => "\x1b[1;34m[DEBUG]\x1b[0m",
                Level::Trace => "\x1b[1;30m[TRACE]\x1b[0m",
            };
            crate::println!("{} [{}] {}", level_tag, record.target(), record.args());
        }
    }

    fn flush(&self) {}
}
