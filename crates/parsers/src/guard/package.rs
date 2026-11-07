use std::io::{self, BufRead, BufReader, Cursor, Read};
use std::path::Component;

use zip::read::ZipFile;
use zip::result::ZipError;
use zip::ZipArchive;

use super::GuardLimits;
use crate::error::ParseError;

pub(crate) struct Package<'a> {
    archive: ZipArchive<Cursor<&'a [u8]>>,
    limits: &'a GuardLimits,
    read_total: u64,
}

impl<'a> Package<'a> {
    pub(crate) fn open(bytes: &'a [u8], limits: &'a GuardLimits) -> Result<Self, ParseError> {
        let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(zip_error)?;
        let mut declared_total: u64 = 0;
        for i in 0..archive.len() {
            let entry = archive.by_index_raw(i).map_err(zip_error)?;
            if !safe_name(&entry) {
                return Err(ParseError::Corrupted);
            }
            if entry.encrypted() {
                return Err(ParseError::Encrypted);
            }
            let size = entry.size();
            declared_total = declared_total.saturating_add(size);
            if size > limits.max_member_bytes
                || declared_total > limits.max_total_bytes
                || size > entry.compressed_size().saturating_mul(limits.max_ratio)
            {
                return Err(ParseError::ZipBomb);
            }
        }
        Ok(Self { archive, limits, read_total: 0 })
    }

    pub(crate) fn names(&self) -> Vec<String> {
        self.archive.file_names().map(str::to_string).collect()
    }

    pub(crate) fn member(&mut self, name: &str) -> Result<Option<Member<'_>>, ParseError> {
        let Some(index) = self.archive.index_for_name(name) else {
            return Ok(None);
        };
        let file = self.archive.by_index(index).map_err(zip_error)?;
        Ok(Some(Member {
            compressed: file.compressed_size(),
            inner: BufReader::new(file),
            read: 0,
            total: &mut self.read_total,
            limits: self.limits,
            violation: None,
        }))
    }
}

fn safe_name(entry: &ZipFile<'_>) -> bool {
    let name = entry.name();
    if name.contains('\\') || name.contains('\0') {
        return false;
    }
    match entry.enclosed_name() {
        Some(path) => path.components().all(|c| matches!(c, Component::Normal(_))),
        None => false,
    }
}

fn zip_error(e: ZipError) -> ParseError {
    match e {
        ZipError::UnsupportedArchive(msg) if msg == ZipError::PASSWORD_REQUIRED => {
            ParseError::Encrypted
        }
        ZipError::InvalidPassword => ParseError::Encrypted,
        _ => ParseError::Corrupted,
    }
}

pub(crate) struct Member<'p> {
    inner: BufReader<ZipFile<'p>>,
    compressed: u64,
    read: u64,
    total: &'p mut u64,
    limits: &'p GuardLimits,
    violation: Option<ParseError>,
}

impl Member<'_> {
    pub(crate) fn violation(&self) -> Option<ParseError> {
        self.violation.clone()
    }

    fn account(&mut self, n: u64) {
        self.read = self.read.saturating_add(n);
        *self.total = self.total.saturating_add(n);
        if self.read > self.limits.max_member_bytes
            || *self.total > self.limits.max_total_bytes
            || self.read > self.compressed.saturating_mul(self.limits.max_ratio)
        {
            self.violation = Some(ParseError::ZipBomb);
        }
    }
}

impl Read for Member<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let available = self.fill_buf()?;
        let n = available.len().min(buf.len());
        buf[..n].copy_from_slice(&available[..n]);
        self.consume(n);
        Ok(n)
    }
}

impl BufRead for Member<'_> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.violation.is_some() {
            return Err(io::Error::other("limit"));
        }
        self.inner.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.inner.consume(amt);
        self.account(amt as u64);
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use zip::CompressionMethod;

    use super::*;
    use crate::testzip::build_zip_with;

    const DECLARED: u32 = 10;

    fn patch_sizes(bytes: &mut [u8]) {
        let mut i = 0;
        while i + 4 <= bytes.len() {
            match &bytes[i..i + 4] {
                b"PK\x03\x04" => bytes[i + 22..i + 26].copy_from_slice(&DECLARED.to_le_bytes()),
                b"PK\x01\x02" => bytes[i + 24..i + 28].copy_from_slice(&DECLARED.to_le_bytes()),
                _ => {}
            }
            i += 1;
        }
    }

    fn small_limits() -> GuardLimits {
        GuardLimits { max_member_bytes: 1000, ..GuardLimits::default() }
    }

    #[test]
    fn declared_size_over_member_limit_is_rejected_at_open() {
        let data = vec![b'x'; 5000];
        let bytes = build_zip_with(&[("a.xml", &data)], CompressionMethod::Stored);
        let limits = small_limits();
        assert_eq!(Package::open(&bytes, &limits).err(), Some(ParseError::ZipBomb));
    }

    #[test]
    fn understated_size_is_caught_while_streaming() {
        let data = vec![b'x'; 5000];
        let mut bytes = build_zip_with(&[("a.xml", &data)], CompressionMethod::Stored);
        patch_sizes(&mut bytes);
        let limits = small_limits();
        let mut package = Package::open(&bytes, &limits).expect("open");
        let mut member = package.member("a.xml").expect("member").expect("present");
        let mut sink = Vec::new();
        let result = member.read_to_end(&mut sink);
        assert!(result.is_err(), "read {} bytes", sink.len());
        assert_eq!(member.violation(), Some(ParseError::ZipBomb));
    }
}
