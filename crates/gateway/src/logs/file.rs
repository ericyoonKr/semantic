use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const KST_OFFSET_SECS: i64 = 9 * 3600;

pub fn kst_today() -> i64 {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    (secs + KST_OFFSET_SECS).div_euclid(86_400)
}

pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { yoe + era * 400 + 1 } else { yoe + era * 400 }, m, d)
}

pub fn date_text(day: i64) -> String {
    let (y, m, d) = civil_from_days(day);
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn parse_date(text: &str) -> Option<i64> {
    let mut parts = text.split('-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    let d: i64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_from_civil(y, m, d))
}

#[derive(Debug)]
pub struct RollingFile {
    dir: PathBuf,
    prefix: String,
    limit: u64,
    keep_days: i64,
    day: i64,
    part: u32,
    size: u64,
    file: Option<File>,
}

impl RollingFile {
    pub fn open(dir: &Path, prefix: &str, limit: u64, keep_days: i64, today: i64) -> io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let mut f = Self { dir: dir.to_path_buf(), prefix: prefix.to_string(), limit, keep_days, day: today, part: 0, size: 0, file: None };
        f.roll_to(today)?;
        Ok(f)
    }

    pub fn path_for(&self, day: i64, part: u32) -> PathBuf {
        let date = date_text(day);
        let name = if part == 0 { format!("{}-{date}.log", self.prefix) } else { format!("{}-{date}.{part}.log", self.prefix) };
        self.dir.join(name)
    }

    pub fn write_on(&mut self, day: i64, buf: &[u8]) -> io::Result<()> {
        if day != self.day {
            self.roll_to(day)?;
        } else if self.size > 0 && self.size + buf.len() as u64 > self.limit {
            self.part += 1;
            self.open_part()?;
        }
        if let Some(file) = self.file.as_mut() {
            file.write_all(buf)?;
            self.size += buf.len() as u64;
        }
        Ok(())
    }

    fn roll_to(&mut self, day: i64) -> io::Result<()> {
        self.day = day;
        self.part = 0;
        self.purge();
        while std::fs::metadata(self.path_for(day, self.part)).map(|m| m.len()).unwrap_or(0) >= self.limit {
            self.part += 1;
        }
        self.open_part()
    }

    fn open_part(&mut self) -> io::Result<()> {
        let file = OpenOptions::new().create(true).append(true).open(self.path_for(self.day, self.part))?;
        self.size = file.metadata()?.len();
        self.file = Some(file);
        Ok(())
    }

    fn file_day(&self, name: &str) -> Option<i64> {
        let rest = name.strip_prefix(&self.prefix)?.strip_prefix('-')?;
        let date = rest.get(..10)?;
        let tail = rest.get(10..)?;
        let numbered = tail.strip_prefix('.').and_then(|t| t.strip_suffix(".log")).is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
        if tail != ".log" && !numbered {
            return None;
        }
        parse_date(date)
    }

    fn purge(&self) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if self.file_day(&name).is_some_and(|d| d + self.keep_days <= self.day) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_round_trip() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(date_text(days_from_civil(2026, 11, 8)), "2026-11-08");
        assert_eq!(date_text(days_from_civil(2024, 2, 29)), "2024-02-29");
        assert_eq!(parse_date("2026-11-08"), Some(days_from_civil(2026, 11, 8)));
        assert_eq!(parse_date("2026-13-01"), None);
    }

    #[test]
    fn rolls_by_size_and_day_and_purges_old_files() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let today = days_from_civil(2026, 11, 22);
        let old = dir.path().join("gateway-2026-11-08.log");
        let old_part = dir.path().join("gateway-2026-11-08.3.log");
        let kept = dir.path().join("gateway-2026-11-09.log");
        let other = dir.path().join("server-2026-01-01.log");
        for p in [&old, &old_part, &kept, &other] {
            std::fs::write(p, b"x")?;
        }
        let mut f = RollingFile::open(dir.path(), "gateway", 10, 14, today)?;
        assert!(!old.exists() && !old_part.exists());
        assert!(kept.exists() && other.exists());
        f.write_on(today, b"0123456")?;
        f.write_on(today, b"789")?;
        f.write_on(today, b"abc")?;
        assert_eq!(std::fs::read(f.path_for(today, 0))?, b"0123456789");
        assert_eq!(std::fs::read(f.path_for(today, 1))?, b"abc");
        f.write_on(today + 1, b"next")?;
        assert_eq!(std::fs::read(f.path_for(today + 1, 0))?, b"next");
        assert!(!kept.exists());
        drop(f);
        let again = RollingFile::open(dir.path(), "gateway", 10, 14, today)?;
        assert_eq!(again.part, 1);
        Ok(())
    }
}
