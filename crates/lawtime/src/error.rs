#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("파일을 읽지 못함: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON 형식이 맞지 않음: {0}")]
    Json(#[from] serde_json::Error),
    #[error("환경변수 {0}가 없음")]
    MissingEnv(&'static str),
    #[error("등록되지 않은 노드 종류: {0}")]
    UnknownNodeKind(String),
    #[error("등록되지 않은 관계: {0}")]
    UnknownRelation(String),
    #[error("그래프가 계약에 맞지 않음: {0}")]
    Graph(String),
    #[error("판본 구간이 겹침: {0}")]
    Overlap(String),
    #[error("한 날짜에 판본이 둘 이상: {0}")]
    Ambiguous(String),
    #[error("날짜 계산이 범위를 벗어남")]
    DateRange,
    #[error("구간의 끝이 시작보다 앞섬")]
    BadPeriod,
    #[error("정규식을 만들지 못함: {0}")]
    Regex(String),
    #[error("고정 규칙표가 맞지 않음: {0}")]
    Table(String),
    #[error("법령 DB 조회 실패: {0}")]
    Db(#[from] sqlx::Error),
    #[error("법령 DB 자료가 맞지 않음: {0}")]
    Data(String),
}
