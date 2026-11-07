use std::time::Duration;

use llm::CallOptions;

pub const LEAF_PRIORITY: i32 = 2;
pub const PER_JUDGE_PERMITS: usize = 8;
pub const LEAF_DEADLINE: Duration = Duration::from_secs(60);
pub const LEAF_CALL_DEADLINE: Duration = Duration::from_secs(45);

#[derive(Debug, Clone)]
pub struct EngineSettings {
    pub fact_budget_tokens: usize,
    pub candidate_k: usize,
    pub authority_k: usize,
    pub per_judge_permits: usize,
    pub leaf_deadline: Duration,
    pub call: CallOptions,
    pub solver_timeout: Duration,
}

impl EngineSettings {
    pub fn new(
        fact_budget_tokens: usize,
        candidate_k: usize,
        authority_k: usize,
        solver_timeout: Duration,
    ) -> Self {
        Self {
            fact_budget_tokens,
            candidate_k,
            authority_k,
            per_judge_permits: PER_JUDGE_PERMITS,
            leaf_deadline: LEAF_DEADLINE,
            call: CallOptions {
                deadline: LEAF_CALL_DEADLINE,
                priority: Some(LEAF_PRIORITY),
                ..CallOptions::default()
            },
            solver_timeout,
        }
    }
}
