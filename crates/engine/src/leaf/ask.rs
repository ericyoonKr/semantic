use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use llm::{CallOptions, LlmClient, LlmError, LlmLimiter, acquire_tiered};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::deps::{AuthoritySearch, EngineDeps, FactProvider, LeafCache};
use crate::prompt::{SYSTEM, user_message};
use crate::schema::{NodeId, Trit};
use crate::slices::LeafRequest;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct LeafAnswer {
    pub value: Trit,
    #[serde(default)]
    pub quote: String,
    #[serde(default)]
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AskFailure {
    Llm(LlmError),
    Cancelled,
    Aborted,
}

pub(crate) type Asked = Result<LeafAnswer, AskFailure>;

#[derive(Clone)]
pub(crate) struct Caller {
    pub llm: LlmClient,
    pub local: LlmLimiter,
    pub global: LlmLimiter,
    pub call: CallOptions,
    pub deadline: Duration,
    pub cancel: CancellationToken,
}

impl Caller {
    pub(crate) fn new<F, A, C>(deps: &EngineDeps<F, A, C>, cancel: &CancellationToken) -> Self
    where
        F: FactProvider,
        A: AuthoritySearch,
        C: LeafCache,
    {
        Self {
            llm: deps.llm.clone(),
            local: LlmLimiter::new(deps.settings.per_judge_permits),
            global: deps.limiter.clone(),
            call: deps.settings.call.clone(),
            deadline: deps.settings.leaf_deadline,
            cancel: cancel.clone(),
        }
    }

    async fn ask(self, req: Arc<LeafRequest>) -> Asked {
        self.call::<LeafAnswer>(SYSTEM, &user_message(&req)).await
    }

    pub(crate) async fn call<T: DeserializeOwned>(&self, system: &str, user: &str) -> Result<T, AskFailure> {
        let work = async {
            let _permit = acquire_tiered(&self.local, &self.global).await?;
            self.llm.complete_json::<T>(system, user, &self.call).await
        };
        tokio::select! {
            biased;
            () = self.cancel.cancelled() => Err(AskFailure::Cancelled),
            timed = tokio::time::timeout(self.deadline, work) => match timed {
                Ok(result) => result.map_err(AskFailure::Llm),
                Err(_) => Err(AskFailure::Llm(LlmError::Timeout)),
            },
        }
    }
}

pub(crate) async fn ask_all(caller: &Caller, reqs: &[Arc<LeafRequest>]) -> HashMap<NodeId, Asked> {
    let mut set = JoinSet::new();
    let mut ids = HashMap::new();
    for req in reqs {
        let handle = set.spawn(caller.clone().ask(req.clone()));
        ids.insert(handle.id(), req.node_id.clone());
    }
    let mut out = HashMap::new();
    while let Some(joined) = set.join_next_with_id().await {
        match joined {
            Ok((task, asked)) => {
                if let Some(id) = ids.get(&task) {
                    out.insert(id.clone(), asked);
                }
            }
            Err(error) => {
                tracing::error!(%error, "engine.leaf.task");
                if let Some(id) = ids.get(&error.id()) {
                    out.insert(id.clone(), Err(AskFailure::Aborted));
                }
            }
        }
    }
    for req in reqs {
        out.entry(req.node_id.clone())
            .or_insert(Err(AskFailure::Aborted));
    }
    out
}
