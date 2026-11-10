pub(crate) fn decode(logits: &[f32], steps: usize, classes: usize, dict: &[String]) -> Option<(String, f32)> {
    let mut text = String::new();
    let mut probs = Vec::new();
    let mut last = 0usize;
    for t in 0..steps {
        let row = logits.get(t * classes..(t + 1) * classes)?;
        let (best, p) = row
            .iter()
            .enumerate()
            .fold((0usize, f32::MIN), |acc, (i, v)| if *v > acc.1 { (i, *v) } else { acc });
        if best != 0
            && best != last
            && let Some(ch) = dict.get(best - 1)
        {
            text.push_str(ch);
            probs.push(p);
        }
        last = best;
    }
    if probs.is_empty() {
        return None;
    }
    let score = probs.iter().sum::<f32>() / probs.len() as f32;
    Some((text, score))
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn drops_blanks_and_repeats_and_maps_last_index_to_space() {
        let dict: Vec<String> = ["가", "나", " "].iter().map(|s| s.to_string()).collect();
        let classes = 4;
        let one_hot = |i: usize| {
            let mut row = vec![0.0f32; classes];
            row[i] = 0.9;
            row
        };
        let logits: Vec<f32> = [1, 1, 0, 1, 3, 2].iter().flat_map(|&i| one_hot(i)).collect();
        let (text, score) = decode(&logits, 6, classes, &dict).unwrap_or_default();
        assert_eq!(text, "가가 나");
        assert!((score - 0.9).abs() < 1e-6);
    }
}
