pub(crate) mod ask;
pub(crate) mod settle;

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use futures::future::join_all;
use tokio_util::sync::CancellationToken;

use crate::deps::{AuthoritySearch, EngineDeps, FactProvider, LeafCache};
use crate::schema::{FailureReason, LeafResult, NodeId};
use crate::slices::{LeafRequest, NO_FACTS, Prepared};
use ask::{Caller, ask_all};
use settle::{Settled, is_true, settle};

pub use settle::{ABORTED, CANCELLED, LOCATE_FAILED, MODEL_FOUND_NOTHING};

struct Run<'a, F, A, C>
where
    F: FactProvider,
    A: AuthoritySearch,
    C: LeafCache,
{
    deps: &'a EngineDeps<F, A, C>,
    session: &'a str,
    caller: Caller,
    ready: HashMap<NodeId, Arc<LeafRequest>>,
    keys: HashMap<NodeId, String>,
    hits: HashMap<NodeId, LeafResult>,
    done: HashMap<NodeId, Settled>,
}

pub async fn judge_all<F, A, C>(
    reqs: Vec<Prepared>,
    deps: &EngineDeps<F, A, C>,
    session: &str,
    cancel: &CancellationToken,
) -> HashMap<NodeId, LeafResult>
where
    F: FactProvider,
    A: AuthoritySearch,
    C: LeafCache,
{
    let mut run = Run {
        deps,
        session,
        caller: Caller::new(deps, cancel),
        ready: HashMap::new(),
        keys: HashMap::new(),
        hits: HashMap::new(),
        done: HashMap::new(),
    };
    for prepared in reqs {
        match prepared {
            Prepared::Ready(req) => {
                run.keys.insert(req.node_id.clone(), req.cache_key.clone());
                run.ready.insert(req.node_id.clone(), Arc::new(req));
            }
            Prepared::NoEvidence { node_id, cache_key } => {
                run.keys.insert(node_id.clone(), cache_key);
                let failure = FailureReason::NoEvidence(NO_FACTS.to_string());
                run.done.insert(node_id, Settled::unknown(failure, true));
            }
            Prepared::Failed { node_id, failure } => {
                run.done.insert(node_id, Settled::unknown(failure, false));
            }
        }
    }
    let misses = run.look_up().await;
    run.ask(&misses).await;
    run.resolve_contradictions().await;
    run.store().await;
    let mut out = run.hits;
    out.extend(run.done.into_iter().map(|(id, s)| (id, s.result)));
    out
}

impl<F, A, C> Run<'_, F, A, C>
where
    F: FactProvider,
    A: AuthoritySearch,
    C: LeafCache,
{
    async fn look_up(&mut self) -> Vec<Arc<LeafRequest>> {
        let cache = &self.deps.cache;
        let looked = join_all(
            self.ready
                .values()
                .map(|req| async move { (req.clone(), cache.get(&req.cache_key).await) }),
        )
        .await;
        let mut misses = Vec::new();
        for (req, got) in looked {
            match got {
                Ok(Some(hit)) => {
                    self.hits.insert(req.node_id.clone(), hit);
                }
                Ok(None) => misses.push(req),
                Err(error) => {
                    tracing::warn!(node = %req.node_id, %error, "engine.leaf.cache_get");
                    misses.push(req);
                }
            }
        }
        misses
    }

    async fn ask(&mut self, reqs: &[Arc<LeafRequest>]) -> HashMap<NodeId, Settled> {
        let asked = ask_all(&self.caller, reqs).await;
        let facts = &self.deps.facts;
        let session = self.session;
        let settled = join_all(asked.into_iter().filter_map(|(id, answer)| {
            let req = self.ready.get(&id)?.clone();
            Some(async move { (id, settle(answer, &req, facts, session).await) })
        }))
        .await;
        let settled: HashMap<NodeId, Settled> = settled.into_iter().collect();
        for (id, s) in &settled {
            self.hits.remove(id);
            self.done.insert(id.clone(), s.clone());
        }
        settled
    }

    fn current_true(&self, id: &NodeId) -> bool {
        self.done
            .get(id)
            .map(Settled::is_true)
            .or_else(|| self.hits.get(id).map(is_true))
            .unwrap_or(false)
    }

    fn contradicted_pairs(&self) -> Vec<(NodeId, NodeId)> {
        let pairs: BTreeSet<(NodeId, NodeId)> = self
            .ready
            .values()
            .filter_map(|req| {
                let other = req.negation_of.as_ref()?;
                if !self.ready.contains_key(other) || other == &req.node_id {
                    return None;
                }
                let (a, b) = if req.node_id < *other {
                    (req.node_id.clone(), other.clone())
                } else {
                    (other.clone(), req.node_id.clone())
                };
                Some((a, b))
            })
            .collect();
        pairs
            .into_iter()
            .filter(|(a, b)| self.current_true(a) && self.current_true(b))
            .collect()
    }

    async fn resolve_contradictions(&mut self) {
        let pairs = self.contradicted_pairs();
        if pairs.is_empty() {
            return;
        }
        let ids: BTreeSet<NodeId> = pairs
            .iter()
            .flat_map(|(a, b)| [a.clone(), b.clone()])
            .collect();
        let reqs: Vec<Arc<LeafRequest>> = ids
            .iter()
            .filter_map(|id| self.ready.get(id).cloned())
            .collect();
        tracing::info!(pairs = pairs.len(), "engine.leaf.rejudge");
        let again = self.ask(&reqs).await;
        for (a, b) in pairs {
            let still = again.get(&a).is_some_and(Settled::is_true)
                && again.get(&b).is_some_and(Settled::is_true);
            if still {
                for id in [a, b] {
                    self.done
                        .insert(id, Settled::unknown(FailureReason::Contradiction, true));
                }
            }
        }
    }

    async fn store(&self) {
        let cache = &self.deps.cache;
        let puts = self
            .done
            .iter()
            .filter(|(_, s)| s.store)
            .filter_map(|(id, s)| {
                let key = self.keys.get(id)?;
                Some(async move { (id, cache.put(key, &s.result).await) })
            });
        for (id, outcome) in join_all(puts).await {
            if let Err(error) = outcome {
                tracing::warn!(node = %id, %error, "engine.leaf.cache_put");
            }
        }
    }
}
