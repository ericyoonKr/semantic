pub mod file;
pub mod mask;

use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use tracing_subscriber::fmt::MakeWriter;

use crate::logs::file::{RollingFile, kst_today};
use crate::logs::mask::mask;

pub const LOG_PREFIX: &str = "gateway";
pub const FILE_LIMIT_BYTES: u64 = 100 * 1024 * 1024;
pub const KEEP_DAYS: i64 = 14;

#[derive(Clone, Default)]
pub struct LogSink {
    file: Option<Arc<Mutex<RollingFile>>>,
}

pub struct SinkWriter {
    file: Option<Arc<Mutex<RollingFile>>>,
}

impl LogSink {
    pub fn open(data_dir: Option<&Path>) -> (Self, Option<io::Error>) {
        let Some(dir) = data_dir else {
            return (Self::default(), None);
        };
        match RollingFile::open(&dir.join("logs"), LOG_PREFIX, FILE_LIMIT_BYTES, KEEP_DAYS, kst_today()) {
            Ok(f) => (Self { file: Some(Arc::new(Mutex::new(f))) }, None),
            Err(e) => (Self::default(), Some(e)),
        }
    }
}

impl<'a> MakeWriter<'a> for LogSink {
    type Writer = SinkWriter;

    fn make_writer(&'a self) -> Self::Writer {
        SinkWriter { file: self.file.clone() }
    }
}

impl Write for SinkWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let text = mask(&String::from_utf8_lossy(buf));
        let _ = io::stderr().write_all(text.as_bytes());
        if let Some(Ok(mut f)) = self.file.as_ref().map(|file| file.lock()) {
            let _ = f.write_on(kst_today(), text.as_bytes());
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        io::stderr().flush()
    }
}

pub fn init(data_dir: Option<&Path>) {
    let (sink, err) = LogSink::open(data_dir);
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_ansi(false)
        .with_writer(sink)
        .init();
    if let Some(e) = err {
        tracing::warn!(kind = ?e.kind(), "logs.file_open_failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sink_masks_before_writing_file() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let (sink, err) = LogSink::open(Some(dir.path()));
        assert!(err.is_none());
        let mut w = sink.make_writer();
        w.write_all(b"user phone=010-1234-5678 done\n")?;
        let path = dir.path().join("logs").join(format!("{LOG_PREFIX}-{}.log", file::date_text(kst_today())));
        let text = std::fs::read_to_string(path)?;
        assert_eq!(text, "user phone=[가림] done\n");
        Ok(())
    }
}
