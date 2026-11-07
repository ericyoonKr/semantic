use serde_json::Value;

const REF_SUFFIX: &str = "_ref";

pub(crate) fn resolve(value: &mut Value, texts: &[String]) -> Result<(), ()> {
    match value {
        Value::Object(map) => {
            let refs: Vec<(String, u64)> = map
                .iter()
                .filter_map(|(k, v)| {
                    let base = k.strip_suffix(REF_SUFFIX)?;
                    let idx = v.as_u64()?;
                    (!base.is_empty()).then(|| (k.clone(), idx))
                })
                .collect();
            for (key, idx) in refs {
                let text = usize::try_from(idx)
                    .ok()
                    .and_then(|i| texts.get(i))
                    .ok_or(())?;
                let base = &key[..key.len() - REF_SUFFIX.len()];
                if map.contains_key(base) {
                    return Err(());
                }
                map.remove(&key);
                map.insert(base.to_string(), Value::String(text.clone()));
            }
            for v in map.values_mut() {
                resolve(v, texts)?;
            }
            Ok(())
        }
        Value::Array(items) => {
            for v in items {
                resolve(v, texts)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::resolve;
    use serde_json::json;

    #[test]
    fn replaces_nested_refs_and_rejects_bad_index() {
        let texts = vec!["a".to_string(), "b".to_string()];
        let mut v = json!({"x_ref": 1, "list": [{"y_ref": 0}], "z_ref": "keep"});
        assert_eq!(resolve(&mut v, &texts), Ok(()));
        assert_eq!(v, json!({"x": "b", "list": [{"y": "a"}], "z_ref": "keep"}));
        let mut bad = json!({"x_ref": 2});
        assert_eq!(resolve(&mut bad, &texts), Err(()));
        let mut clash = json!({"x_ref": 0, "x": "c"});
        assert_eq!(resolve(&mut clash, &texts), Err(()));
    }
}
