use std::fs;
use std::ops::Deref;
use std::sync::Arc;

use kg::{KgConfig, KgSet};
use sessdb::SessDb;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::config::Config;
use crate::error::StartupError;
use crate::judge::EngineParts;
use crate::kgids::KgIdLocks;
use crate::rules::RuleSet;
use crate::session::Registry;
use crate::tasks;
use crate::upload::PdfWorker;

pub struct Inner {
    pub cfg: Config,
    pub db: SessDb,
    pub kg: Arc<KgSet>,
    pub rules: Arc<RuleSet>,
    pub engine: Option<EngineParts>,
    pub corpus: Option<retrieval::Corpus>,
    pub lawtime: Option<Arc<crate::versions::LawtimeParts>>,
    pub z3: Semaphore,
    pub cpu: rayon::ThreadPool,
    pub uploads: Semaphore,
    pub pdf: PdfWorker,
    pub sessions: Registry,
    pub kg_ids: KgIdLocks,
    pub shutdown: CancellationToken,
    pub tracker: TaskTracker,
    pub reads: crate::funcall::readlog::ReadLog,
    pub steps: crate::funcall::next_step::StepLog,
    pub coverage: crate::funcall::coverage::Coverage,
    pub known: crate::funcall::known::KnownLaws,
}

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

impl Deref for AppState {
    type Target = Inner;

    fn deref(&self) -> &Inner {
        &self.0
    }
}

impl AppState {
    pub async fn open(cfg: Config) -> Result<Self, StartupError> {
        fs::create_dir_all(cfg.rocksdb_dir())?;
        let db = sessdb::open(&cfg.rocksdb_dir())?;
        let cpu = rayon::ThreadPoolBuilder::new()
            .num_threads(cfg.threads.rayon)
            .thread_name(|i| format!("cpu-{i}"))
            .build()
            .map_err(|_| StartupError::CpuPool)?;
        let kg = Arc::new(load_kg(&cfg, &cpu).await);
        let rules = Arc::new(crate::rules::startup(cfg.kg_dir.clone(), kg.clone(), &cpu).await);
        let engine = EngineParts::build(&cfg.judge, &db);
        let reads = crate::funcall::readlog::ReadLog::new(db.clone());
        let corpus = cfg.corpus.open();
        let lawtime = crate::versions::startup(cfg.lawtime_principles.as_deref());
        crate::versions::load_aliases(cfg.law_aliases.as_deref());
        let state = Self(Arc::new(Inner {
            rules,
            engine,
            corpus,
            lawtime,
            z3: Semaphore::new(cfg.judge.z3_concurrency),
            uploads: Semaphore::new(cfg.limits.upload_concurrency),
            pdf: PdfWorker::start(cfg.pdfium_dir.as_deref(), cfg.ocr_dir.as_deref(), cfg.ocr_threads, cfg.ocr_lanes, cfg.limits.upload_concurrency),
            cfg,
            db,
            kg,
            cpu,
            sessions: Registry::default(),
            kg_ids: KgIdLocks::default(),
            shutdown: CancellationToken::new(),
            tracker: TaskTracker::new(),
            reads,
            steps: crate::funcall::next_step::StepLog::default(),
            coverage: crate::funcall::coverage::Coverage::default(),
            known: crate::funcall::known::KnownLaws::default(),
        }));
        let loader = state.clone();
        state.tracker.spawn(async move {
            if let Some(corpus) = loader.corpus.as_ref() {
                loader.known.load(corpus).await;
            }
        });
        Ok(state)
    }
}

async fn load_kg(cfg: &Config, cpu: &rayon::ThreadPool) -> KgSet {
    let dir = cfg.kg_dir.clone();
    let kg_cfg = KgConfig::new(&cfg.kg_consumer_schema, &cfg.kg_manifest_schema)
        .with_source_schema(cfg.kg_source_schema.clone());
    match tasks::cpu(cpu, move || kg::sync_all(&dir, &kg_cfg)).await {
        Ok(Ok(set)) => {
            for d in set.disabled() {
                tracing::warn!(bundle = %d.bundle, reason = %d.reason, "knowledge graph bundle disabled");
            }
            tracing::info!(
                graphs = set.catalog().len(),
                disabled = set.disabled().len(),
                "knowledge graphs ready"
            );
            set
        }
        Ok(Err(e)) => {
            tracing::error!(error = %e, "knowledge graphs could not be read; continuing without them");
            KgSet::default()
        }
        Err(_) => {
            tracing::error!("knowledge graph loading failed; continuing without them");
            KgSet::default()
        }
    }
}
