use std::collections::HashMap;
use std::hash::Hash;

pub const RRF_K: f64 = 60.0;
pub const CANDIDATES: i64 = 80;

pub fn rrf<K: Eq + Hash + Clone>(lists: &[&[K]], k: f64) -> Vec<(K, f64)> {
    let mut order: Vec<K> = Vec::new();
    let mut scores: HashMap<K, f64> = HashMap::new();
    for list in lists {
        let mut seen: Vec<&K> = Vec::new();
        let mut rank = 0usize;
        for key in list.iter() {
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            rank += 1;
            let add = 1.0 / (k + rank as f64);
            match scores.get_mut(key) {
                Some(s) => *s += add,
                None => {
                    scores.insert(key.clone(), add);
                    order.push(key.clone());
                }
            }
        }
    }
    let mut out: Vec<(usize, K, f64)> = order
        .into_iter()
        .enumerate()
        .map(|(i, key)| {
            let s = scores.get(&key).copied().unwrap_or(0.0);
            (i, key, s)
        })
        .collect();
    out.sort_by(|a, b| b.2.total_cmp(&a.2).then(a.0.cmp(&b.0)));
    out.into_iter().map(|(_, key, s)| (key, s)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_in_both_lists_rise_and_scores_follow_the_formula() {
        let text = [1, 2, 3];
        let vec = [3, 4];
        let fused = rrf(&[&text, &vec], RRF_K);
        let keys: Vec<i32> = fused.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, vec![3, 1, 2, 4]);
        let three = 1.0 / 63.0 + 1.0 / 61.0;
        assert!((fused[0].1 - three).abs() < 1e-12);
        assert!((fused[1].1 - 1.0 / 61.0).abs() < 1e-12);
    }

    #[test]
    fn equal_scores_keep_text_order_first_and_repeats_count_once() {
        let text = [10, 10, 11];
        let vec = [12];
        let fused = rrf(&[&text, &vec], RRF_K);
        let keys: Vec<i32> = fused.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, vec![10, 12, 11]);
        assert!((fused[0].1 - fused[1].1).abs() < 1e-12);
    }

    #[test]
    fn empty_lists_give_nothing() {
        let none: [i32; 0] = [];
        assert!(rrf(&[&none, &none], RRF_K).is_empty());
    }
}
