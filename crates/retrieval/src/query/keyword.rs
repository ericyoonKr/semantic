pub mod bigram;
pub mod error;

use std::ops::Bound;
use std::path::Path;

use chrono::{Datelike, NaiveDate};

use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, BoostQuery, ConstScoreQuery, Occur, Query, RangeQuery, TermQuery};
use tantivy::schema::{
    FAST, Field, FieldType, INDEXED, IndexRecordOption, STORED, STRING, Schema, TextFieldIndexing, TextOptions, Value,
};
use tantivy::{Index, IndexReader, ReloadPolicy, TantivyDocument, Term};

use bigram::BigramTokenizer;
use tantivy::tokenizer::TokenStream;
use error::KeywordError;

pub const TOKENIZER: &str = "bigram-v1";
pub const KIND_UNIT: &str = "unit";
pub const KIND_NODE: &str = "node";
pub const KIND_SECTION: &str = "section";
const HEAD_BOOST: f32 = 2.0;
pub const OPEN_FROM: i64 = i64::MIN;
pub const OPEN_TO: i64 = i64::MAX;

#[derive(Debug, Clone, Copy)]
pub struct Fields {
    pub kind: Field,
    pub id: Field,
    pub row: Field,
    pub law_id: Field,
    pub head: Field,
    pub text: Field,
    pub valid_from: Field,
    pub valid_to: Field,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KeywordHit {
    pub kind: String,
    pub id: String,
    pub row: i64,
    pub law_id: String,
    pub score: f32,
}

pub fn schema() -> (Schema, Fields) {
    let mut b = Schema::builder();
    let words = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default().set_tokenizer(TOKENIZER).set_index_option(IndexRecordOption::WithFreqs),
    );
    let fields = Fields {
        kind: b.add_text_field("kind", STRING | STORED),
        id: b.add_text_field("id", STRING | STORED),
        row: b.add_i64_field("row", STORED),
        law_id: b.add_text_field("law_id", STRING | STORED),
        head: b.add_text_field("head", words.clone()),
        text: b.add_text_field("text", words),
        valid_from: b.add_i64_field("valid_from", INDEXED | FAST),
        valid_to: b.add_i64_field("valid_to", INDEXED | FAST),
    };
    (b.build(), fields)
}

pub fn day_number(d: NaiveDate) -> i64 {
    i64::from(d.num_days_from_ce())
}

pub fn register(index: &Index) {
    index.tokenizers().register(TOKENIZER, BigramTokenizer);
}

fn built_with(index: &Index, field: Field) -> Option<String> {
    match index.schema().get_field_entry(field).field_type() {
        FieldType::Str(o) => o.get_indexing_options().map(|i| i.tokenizer().to_string()),
        _ => None,
    }
}

pub struct KeywordIndex {
    index: Index,
    reader: IndexReader,
    fields: Fields,
    max_df_ratio: f64,
}

pub const KEEP_RAREST: usize = 3;

fn text_of(doc: &TantivyDocument, field: Field) -> String {
    doc.get_first(field).and_then(|v| v.as_str()).unwrap_or_default().to_string()
}

fn int_of(doc: &TantivyDocument, field: Field) -> i64 {
    doc.get_first(field).and_then(|v| v.as_i64()).unwrap_or_default()
}

impl KeywordIndex {
    pub fn open(dir: &Path) -> Result<Self, KeywordError> {
        let index = Index::open_in_dir(dir).map_err(error::open(dir))?;
        let (_, fields) = schema();
        let found = built_with(&index, fields.text).unwrap_or_default();
        if found != TOKENIZER {
            return Err(error::mismatch(dir, &found, TOKENIZER));
        }
        register(&index);
        let reader = index.reader_builder().reload_policy(ReloadPolicy::Manual).try_into().map_err(error::open(dir))?;
        Ok(Self { index, reader, fields, max_df_ratio: 1.0 })
    }

    fn valid_on(&self, on: NaiveDate) -> Vec<(Occur, Box<dyn Query>)> {
        let day = day_number(on);
        let started = RangeQuery::new(Bound::Unbounded, Bound::Included(Term::from_field_i64(self.fields.valid_from, day)));
        let not_ended = RangeQuery::new(Bound::Excluded(Term::from_field_i64(self.fields.valid_to, day)), Bound::Unbounded);
        vec![(Occur::Must, Box::new(started)), (Occur::Must, Box::new(not_ended))]
    }

    fn pieces(&self, text: &str) -> Result<Vec<String>, KeywordError> {
        let mut analyzer = self.index.tokenizer_for_field(self.fields.text).map_err(error::search)?;
        let mut stream = analyzer.token_stream(text);
        let mut out = Vec::new();
        while stream.advance() {
            out.push(stream.token().text.clone());
        }
        Ok(out)
    }

    pub fn tokenizer(&self) -> String {
        built_with(&self.index, self.fields.text).unwrap_or_default()
    }

    pub fn with_max_df_ratio(mut self, ratio: f64) -> Self {
        self.max_df_ratio = ratio;
        self
    }

    fn prune(&self, pieces: Vec<String>) -> Result<Vec<String>, KeywordError> {
        if self.max_df_ratio >= 1.0 {
            return Ok(pieces);
        }
        let searcher = self.reader.searcher();
        let limit = (searcher.num_docs() as f64 * self.max_df_ratio) as u64;
        let mut freq: Vec<(u64, String)> = Vec::with_capacity(pieces.len());
        for p in pieces {
            let df = searcher.doc_freq(&Term::from_field_text(self.fields.text, &p)).map_err(error::search)?;
            freq.push((df, p));
        }
        let kept: Vec<String> = freq.iter().filter(|(df, _)| *df <= limit).map(|(_, p)| p.clone()).collect();
        if !kept.is_empty() {
            return Ok(kept);
        }
        freq.sort();
        Ok(freq.into_iter().take(KEEP_RAREST).map(|(_, p)| p).collect())
    }

    pub fn rarest_words(&self, words: &[String], min_chars: usize, n: usize) -> Result<Vec<usize>, KeywordError> {
        let searcher = self.reader.searcher();
        let mut found: Vec<(u64, usize)> = Vec::new();
        for (i, w) in words.iter().enumerate() {
            if w.chars().count() < min_chars {
                continue;
            }
            let mut df = u64::MAX;
            for p in self.pieces(w)? {
                df = df.min(searcher.doc_freq(&Term::from_field_text(self.fields.text, &p)).map_err(error::search)?);
            }
            if df != u64::MAX {
                found.push((df, i));
            }
        }
        found.sort();
        Ok(found.into_iter().take(n).map(|(_, i)| i).collect())
    }

    fn words(&self, text: &str) -> Result<Option<Box<dyn Query>>, KeywordError> {
        let mut pieces = self.pieces(text)?;
        pieces.sort();
        pieces.dedup();
        let pieces = self.prune(pieces)?;
        if pieces.is_empty() {
            return Ok(None);
        }
        let mut any: Vec<(Occur, Box<dyn Query>)> = Vec::with_capacity(pieces.len() * 2);
        for p in &pieces {
            let head = TermQuery::new(Term::from_field_text(self.fields.head, p), IndexRecordOption::WithFreqs);
            let body = TermQuery::new(Term::from_field_text(self.fields.text, p), IndexRecordOption::WithFreqs);
            any.push((Occur::Should, Box::new(BoostQuery::new(Box::new(head), HEAD_BOOST))));
            any.push((Occur::Should, Box::new(body)));
        }
        Ok(Some(Box::new(BooleanQuery::new(any))))
    }

    fn query(&self, words: Box<dyn Query>, law_ids: &[String], on: NaiveDate) -> Box<dyn Query> {
        let mut parts: Vec<(Occur, Box<dyn Query>)> = vec![(Occur::Must, words)];
        parts.extend(self.valid_on(on));
        if !law_ids.is_empty() {
            let laws: Vec<(Occur, Box<dyn Query>)> = law_ids
                .iter()
                .map(|id| {
                    let term = Term::from_field_text(self.fields.law_id, id);
                    (Occur::Should, Box::new(TermQuery::new(term, IndexRecordOption::Basic)) as Box<dyn Query>)
                })
                .collect();
            parts.push((Occur::Must, Box::new(BooleanQuery::new(laws))));
        }
        Box::new(BooleanQuery::new(parts))
    }

    pub fn search(&self, text: &str, law_ids: &[String], on: NaiveDate, limit: usize) -> Result<Vec<KeywordHit>, KeywordError> {
        let Some(words) = self.words(text)? else {
            return Ok(Vec::new());
        };
        self.top(&*self.query(words, law_ids, on), limit)
    }

    pub fn search_sections(&self, text: &str, limit: usize) -> Result<Vec<KeywordHit>, KeywordError> {
        let Some(words) = self.words(text)? else {
            return Ok(Vec::new());
        };
        let kind = TermQuery::new(Term::from_field_text(self.fields.kind, KIND_SECTION), IndexRecordOption::Basic);
        let query = BooleanQuery::new(vec![(Occur::Must, words), (Occur::Must, Box::new(ConstScoreQuery::new(Box::new(kind), 0.0)))]);
        self.top(&query, limit)
    }

    fn top(&self, query: &dyn Query, limit: usize) -> Result<Vec<KeywordHit>, KeywordError> {
        let searcher = self.reader.searcher();
        let top = searcher.search(query, &TopDocs::with_limit(limit.max(1)).order_by_score()).map_err(error::search)?;
        top.into_iter()
            .map(|(score, address)| {
                let doc: TantivyDocument = searcher.doc(address).map_err(error::read)?;
                Ok(KeywordHit {
                    kind: text_of(&doc, self.fields.kind),
                    id: text_of(&doc, self.fields.id),
                    row: int_of(&doc, self.fields.row),
                    law_id: text_of(&doc, self.fields.law_id),
                    score,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{KIND_SECTION, KIND_UNIT, KeywordIndex, OPEN_FROM, OPEN_TO, TOKENIZER, day_number, register, schema};
    use chrono::NaiveDate;
    use tantivy::{Index, doc};

    fn day(y: i32, m: u32, d: u32) -> Result<NaiveDate, Box<dyn std::error::Error>> {
        NaiveDate::from_ymd_opt(y, m, d).ok_or_else(|| "bad date".into())
    }

    fn build(dir: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
        let (schema, f) = schema();
        let index = Index::create_in_dir(dir, schema)?;
        register(&index);
        let mut writer = index.writer(50_000_000)?;
        let (from, until) = (day_number(day(2020, 1, 1)?), day_number(day(2023, 1, 1)?));
        writer.add_document(doc!(f.kind => KIND_UNIT, f.id => "1", f.row => 10i64, f.law_id => "A",
            f.head => "행정절차법 제21조 처분의 사전 통지", f.text => "행정청은 권익을 제한하는 처분을 하는 경우에는 미리 당사자에게 통지하여야 한다.",
            f.valid_from => from, f.valid_to => OPEN_TO))?;
        writer.add_document(doc!(f.kind => KIND_UNIT, f.id => "2", f.row => 20i64, f.law_id => "B",
            f.head => "민법 제750조 불법행위의 내용", f.text => "고의 또는 과실로 인한 위법행위로 타인에게 손해를 가한 자는 그 손해를 배상할 책임이 있다.",
            f.valid_from => from, f.valid_to => OPEN_TO))?;
        writer.add_document(doc!(f.kind => KIND_UNIT, f.id => "3", f.row => 30i64, f.law_id => "A",
            f.head => "행정절차법 제21조 처분의 사전 통지", f.text => "행정청은 처분을 하는 경우 미리 통지하여야 한다.",
            f.valid_from => from - 1000, f.valid_to => until - 1000))?;
        writer.commit()?;
        Ok(())
    }

    #[test]
    fn particles_do_not_block_matches_and_law_filter_restricts_hits() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        build(dir.path())?;
        let index = KeywordIndex::open(dir.path())?;
        assert_eq!(index.tokenizer(), TOKENIZER);
        let today = day(2024, 6, 1)?;
        let hits = index.search("처분을 하기 전의 통지", &[], today, 5)?;
        assert_eq!(hits.first().map(|h| h.id.as_str()), Some("1"));
        assert_eq!(hits.first().map(|h| h.row), Some(10));
        assert!(hits.iter().all(|h| h.id != "3"), "{hits:?}");
        let past = index.search("처분 통지", &[], day(2018, 6, 1)?, 5)?;
        assert_eq!(past.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(), ["3"]);
        let only_b = index.search("처분 통지 손해", &["B".to_string()], today, 5)?;
        assert!(only_b.iter().all(|h| h.law_id == "B"), "{only_b:?}");
        Ok(())
    }

    #[test]
    fn section_search_reads_only_case_sections() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let (schema, f) = schema();
        let index = tantivy::Index::create_in_dir(dir.path(), schema)?;
        register(&index);
        let mut writer = index.writer(50_000_000)?;
        writer.add_document(doc!(f.kind => KIND_SECTION, f.id => "501", f.row => 50i64, f.law_id => "",
            f.head => "부당해고구제재심판정취소", f.text => "근로자에 대한 해고가 정당한 이유 없이 이루어진 경우 부당해고에 해당한다.",
            f.valid_from => OPEN_FROM, f.valid_to => OPEN_TO))?;
        writer.add_document(doc!(f.kind => KIND_SECTION, f.id => "502", f.row => 51i64, f.law_id => "",
            f.head => "손해배상", f.text => "공무원의 직무상 과실로 인한 국가배상 책임을 인정한 사례.",
            f.valid_from => OPEN_FROM, f.valid_to => OPEN_TO))?;
        writer.add_document(doc!(f.kind => KIND_UNIT, f.id => "9", f.row => 90i64, f.law_id => "A",
            f.head => "근로기준법 제23조 해고 등의 제한", f.text => "사용자는 근로자에게 정당한 이유 없이 해고를 하지 못한다.",
            f.valid_from => OPEN_FROM, f.valid_to => OPEN_TO))?;
        writer.commit()?;
        let index = KeywordIndex::open(dir.path())?;
        let hits = index.search_sections("정당한 이유 없는 해고는 부당해고", 5)?;
        assert_eq!(hits.iter().map(|h| (h.id.as_str(), h.row)).collect::<Vec<_>>(), [("501", 50)]);
        assert!(hits.iter().all(|h| h.kind == KIND_SECTION), "{hits:?}");
        assert!(index.search_sections("", 5)?.is_empty());
        Ok(())
    }
}
