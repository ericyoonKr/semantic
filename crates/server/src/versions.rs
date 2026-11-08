use std::path::Path;
use std::sync::Arc;

use lawtime::addendum::RuleSet;
use lawtime::adjust::AdjustTable;
use lawtime::principles::Graph;
use lawtime::resolve::Engine;
use lawtime::versions::MemoryStore;

pub struct LawtimeParts {
    pub graph: Graph,
    pub adjust: AdjustTable,
}

impl LawtimeParts {
    pub fn load(path: &Path) -> Result<Self, lawtime::Error> {
        let graph = Graph::from_path(path)?;
        let adjust = AdjustTable::standard()?;
        Engine::new(&graph, &MemoryStore::new(), &RuleSet::default(), &adjust)?;
        Ok(Self { graph, adjust })
    }
}

pub fn startup(path: Option<&Path>) -> Option<Arc<LawtimeParts>> {
    let Some(path) = path else {
        tracing::warn!("LAWTIME_PRINCIPLES is not set; version resolution will report failure");
        return None;
    };
    match LawtimeParts::load(path) {
        Ok(parts) => {
            tracing::info!(nodes = parts.graph.nodes().count(), "time principle graph ready");
            Some(Arc::new(parts))
        }
        Err(e) => {
            tracing::error!(error = %e, "time principle graph could not be read; version resolution disabled");
            None
        }
    }
}
