#![cfg(feature = "store-encryption")]

use httpmock::prelude::*;
use openact_authflow::engine::{run_flow, TaskHandler};
use openact_core::{store::AuthConnectionStore, AuthConnection};
use openact_store::SqlStore;
use serde_json::json;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Row, SqlitePool};
use std::path::PathBuf;
use stepflow_dsl::WorkflowDSL;
use tempfile;

#[derive(Clone)]
struct Router;

impl TaskHandler for Router {
    fn execute(
        &self,
        resource: &str,
        state_name: &str,
        ctx: &serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        match resource {
            "http.request" => {
                openact_authflow::actions::HttpTaskHandler.execute(resource, state_name, ctx)
            }
            _ => anyhow::bail!("unknown resource {resource}"),
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn end_to_end_http_with_encrypted_auth_store() {
    std::env::set_var(
        "OPENACT_ENC_KEY",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
    );

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("enc_authflow.sqlite");
    let url = format!("sqlite://{}", db_path.to_string_lossy());

    let store = SqlStore::new(&url).await.unwrap();

    // Seed an access token and ensure it's stored encrypted
    let mut ac = AuthConnection::new("t1", "prov", "u1", "tok_abc");
    ac.update_refresh_token(Some("rt_def".into()));
    store.put(&ac.trn, &ac).await.unwrap();

    // Verify ciphertext in db
    let pool = SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename(PathBuf::from(&db_path))
            .create_if_missing(true),
    )
    .await
    .unwrap();
    let row = sqlx::query(
        "SELECT access_token_encrypted, access_token_nonce FROM auth_connections WHERE trn = ?",
    )
    .bind(&ac.trn)
    .fetch_one(&pool)
    .await
    .unwrap();
    let at_ct: String = row.get("access_token_encrypted");
    assert_ne!(at_ct, "tok_abc");

    // Mock API requiring Bearer token
    let server = MockServer::start();
    let m = server.mock(|when, then| {
        when.method(GET)
            .path("/secure")
            .header("authorization", "Bearer tok_abc");
        then.status(200).json_body(json!({"ok": true}));
    });

    // Simple flow: inject header and call API
    let yaml = format!(
        r#"
startAt: "Call"
states:
  Call:
    type: task
    resource: "http.request"
    parameters:
      method: "GET"
      url: "{}{}"
      headers:
        authorization: "Bearer {}"
    output: "{{% result.status %}}"
    end: true
"#,
        server.base_url(),
        "/secure",
        "tok_abc"
    );

    let dsl: WorkflowDSL = serde_yaml::from_str(&yaml).unwrap();
    let out = run_flow(&dsl, &dsl.start_at, json!({}), &Router, 20).unwrap();
    m.assert();
    assert_eq!(out["states"]["Call"]["result"], json!(200));
}
