use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use serde::Deserialize;

use crate::sample::{Item, read_jsonl};

#[derive(Debug, Deserialize)]
struct Prediction {
    id: String,
    answer: Option<i64>,
}

pub fn report(items: &[Item], predictions: Option<&Path>) -> Result<()> {
    let qs: Vec<&Item> = items.iter().filter(|i| i.kind == "mcq").collect();
    if qs.is_empty() {
        return Ok(());
    }
    println!("== 선택형 문항 {}개", qs.len());
    let Some(path) = predictions else {
        println!("  답 기록 파일(--predictions)이 없어 정답률은 아직 안 쟀습니다");
        return Ok(());
    };
    let preds: HashMap<String, Option<i64>> = read_jsonl::<Prediction>(path)?
        .into_iter()
        .map(|p| (p.id, p.answer))
        .collect();
    let (mut right, mut wrong, mut blank) = (0usize, 0usize, 0usize);
    for q in &qs {
        let gold = q
            .gold
            .as_ref()
            .map(|g| g.official_answers.clone())
            .unwrap_or_default();
        match preds.get(&q.id).copied().flatten() {
            Some(a) if gold.contains(&a) => right += 1,
            Some(_) => wrong += 1,
            None => blank += 1,
        }
    }
    let n = qs.len();
    println!(
        "  정답 {right} · 오답 {wrong} · 무답 {blank} · 정답률 {:.1}%",
        100.0 * right as f64 / n as f64
    );
    Ok(())
}
