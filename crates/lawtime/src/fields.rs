use serde::Serialize;

use crate::Error;
use crate::adjust::Family;
use crate::facts::CaseKind;
use crate::principles::Graph;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Row {
    pub kind: CaseKind,
    pub field: &'static str,
    pub principle: Option<&'static str>,
    pub span_principle: Option<&'static str>,
    pub adjust: Option<Family>,
}

const fn row(
    kind: CaseKind,
    field: &'static str,
    principle: Option<&'static str>,
    adjust: Option<Family>,
) -> Row {
    Row { kind, field, principle, span_principle: None, adjust }
}

pub const ROWS: [Row; 12] = [
    Row {
        kind: CaseKind::CriminalSubstantive,
        field: "F:형사",
        principle: Some("P:행위시법"),
        span_principle: Some("P:포괄일죄실행종료"),
        adjust: Some(Family::CriminalLighter),
    },
    row(CaseKind::CriminalProcedure, "F:형사", Some("P:형사절차신법"), None),
    row(CaseKind::AdminSanction, "F:행정", Some("P:제재행위시법"), Some(Family::SanctionLighter)),
    row(CaseKind::AdminFine, "F:행정", Some("P:과태료행위시법"), Some(Family::FineLighter)),
    row(CaseKind::AdminApplication, "F:행정", Some("P:처분시법"), Some(Family::ApplicationChange)),
    row(CaseKind::AdminGeneral, "F:행정", Some("P:행정소급금지"), None),
    row(CaseKind::AdminLitigation, "F:행정", Some("P:위법판단처분시"), None),
    row(CaseKind::AdminOmission, "F:행정", None, None),
    row(CaseKind::CivilSubstantive, "F:민사", Some("P:민법소급효"), None),
    row(CaseKind::CivilProcedure, "F:민사", Some("P:민사소송신법"), None),
    row(CaseKind::Tax, "F:조세", Some("P:납세의무성립시기"), Some(Family::TaxPeriodChange)),
    row(CaseKind::InternationalInheritance, "F:국제사법", Some("P:상속준거법사망시"), None),
];

pub fn of(kind: CaseKind) -> Result<Row, Error> {
    ROWS.iter()
        .find(|r| r.kind == kind)
        .copied()
        .ok_or_else(|| Error::Table(format!("사건 종류 행이 없음: {kind:?}")))
}

pub fn check(graph: &Graph) -> Result<(), Error> {
    for r in &ROWS {
        let label = graph.field_label(r.field)?;
        for id in r.principle.iter().chain(r.span_principle.iter()) {
            let node = graph.rule(id)?;
            if node.field.as_deref() != Some(label) {
                return Err(Error::Graph(format!("{id}의 분야가 {label}가 아님")));
            }
        }
    }
    Ok(())
}
