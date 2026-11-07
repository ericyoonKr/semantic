use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::emit;
use crate::error::LoaderError;
use crate::report::render;
use crate::source::{self, RuleFile};
use crate::validate::validate;
use crate::write::{ensure_outside_repo, replace_dir};

const CONSUMER_ENV: &str = "KG_CONSUMER_SCHEMA";
const MANIFEST_ENV: &str = "KG_MANIFEST_SCHEMA";

#[derive(Parser)]
#[command(name = "ruleloader")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Check {
        yaml: PathBuf,
    },
    Build {
        dir: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        schema: Option<String>,
        #[arg(long)]
        manifest_schema: Option<String>,
    },
}

pub fn run() -> ExitCode {
    let result = match Cli::parse().cmd {
        Cmd::Check { yaml } => check(&yaml),
        Cmd::Build {
            dir,
            out,
            schema,
            manifest_schema,
        } => build(&dir, &out, schema, manifest_schema),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("오류: {e}");
            ExitCode::FAILURE
        }
    }
}

fn check(path: &Path) -> Result<bool, LoaderError> {
    let file = source::read(path)?;
    let report = validate(&file);
    println!("파일: {}", path.display());
    print!("{}", render(&file, &report));
    Ok(report.check_ok())
}

fn build(
    dir: &Path,
    out: &Path,
    schema: Option<String>,
    manifest_schema: Option<String>,
) -> Result<bool, LoaderError> {
    let schema = setting(schema, CONSUMER_ENV)?;
    let manifest_schema = setting(manifest_schema, MANIFEST_ENV)?;
    let out = ensure_outside_repo(out)?;
    let files = load_all(dir)?;
    let mut ok = true;
    let mut seen = HashSet::new();
    for (path, file) in &files {
        let report = validate(file);
        println!("파일: {}", path.display());
        print!("{}", render(file, &report));
        ok &= report.build_ok();
        if !seen.insert(file.claim_type.clone()) {
            return Err(LoaderError::DuplicateClaim(file.claim_type.clone()));
        }
    }
    if !ok {
        return Err(LoaderError::Rejected);
    }
    let graphs = files
        .iter()
        .map(|(_, f)| emit::graph(f, &schema))
        .collect::<Result<Vec<_>, _>>()?;
    let manifest = emit::manifest(&graphs, &manifest_schema)?;
    let count = graphs.len();
    let mut outputs: Vec<(String, Vec<u8>)> =
        graphs.into_iter().map(|g| (g.file, g.body)).collect();
    outputs.push(("manifest.json".into(), manifest));
    replace_dir(&out, &outputs)?;
    println!("그래프 {count}개를 썼다: {}", out.display());
    println!("서버를 다시 띄워야 새 요건 트리를 읽는다.");
    Ok(true)
}

fn setting(arg: Option<String>, env: &'static str) -> Result<String, LoaderError> {
    arg.or_else(|| std::env::var(env).ok())
        .filter(|v| !v.trim().is_empty())
        .ok_or(LoaderError::MissingSetting(env))
}

fn load_all(dir: &Path) -> Result<Vec<(PathBuf, RuleFile)>, LoaderError> {
    let entries = fs::read_dir(dir).map_err(|source| LoaderError::Read {
        path: dir.to_path_buf(),
        source,
    })?;
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter(|p| {
            p.extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("yaml") || x.eq_ignore_ascii_case("yml"))
        })
        .collect();
    paths.sort();
    if paths.is_empty() {
        return Err(LoaderError::NoInput(dir.to_path_buf()));
    }
    paths
        .into_iter()
        .map(|p| source::read(&p).map(|f| (p, f)))
        .collect()
}
