use std::time::Duration;

use parsers::GuardLimits;

use super::Reader;
use crate::error::ConfigError;

const MIB: u64 = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Limits {
    pub guard: GuardLimits,
    pub json_body_bytes: usize,
    pub upload_body_bytes: usize,
    pub upload_concurrency: usize,
    pub session_channel: usize,
    pub session_idle: Duration,
    pub upload_deadline: Duration,
    pub kg_deadline: Duration,
    pub shutdown_grace: Duration,
    pub outline_level: u8,
    pub outline_lines: usize,
    pub filename_chars: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct Threads {
    pub tokio_workers: usize,
    pub tokio_blocking: usize,
    pub rayon: usize,
}

pub(super) fn read_limits<F: Fn(&str) -> Option<String>>(
    r: &Reader<F>,
) -> Result<Limits, ConfigError> {
    let base = GuardLimits::default();
    let parse_secs: u64 = r.positive("PARSE_TIME_LIMIT_SECS", 120)?;
    let guard = GuardLimits {
        max_file_bytes: r.positive("UPLOAD_MAX_BYTES", base.max_file_bytes)?,
        max_pages: r.positive("UPLOAD_MAX_PAGES", base.max_pages)?,
        max_chars: r.positive("UPLOAD_MAX_CHARS", base.max_chars)?,
        max_member_bytes: r.positive("ZIP_MEMBER_MAX_BYTES", base.max_member_bytes)?,
        max_total_bytes: r.positive("ZIP_TOTAL_MAX_BYTES", base.max_total_bytes)?,
        max_ratio: r.positive("ZIP_MAX_RATIO", base.max_ratio)?,
        time_limit: Some(Duration::from_secs(parse_secs)),
    };
    let upload_body_bytes = usize::try_from(guard.max_file_bytes.saturating_add(MIB))
        .map_err(|_| ConfigError::Invalid("UPLOAD_MAX_BYTES"))?;
    let limits = Limits {
        guard,
        json_body_bytes: r.positive("JSON_BODY_MAX_BYTES", 64 * 1024)?,
        upload_body_bytes,
        upload_concurrency: r.positive("UPLOAD_CONCURRENCY", 2)?,
        session_channel: r.positive("SESSION_CHANNEL", 16)?,
        session_idle: r.secs("SESSION_IDLE_SECS", 600)?,
        upload_deadline: r.secs("UPLOAD_DEADLINE_SECS", 170)?,
        kg_deadline: r.secs("KG_DEADLINE_SECS", 20)?,
        shutdown_grace: r.secs("SHUTDOWN_GRACE_SECS", 30)?,
        outline_level: r.positive("OUTLINE_MAX_LEVEL", 2)?,
        outline_lines: r.positive("OUTLINE_MAX_LINES", 60)?,
        filename_chars: r.positive("FILENAME_MAX_CHARS", crate::upload::filename::MAX_CHARS)?,
    };
    if Duration::from_secs(parse_secs) >= limits.upload_deadline {
        return Err(ConfigError::DeadlineOrder(
            "PARSE_TIME_LIMIT_SECS",
            "UPLOAD_DEADLINE_SECS",
        ));
    }
    Ok(limits)
}
