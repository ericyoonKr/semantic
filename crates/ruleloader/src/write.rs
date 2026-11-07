use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::LoaderError;

pub fn ensure_outside_repo(out: &Path) -> Result<PathBuf, LoaderError> {
    let abs = std::path::absolute(out)?;
    if abs.ancestors().any(|a| a.join(".git").exists()) {
        return Err(LoaderError::OutputInRepo(abs));
    }
    Ok(abs)
}

pub fn replace_dir(out: &Path, files: &[(String, Vec<u8>)]) -> Result<(), LoaderError> {
    let (Some(parent), Some(name)) = (out.parent(), out.file_name()) else {
        return Err(LoaderError::BadOutput(out.to_path_buf()));
    };
    let name = name.to_string_lossy();
    fs::create_dir_all(parent)?;
    let pid = std::process::id();
    let tmp = parent.join(format!(".{name}.tmp-{pid}"));
    let old = parent.join(format!(".{name}.old-{pid}"));
    if tmp.exists() {
        fs::remove_dir_all(&tmp)?;
    }
    fs::create_dir(&tmp)?;
    if let Err(e) = fill(&tmp, files) {
        let _ = fs::remove_dir_all(&tmp);
        return Err(e);
    }
    if !out.exists() {
        return fs::rename(&tmp, out).map_err(|e| cleanup(&tmp, e));
    }
    fs::rename(out, &old).map_err(|e| cleanup(&tmp, e))?;
    if let Err(e) = fs::rename(&tmp, out) {
        let _ = fs::rename(&old, out);
        return Err(cleanup(&tmp, e));
    }
    fs::remove_dir_all(&old)?;
    Ok(())
}

fn fill(dir: &Path, files: &[(String, Vec<u8>)]) -> Result<(), LoaderError> {
    for (name, body) in files {
        let mut f = fs::File::create(dir.join(name))?;
        f.write_all(body)?;
        f.sync_all()?;
    }
    Ok(())
}

fn cleanup(tmp: &Path, e: std::io::Error) -> LoaderError {
    let _ = fs::remove_dir_all(tmp);
    LoaderError::Io(e)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{ensure_outside_repo, replace_dir};

    #[test]
    fn replace_swaps_whole_folder_and_leaves_no_temp() -> Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let out = root.path().join("bundle");
        replace_dir(&out, &[("a.jsonl".into(), b"one".to_vec())])?;
        assert_eq!(fs::read(out.join("a.jsonl"))?, b"one");
        replace_dir(&out, &[("b.jsonl".into(), b"two".to_vec())])?;
        assert!(!out.join("a.jsonl").exists());
        assert_eq!(fs::read(out.join("b.jsonl"))?, b"two");
        let names: Vec<_> = fs::read_dir(root.path())?
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["bundle".to_string()]);
        Ok(())
    }

    #[test]
    fn output_inside_git_tree_is_refused() -> Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let plain = root.path().join("plain").join("out");
        assert!(ensure_outside_repo(&plain).is_ok());
        let repo = root.path().join("repo");
        fs::create_dir_all(repo.join(".git"))?;
        assert!(ensure_outside_repo(&repo.join("deep").join("out")).is_err());
        assert!(ensure_outside_repo(&repo).is_err());
        Ok(())
    }
}
