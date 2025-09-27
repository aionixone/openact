#[cfg(feature = "server")]
pub fn normalize_dsl_json(mut dsl: serde_json::Value) -> serde_json::Value {
    if let Some(provider) = dsl.get_mut("provider") {
        if let Some(flows) = provider.get_mut("flows").and_then(|v| v.as_object_mut()) {
            for (_flow, flow_obj) in flows.iter_mut() {
                if let Some(states) = flow_obj.get_mut("states").and_then(|v| v.as_object_mut()) {
                    for (_sn, state) in states.iter_mut() {
                        let params_exists = state.get("parameters").is_some();
                        if !params_exists {
                            if let Some(mapping) =
                                state.get_mut("mapping").and_then(|v| v.as_object_mut())
                            {
                                if let Some(input) =
                                    mapping.get_mut("input").and_then(|v| v.as_object_mut())
                                {
                                    if let Some(inline) = input.remove("inlineTemplate") {
                                        state
                                            .as_object_mut()
                                            .unwrap()
                                            .insert("parameters".into(), inline);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Provider-level config fan-out to states.parameters
    let provider_config = dsl
        .get("provider")
        .and_then(|p| p.get("config"))
        .cloned()
        .unwrap_or(serde_json::json!({}));
    if provider_config.is_object() {
        if let Some(provider) = dsl.get_mut("provider") {
            if let Some(flows) = provider.get_mut("flows").and_then(|v| v.as_object_mut()) {
                for (_flow, flow_obj) in flows.iter_mut() {
                    if let Some(states) = flow_obj.get_mut("states").and_then(|v| v.as_object_mut())
                    {
                        for (_sn, state) in states.iter_mut() {
                            if let Some(params) =
                                state.get_mut("parameters").and_then(|v| v.as_object_mut())
                            {
                                if let Some(cfg) = provider_config.as_object() {
                                    for (k, v) in cfg.iter() {
                                        if !params.contains_key(k) {
                                            params.insert(k.clone(), v.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    dsl
}
