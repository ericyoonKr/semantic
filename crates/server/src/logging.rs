use std::fs;
use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder, Rotation};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::error::StartupError;

const KEEP_FILES: usize = 14;

pub fn init(dir: &Path) -> Result<WorkerGuard, StartupError> {
    fs::create_dir_all(dir)?;
    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("server")
        .filename_suffix("log")
        .max_log_files(KEEP_FILES)
        .build(dir)
        .map_err(|_| StartupError::Logging)?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(writer)
                .with_ansi(false),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .try_init()
        .map_err(|_| StartupError::Logging)?;
    Ok(guard)
}
