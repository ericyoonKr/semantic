use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use z3::ast::Bool;
use z3::{Config, SatResult, Solver, with_z3_config};

use crate::error::EngineError;
use crate::graph::ClaimGraph;
use crate::schema::{GateType, LeafResult, NodeId, Relation};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SolverResult {
    Sat,
    Unsat,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawVerdict {
    pub scope: NodeId,
    pub holds: SolverResult,
    pub fails: SolverResult,
    pub core: Vec<NodeId>,
    pub model: Option<Vec<(NodeId, bool)>>,
}

struct Plan {
    order: Vec<NodeId>,
    facts: Vec<(NodeId, bool)>,
    assumed: Vec<(NodeId, bool)>,
    free: Vec<NodeId>,
}

struct Solved {
    holds: SolverResult,
    fails: SolverResult,
    core: Vec<NodeId>,
    model: Option<Vec<(NodeId, bool)>>,
}

pub fn check(
    subtree: &ClaimGraph,
    leaves: &HashMap<NodeId, LeafResult>,
    assumptions: &HashMap<NodeId, bool>,
    timeout: Duration,
) -> Result<RawVerdict, EngineError> {
    let roots = subtree.roots();
    let [scope] = roots.as_slice() else {
        return Err(EngineError::NotSingleRoot(roots));
    };
    let plan = plan(subtree, leaves, assumptions)?;
    let mut cfg = Config::new();
    cfg.set_timeout_msec(u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX).max(1));
    let solved = with_z3_config(&cfg, || solve(subtree, &plan, scope));
    Ok(RawVerdict {
        scope: scope.clone(),
        holds: solved.holds,
        fails: solved.fails,
        core: solved.core,
        model: solved.model,
    })
}

fn plan(
    subtree: &ClaimGraph,
    leaves: &HashMap<NodeId, LeafResult>,
    assumptions: &HashMap<NodeId, bool>,
) -> Result<Plan, EngineError> {
    if let Some(stray) = assumptions.keys().find(|id| !subtree.contains(id)) {
        return Err(EngineError::UnknownNode(stray.clone()));
    }
    let mut facts = Vec::new();
    let mut free = Vec::new();
    for leaf in subtree.leaves() {
        if leaf.arithmetic.is_some() {
            return Err(EngineError::ArithmeticUnsupported(leaf.id.clone()));
        }
        match leaves.get(&leaf.id) {
            None => return Err(EngineError::MissingLeafResult(leaf.id.clone())),
            Some(LeafResult::Judged { value, .. }) => facts.push((leaf.id.clone(), *value)),
            Some(LeafResult::Unknown { .. }) => {
                if !assumptions.contains_key(&leaf.id) {
                    free.push(leaf.id.clone());
                }
            }
        }
    }
    let assumed = subtree
        .nodes()
        .filter_map(|n| assumptions.get(&n.id).map(|v| (n.id.clone(), *v)))
        .collect();
    Ok(Plan {
        order: subtree.topo_order()?,
        facts,
        assumed,
        free,
    })
}

fn solve(graph: &ClaimGraph, plan: &Plan, root: &NodeId) -> Solved {
    let (vars, holds) = build(graph, &plan.order);
    let solver = Solver::new();
    let mut trackers: Vec<(Bool, NodeId)> = Vec::new();
    for (id, value) in &plan.facts {
        if let Some(v) = vars.get(id) {
            track(&solver, &mut trackers, literal(v, *value), id);
        }
    }
    for (id, value) in &plan.assumed {
        if let Some(h) = holds.get(id) {
            track(&solver, &mut trackers, literal(h, *value), id);
        }
    }
    for node in graph.nodes() {
        let Some(other) = &node.negation_of else {
            continue;
        };
        let a = vars.get(&node.id).or_else(|| holds.get(&node.id));
        let b = vars.get(other).or_else(|| holds.get(other));
        if let (Some(a), Some(b)) = (a, b) {
            solver.assert(a.iff(b.not()));
        }
    }
    let Some(goal) = holds.get(root) else {
        return Solved {
            holds: SolverResult::Unknown,
            fails: SolverResult::Unknown,
            core: Vec::new(),
            model: None,
        };
    };
    let core_of = |solver: &Solver| -> Vec<NodeId> {
        let lits = solver.get_unsat_core();
        trackers
            .iter()
            .filter(|(t, _)| lits.iter().any(|l| l == t))
            .map(|(_, id)| id.clone())
            .collect()
    };
    match convert(solver.check()) {
        SolverResult::Sat => {}
        other => {
            let core = if other == SolverResult::Unsat {
                core_of(&solver)
            } else {
                Vec::new()
            };
            return Solved {
                holds: other,
                fails: other,
                core,
                model: None,
            };
        }
    }
    solver.push();
    solver.assert(goal);
    let can_hold = convert(solver.check());
    let mut core = Vec::new();
    let mut model = None;
    match can_hold {
        SolverResult::Unsat => core = core_of(&solver),
        SolverResult::Sat if !plan.free.is_empty() => {
            model = solver.get_model().map(|m| {
                plan.free
                    .iter()
                    .filter_map(|id| {
                        let v = vars.get(id)?;
                        let value = m.eval(v, true)?.as_bool()?;
                        Some((id.clone(), value))
                    })
                    .collect()
            });
        }
        _ => {}
    }
    solver.pop(1);
    solver.push();
    solver.assert(goal.not());
    let can_fail = convert(solver.check());
    solver.pop(1);
    Solved {
        holds: can_hold,
        fails: can_fail,
        core,
        model,
    }
}

fn build(graph: &ClaimGraph, order: &[NodeId]) -> (HashMap<NodeId, Bool>, HashMap<NodeId, Bool>) {
    let mut vars = HashMap::new();
    let mut holds: HashMap<NodeId, Bool> = HashMap::new();
    for id in order.iter().rev() {
        let Some(node) = graph.node(id) else {
            continue;
        };
        let sub = |r: Relation| -> Vec<Bool> {
            graph
                .children(id, r)
                .iter()
                .filter_map(|c| holds.get(&c.id).cloned())
                .collect()
        };
        let base = match node.gate {
            GateType::Leaf => {
                let v = Bool::new_const(id.as_str());
                vars.insert(id.clone(), v.clone());
                v
            }
            GateType::And => Bool::and(&sub(Relation::Element)),
            GateType::Or => Bool::or(&sub(Relation::Element)),
        };
        let mut parts = vec![base];
        for r in [Relation::Defense, Relation::Rebuttal] {
            parts.extend(sub(r).iter().map(Bool::not));
        }
        let expr = match parts.as_slice() {
            [only] => only.clone(),
            _ => Bool::and(&parts),
        };
        holds.insert(id.clone(), expr);
    }
    (vars, holds)
}

fn literal(v: &Bool, value: bool) -> Bool {
    if value { v.clone() } else { v.not() }
}

fn track(solver: &Solver, trackers: &mut Vec<(Bool, NodeId)>, lit: Bool, id: &NodeId) {
    let t = Bool::fresh_const("track");
    solver.assert_and_track(lit, &t);
    trackers.push((t, id.clone()));
}

fn convert(r: SatResult) -> SolverResult {
    match r {
        SatResult::Sat => SolverResult::Sat,
        SatResult::Unsat => SolverResult::Unsat,
        SatResult::Unknown => SolverResult::Unknown,
    }
}
