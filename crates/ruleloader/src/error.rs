use std::io;
use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum LoaderError {
    #[error("파일을 읽지 못했다: {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("YAML 형식이 맞지 않는다: {path}: {source}")]
    Yaml {
        path: PathBuf,
        source: serde_yaml::Error,
    },
    #[error("쓰기에 실패했다: {0}")]
    Io(#[from] io::Error),
    #[error("JSON으로 바꾸지 못했다: {0}")]
    Json(#[from] serde_json::Error),
    #[error("설정값이 없다: {0}")]
    MissingSetting(&'static str),
    #[error("출력 경로가 git 저장소 안에 있다: {0}")]
    OutputInRepo(PathBuf),
    #[error("출력 경로에 폴더 이름이 없다: {0}")]
    BadOutput(PathBuf),
    #[error("청구 유형이 두 파일에 겹친다: {0}")]
    DuplicateClaim(String),
    #[error("청구 유형을 파일 이름으로 쓸 수 없다: {0}")]
    BadClaimName(String),
    #[error("읽을 YAML 파일이 없다: {0}")]
    NoInput(PathBuf),
    #[error("검증에 실패한 파일이 있어 아무것도 쓰지 않았다")]
    Rejected,
}
