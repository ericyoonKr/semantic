use std::collections::BTreeMap;
use std::sync::Arc;

use engine::ClaimGraph;

use crate::rules::catalog::{CatalogItem, items};

#[derive(Default)]
pub struct RuleSet {
    trees: BTreeMap<String, Arc<ClaimGraph>>,
    catalog: Vec<CatalogItem>,
}

impl RuleSet {
    pub fn new(trees: BTreeMap<String, ClaimGraph>) -> Self {
        let catalog = trees
            .iter()
            .flat_map(|(claim_type, tree)| items(claim_type, tree))
            .collect();
        let trees = trees.into_iter().map(|(k, v)| (k, Arc::new(v))).collect();
        Self { trees, catalog }
    }

    pub fn tree(&self, claim_type: &str) -> Option<Arc<ClaimGraph>> {
        self.trees.get(claim_type).cloned()
    }

    pub fn catalog(&self) -> &[CatalogItem] {
        &self.catalog
    }

    pub fn len(&self) -> usize {
        self.trees.len()
    }

    pub fn is_empty(&self) -> bool {
        self.trees.is_empty()
    }
}
