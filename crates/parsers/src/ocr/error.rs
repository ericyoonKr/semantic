use std::fmt::{self, Display};
use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Detection,
    Recognition,
}

impl Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Stage::Detection => "글자 영역 검출",
            Stage::Recognition => "글자 인식",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OcrError {
    #[error("글자 인식 모델 파일을 읽지 못했습니다: {path}: {reason}")]
    ModelFile { path: PathBuf, reason: String },
    #[error("ONNX Runtime 환경을 만들지 못했습니다: {0}")]
    Environment(String),
    #[error("{stage} 모델 세션을 만들지 못했습니다: {path}: {reason}")]
    Session { stage: Stage, path: PathBuf, reason: String },
    #[error("쪽을 그림으로 그리지 못했습니다: {0}")]
    Render(String),
    #[error("{stage} 모델 입력을 만들지 못했습니다: {reason}")]
    Input { stage: Stage, reason: String },
    #[error("{stage} 모델 실행이 실패했습니다: {reason}")]
    Inference { stage: Stage, reason: String },
    #[error("{stage} 모델 출력 모양이 예상과 다릅니다: {shape:?}")]
    OutputShape { stage: Stage, shape: Vec<i64> },
    #[error("{stage} 모델 세션 잠금이 깨졌습니다")]
    Poisoned { stage: Stage },
}

pub(crate) fn model_file(path: &Path) -> impl FnOnce(std::io::Error) -> OcrError + '_ {
    move |e| OcrError::ModelFile { path: path.to_path_buf(), reason: e.to_string() }
}

pub(crate) fn environment(e: impl Display) -> OcrError {
    OcrError::Environment(e.to_string())
}

pub(crate) fn session<E: Display>(stage: Stage, path: &Path) -> impl FnOnce(E) -> OcrError + '_ {
    move |e| OcrError::Session { stage, path: path.to_path_buf(), reason: e.to_string() }
}

pub(crate) fn render(e: impl Display) -> OcrError {
    OcrError::Render(e.to_string())
}

pub(crate) fn input<E: Display>(stage: Stage) -> impl FnOnce(E) -> OcrError {
    move |e| OcrError::Input { stage, reason: e.to_string() }
}

pub(crate) fn inference<E: Display>(stage: Stage) -> impl FnOnce(E) -> OcrError {
    move |e| OcrError::Inference { stage, reason: e.to_string() }
}

pub(crate) fn output_shape(stage: Stage, shape: &[i64]) -> OcrError {
    OcrError::OutputShape { stage, shape: shape.to_vec() }
}

pub(crate) fn poisoned<E>(stage: Stage) -> impl FnOnce(E) -> OcrError {
    move |_| OcrError::Poisoned { stage }
}

pub fn log_load_failure(error: &OcrError, dir: &Path) {
    tracing::error!(dir = %dir.display(), %error, "text recognition models could not be loaded; pages without text stay unextracted");
}

pub fn log_page_failure(error: &OcrError, page: u32) {
    tracing::warn!(page, %error, "text recognition failed on a page; the page stays unextracted");
}

pub fn log_page_unread(page: u32, lines: usize) {
    tracing::info!(page, lines, "text recognition found no usable text on a page; the page stays unextracted");
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{OcrError, Stage, model_file, output_shape, session};

    #[test]
    fn messages_name_the_stage_the_file_and_the_cause() {
        let e = model_file(Path::new("/m/dict.txt"))(std::io::Error::other("없음"));
        assert_eq!(e.to_string(), "글자 인식 모델 파일을 읽지 못했습니다: /m/dict.txt: 없음");
        let e: OcrError = session::<&str>(Stage::Detection, Path::new("/m/det.onnx"))("bad model");
        assert_eq!(e.to_string(), "글자 영역 검출 모델 세션을 만들지 못했습니다: /m/det.onnx: bad model");
        let e = output_shape(Stage::Recognition, &[1, 2]);
        assert_eq!(e.to_string(), "글자 인식 모델 출력 모양이 예상과 다릅니다: [1, 2]");
    }
}
