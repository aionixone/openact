use serde_json::Value;

/// Deep-merge `overlay` into `base` with the following rules:
/// - Objects: merged recursively; keys in overlay overwrite/merge base
/// - Arrays: overlay replaces base entirely
/// - Scalars/null: overlay replaces base
pub fn deep_merge(base: &mut Value, overlay: &Value) {
    match (base, overlay) {
        (Value::Object(base_map), Value::Object(overlay_map)) => {
            for (k, v) in overlay_map {
                match base_map.get_mut(k) {
                    Some(bv) => deep_merge(bv, v),
                    None => {
                        base_map.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        // Arrays: replace
        (b @ Value::Array(_), Value::Array(_)) => {
            *b = overlay.clone();
        }
        // Anything else: replace
        (b, v) => {
            *b = v.clone();
        }
    }
}

/// Build merged config object with precedence (low → high):
/// provider_auth_defaults → provider_defaults → action_extensions → sidecar_overrides
pub fn build_merged_config(
    provider_auth_defaults: &Value,
    provider_defaults: &Value,
    action_extensions: &Value,
    sidecar_overrides: &Value,
) -> Value {
    let mut merged = Value::Object(serde_json::Map::new());
    deep_merge(&mut merged, provider_auth_defaults);
    deep_merge(&mut merged, provider_defaults);
    deep_merge(&mut merged, action_extensions);
    deep_merge(&mut merged, sidecar_overrides);
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_deep_merge_objects_and_arrays() {
        let mut base = json!({
            "a": { "x": 1, "y": [1,2], "z": {"k": true} },
            "b": [1,2,3],
            "c": 10
        });
        let overlay = json!({
            "a": { "x": 2, "y": [9], "z": {"m": false} },
            "b": [4],
            "d": "new"
        });
        deep_merge(&mut base, &overlay);
        assert_eq!(base["a"]["x"], 2);
        // arrays replace
        assert_eq!(base["a"]["y"], json!([9]));
        assert_eq!(base["b"], json!([4]));
        // nested object merged
        assert_eq!(base["a"]["z"]["k"], json!(true));
        assert_eq!(base["a"]["z"]["m"], json!(false));
        // scalar replaced
        assert_eq!(base["c"], json!(10));
        // new key
        assert_eq!(base["d"], json!("new"));
    }

    #[test]
    fn test_build_merged_config_precedence() {
        let pad = json!({"x-auth": {"scheme": "oauth2", "injection": {"type": "jsonada"}}});
        let pd = json!({"x-timeout-ms": 1000, "x-retry": {"max_retries": 1}});
        let action = json!({"x-timeout-ms": 2000, "x-retry": {"max_retries": 2}});
        let sidecar = json!({"x-retry": {"max_retries": 5}});
        let merged = build_merged_config(&pad, &pd, &action, &sidecar);
        assert_eq!(merged["x-auth"]["scheme"], json!("oauth2"));
        assert_eq!(merged["x-timeout-ms"], json!(2000));
        assert_eq!(merged["x-retry"]["max_retries"], json!(5));
    }
}


