pub mod arith;
pub mod compile;
pub mod deps;
pub mod error;
pub mod graph;
pub mod judge;
pub mod leaf;
pub mod period;
pub mod prompt;
pub mod schema;
pub mod settings;
pub mod slices;
pub mod verdict;

pub use compile::{
    Checked, PeriodFacts, RawVerdict, SolverResult, burden_fixes, check, check_both,
    check_with_periods, period_facts,
};
pub use deps::{
    AuthorityHit, AuthoritySearch, AuthoritySnapshot, DepError, EngineDeps, FactProvider,
    LeafCache, Passage, UserFact,
};
pub use error::{CountKind, EngineError, GraphIssue};
pub use graph::ClaimGraph;
pub use judge::{JudgeRequest, judge};
pub use leaf::judge_all;
pub use period::{
    DateSource, DateUse, HolidayNote, PeriodCase, PeriodLength, PeriodReport, StartRule,
};
pub use prompt::prompt_version;
pub use schema::{
    ArithSpec, Burden, ClaimEdge, ClaimNode, ClaimType, Evidence, ExpectedCounts, FailureReason,
    GateType, LeafResult, NodeId, PeriodRule, Relation, Trit, cache_key,
};
pub use settings::EngineSettings;
pub use slices::{
    FactDocument, FactSlice, LeafRequest, Prepared, SlicePlan, build_leaf_requests,
    estimate_tokens,
};
pub use verdict::{
    BurdenVerdict, Outcome, ReductionNote, Verdict, from_checked, from_raw, outcome_of,
};
