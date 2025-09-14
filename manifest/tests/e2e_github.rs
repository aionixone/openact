use std::sync::Arc;

use manifest::action::auth::{AuthAdapter, AuthConfig, InjectionConfig};
use manifest::action::{Action, ActionExecutionContext, ActionRunner, ParameterLocation};
use serde_json::json;

fn build_action(connection_trn: &str, base_url: &str) -> Action {
    let mut action = Action::new(
        "getGithubUser".to_string(),
        "GET".to_string(),
        "/user".to_string(),
        "github".to_string(),
        "tenant1".to_string(),
        "trn:openact:tenant1:action/getGithubUser:provider/github".to_string(),
    );
    action.timeout_ms = Some(5000);
    action.ok_path = Some("$status >= 200 and $status < 300".to_string());
    action.output_pick = Some("$body".to_string());
    action
        .extensions
        .insert("x-real-http".to_string(), json!(true));
    action
        .extensions
        .insert("x-base-url".to_string(), json!(base_url));
    action.auth_config = Some(AuthConfig {
        connection_trn: connection_trn.to_string(),
        scheme: Some("oauth2".to_string()),
        injection: InjectionConfig { r#type: "jsonada".to_string(), mapping: "{\n  \"headers\": {\n    \"Authorization\": \"{% 'Bearer ' & $access_token %}\",\n    \"Accept\": \"{% 'application/vnd.github+json' %}\",\n    \"User-Agent\": \"{% 'openact-test/1.0' %}\"\n  }\n}".to_string() },
        expiry: None,
        refresh: None,
        failure: None,
    });
    action.add_parameter(manifest::action::models::ActionParameter::new(
        "id".to_string(),
        ParameterLocation::Path,
    ));
    action
}

#[tokio::test]
#[ignore]
async fn e2e_github_get_user() {
    let db_url = std::env::var("AUTHFLOW_SQLITE_URL").expect("set AUTHFLOW_SQLITE_URL");
    let trn = std::env::var("CONNECTION_TRN").expect("set CONNECTION_TRN");
    let base_url =
        std::env::var("GITHUB_BASE_URL").unwrap_or_else(|_| "https://api.github.com".to_string());

    let mut runner = ActionRunner::new();
    let mut adapter = AuthAdapter::new("tenant1".to_string());
    adapter.init_store_sqlite(db_url, true).await.unwrap();
    runner.set_auth_adapter(Arc::new(adapter));

    let action = build_action(&trn, &base_url);
    let ctx = ActionExecutionContext::new(
        action.trn.clone(),
        "trn:stepflow:tenant1:execution:action-execution:e2e-1".to_string(),
        "tenant1".to_string(),
        "github".to_string(),
    );

    let res = runner.execute_action(&action, ctx).await.unwrap();

    println!("Execution result: {:?}", res);
    println!("Status: {:?}", res.status);
    println!("Response data: {:?}", res.response_data);
    println!("Error message: {:?}", res.error_message);

    // 检查执行状态
    match res.status {
        manifest::action::models::ExecutionStatus::Success => {
            if let Some(data) = res.response_data {
                println!("✅ Action执行成功，有响应数据");
                assert!(data["ok"].as_bool().unwrap_or(true));
                assert!(data["http"].get("body").is_some());
            } else {
                println!("⚠️ Action执行成功，但没有响应数据");
            }
        }
        manifest::action::models::ExecutionStatus::Failed => {
            panic!("Action执行失败: {:?}", res.error_message);
        }
        _ => {
            panic!("意外的执行状态: {:?}", res.status);
        }
    }
}
