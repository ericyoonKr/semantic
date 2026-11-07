use std::fs::File;
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::error::KgError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureStatus {
    Unchecked,
    Verified,
    Rejected,
}

impl SignatureStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SignatureStatus::Unchecked => "unchecked",
            SignatureStatus::Verified => "verified",
            SignatureStatus::Rejected => "rejected",
        }
    }
}

pub trait SignatureVerifier: Send + Sync {
    fn verify(&self, manifest: &[u8], signature: Option<&[u8]>) -> SignatureStatus;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UncheckedSignature;

impl SignatureVerifier for UncheckedSignature {
    fn verify(&self, _manifest: &[u8], _signature: Option<&[u8]>) -> SignatureStatus {
        SignatureStatus::Unchecked
    }
}

pub(crate) struct HashingReader {
    file: File,
    hasher: Sha256,
}

impl HashingReader {
    pub(crate) fn open(path: &Path) -> Result<Self, KgError> {
        Ok(Self {
            file: File::open(path)?,
            hasher: Sha256::new(),
        })
    }

    pub(crate) fn finish_hex(self) -> String {
        self.hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
}

impl Read for HashingReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.file.read(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }
}

pub(crate) fn file_sha256(path: &Path) -> Result<String, KgError> {
    let mut reader = HashingReader::open(path)?;
    std::io::copy(&mut reader, &mut std::io::sink())?;
    Ok(reader.finish_hex())
}

pub(crate) fn same_hash(a: &str, b: &str) -> bool {
    !a.is_empty() && a.eq_ignore_ascii_case(b)
}
