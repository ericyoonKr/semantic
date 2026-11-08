use serde::Serialize;

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum NodeKind {
    RefTime,
    Field,
    Principle,
    Exception,
    Statute,
    Precedent,
    Addendum,
    History,
}

const NODE_KINDS: [(&str, NodeKind); 8] = [
    ("기준시점", NodeKind::RefTime),
    ("분야", NodeKind::Field),
    ("원칙", NodeKind::Principle),
    ("예외", NodeKind::Exception),
    ("조문", NodeKind::Statute),
    ("판례·결정", NodeKind::Precedent),
    ("부칙", NodeKind::Addendum),
    ("연혁", NodeKind::History),
];

impl NodeKind {
    pub fn parse(s: &str) -> Result<Self, Error> {
        NODE_KINDS
            .iter()
            .find(|(n, _)| *n == s)
            .map(|(_, k)| *k)
            .ok_or_else(|| Error::UnknownNodeKind(s.to_string()))
    }

    pub fn name(self) -> &'static str {
        NODE_KINDS.iter().find(|(_, k)| *k == self).map_or("", |(n, _)| n)
    }

    pub fn is_rule(self) -> bool {
        matches!(self, Self::Principle | Self::Exception | Self::History)
    }

    pub fn is_source(self) -> bool {
        matches!(self, Self::Statute | Self::Precedent | Self::Addendum)
    }
}

const RELATIONS: [&str; 23] = [
    "근거",
    "분야",
    "기준 시점",
    "예외",
    "같은 구조",
    "예",
    "관련",
    "우선",
    "구체화",
    "특칙",
    "다음",
    "형이 폐지된 경우 함께 적용",
    "판례가 정리",
    "적용 밖",
    "연혁",
    "대비",
    "예(시효기간이 지나지 않은 권리)",
    "기준",
    "함께 적용",
    "판례가 넓힘",
    "판례가 제한",
    "판례가 적용 범위를 보완",
    "같은 기준 시점",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Relation(&'static str);

impl Relation {
    pub const BASIS: Relation = Relation("근거");
    pub const FIELD: Relation = Relation("분야");
    pub const REF_TIME: Relation = Relation("기준 시점");
    pub const SPECIAL: Relation = Relation("특칙");

    pub fn parse(s: &str) -> Result<Self, Error> {
        RELATIONS
            .iter()
            .find(|r| **r == s)
            .map(|r| Relation(r))
            .ok_or_else(|| Error::UnknownRelation(s.to_string()))
    }

    pub fn name(self) -> &'static str {
        self.0
    }
}
