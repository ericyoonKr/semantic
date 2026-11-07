use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseError {
    #[error("지원하지 않는 형식입니다: {0}")]
    Unsupported(String),
    #[error("파일이 손상되었습니다")]
    Corrupted,
    #[error("암호가 걸린 파일입니다")]
    Encrypted,
    #[error("추출된 글자가 없습니다")]
    EmptyText,
    #[error("크기 상한을 넘었습니다")]
    TooLarge,
    #[error("압축 해제 상한을 넘었습니다")]
    ZipBomb,
    #[error("파싱 시간 제한을 넘었습니다")]
    Timeout,
}
