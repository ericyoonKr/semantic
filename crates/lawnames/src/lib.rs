use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("약칭 파일을 읽지 못함: {0}")]
    Io(#[from] std::io::Error),
    #[error("약칭 파일 형식이 다름: {0}")]
    Json(#[from] serde_json::Error),
    #[error("약칭 파일 형식 이름이 다름: {0}")]
    Schema(String),
    #[error("같은 약칭이 두 번 있음: {0}")]
    Duplicate(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Entry {
    pub alias: String,
    pub law_id: String,
    pub name: String,
    pub source: String,
}

#[derive(Deserialize)]
struct File {
    schema: String,
    aliases: Vec<Entry>,
}

const SCHEMA: &str = "law-aliases/1";
const DOTS: [char; 4] = ['ㆍ', '・', '‧', '∙'];

pub fn key(name: &str) -> String {
    name.chars().filter(|c| !c.is_whitespace()).map(|c| if DOTS.contains(&c) { '·' } else { c }).collect()
}

#[derive(Debug, Default)]
pub struct Aliases {
    map: HashMap<String, Entry>,
}

impl Aliases {
    pub fn from_entries(entries: Vec<Entry>) -> Result<Self, Error> {
        let mut map = HashMap::new();
        for e in entries {
            let k = key(&e.alias);
            if map.insert(k.clone(), e).is_some() {
                return Err(Error::Duplicate(k));
            }
        }
        Ok(Self { map })
    }

    pub fn load(path: &Path) -> Result<Self, Error> {
        let file: File = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        if file.schema != SCHEMA {
            return Err(Error::Schema(file.schema));
        }
        Self::from_entries(file.aliases)
    }

    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.map.get(&key(name))
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

static GLOBAL: OnceLock<Aliases> = OnceLock::new();

pub fn install(aliases: Aliases) -> bool {
    GLOBAL.set(aliases).is_ok()
}

pub fn lookup(name: &str) -> Option<&'static Entry> {
    GLOBAL.get().and_then(|a| a.get(name))
}
