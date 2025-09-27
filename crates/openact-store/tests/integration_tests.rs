use chrono::Utc;
use openact_core::{
    store::{ActionRepository, ConnectionStore, RunStore},
    ActionRecord, Checkpoint, ConnectionRecord, ConnectorKind, Trn,
};
use openact_store::SqlStore;
use serde_json::json;
use tempfile::{tempdir, TempDir};

/// Helper to create a temporary database for testing
async fn create_test_db() -> (SqlStore, TempDir) {
    let dir = tempdir().expect("Failed to create temp dir");
    let db_path = dir.path().join("test.sqlite");
    let db_url = format!("sqlite://{}", db_path.to_string_lossy());
    let store = SqlStore::new(&db_url)
        .await
        .expect("Failed to create test database");
    (store, dir)
}

#[tokio::test]
async fn test_connection_store_crud() {
    let (store, _dir) = create_test_db().await;

    let connection_trn = Trn::new("trn:openact:tenant1:connection/http/github-api@v1".to_string());
    let connection_record = ConnectionRecord {
        trn: connection_trn.clone(),
        connector: ConnectorKind::new("http".to_string()),
        name: "github-api".to_string(),
        config_json: json!({
            "base_url": "https://api.github.com",
            "timeout_ms": 30000,
            "auth": {
                "type": "bearer",
                "token": "${GITHUB_TOKEN}"
            }
        }),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        version: 1,
    };

    // Test upsert (insert)
    ConnectionStore::upsert(&store, &connection_record)
        .await
        .expect("Failed to insert connection");

    // Test get
    let retrieved = ConnectionStore::get(&store, &connection_trn)
        .await
        .expect("Failed to get connection");
    assert!(retrieved.is_some());
    let retrieved_record = retrieved.unwrap();
    assert_eq!(retrieved_record.trn, connection_record.trn);
    assert_eq!(retrieved_record.connector.as_str(), "http");
    assert_eq!(retrieved_record.name, "github-api");
    assert_eq!(
        retrieved_record.config_json["base_url"],
        "https://api.github.com"
    );

    // Test list_by_connector
    let connections = ConnectionStore::list_by_connector(&store, "http")
        .await
        .expect("Failed to list connections");
    assert_eq!(connections.len(), 1);
    assert_eq!(connections[0].trn, connection_record.trn);

    // Test upsert (update)
    let mut updated_record = connection_record.clone();
    updated_record.config_json = json!({
        "base_url": "https://api.github.com",
        "timeout_ms": 60000,  // Updated timeout
        "auth": {
            "type": "bearer",
            "token": "${GITHUB_TOKEN}"
        }
    });
    updated_record.updated_at = Utc::now();
    updated_record.version = 2;

    ConnectionStore::upsert(&store, &updated_record)
        .await
        .expect("Failed to update connection");

    let retrieved_updated = ConnectionStore::get(&store, &connection_trn)
        .await
        .expect("Failed to get updated connection");
    assert!(retrieved_updated.is_some());
    let retrieved_updated_record = retrieved_updated.unwrap();
    assert_eq!(retrieved_updated_record.config_json["timeout_ms"], 60000);
    assert_eq!(retrieved_updated_record.version, 2);

    // Test delete
    let deleted = ConnectionStore::delete(&store, &connection_trn)
        .await
        .expect("Failed to delete connection");
    assert!(deleted);

    let after_delete = ConnectionStore::get(&store, &connection_trn)
        .await
        .expect("Failed to get after delete");
    assert!(after_delete.is_none());

    // Test delete non-existent
    let deleted_again = ConnectionStore::delete(&store, &connection_trn)
        .await
        .expect("Failed to delete non-existent");
    assert!(!deleted_again);
}

#[tokio::test]
async fn test_action_repository_crud() {
    let (store, _dir) = create_test_db().await;

    // First create a connection
    let connection_trn = Trn::new("trn:openact:tenant1:connection/http/github-api@v1".to_string());
    let connection_record = ConnectionRecord {
        trn: connection_trn.clone(),
        connector: ConnectorKind::new("http".to_string()),
        name: "github-api".to_string(),
        config_json: json!({"base_url": "https://api.github.com"}),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        version: 1,
    };
    ConnectionStore::upsert(&store, &connection_record)
        .await
        .expect("Failed to insert connection");

    // Now create an action
    let action_trn = Trn::new("trn:openact:tenant1:action/http/get-user@v1".to_string());
    let action_record = ActionRecord {
        trn: action_trn.clone(),
        connector: ConnectorKind::new("http".to_string()),
        name: "get-user".to_string(),
        connection_trn: connection_trn.clone(),
        config_json: json!({
            "method": "GET",
            "path": "/user",
            "headers": {
                "Accept": "application/json"
            }
        }),
        mcp_enabled: false,
        mcp_overrides: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        version: 1,
    };

    // Test upsert (insert)
    ActionRepository::upsert(&store, &action_record)
        .await
        .expect("Failed to insert action");

    // Test get
    let retrieved = ActionRepository::get(&store, &action_trn)
        .await
        .expect("Failed to get action");
    assert!(retrieved.is_some());
    let retrieved_record = retrieved.unwrap();
    assert_eq!(retrieved_record.trn, action_record.trn);
    assert_eq!(retrieved_record.connector.as_str(), "http");
    assert_eq!(retrieved_record.name, "get-user");
    assert_eq!(retrieved_record.connection_trn, connection_trn);
    assert_eq!(retrieved_record.config_json["method"], "GET");
    assert_eq!(retrieved_record.config_json["path"], "/user");

    // Test list_by_connection
    let actions = ActionRepository::list_by_connection(&store, &connection_trn)
        .await
        .expect("Failed to list actions");
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].trn, action_record.trn);

    // Test upsert (update)
    let mut updated_record = action_record.clone();
    updated_record.config_json = json!({
        "method": "GET",
        "path": "/user/profile",  // Updated path
        "headers": {
            "Accept": "application/json"
        }
    });
    updated_record.updated_at = Utc::now();
    updated_record.version = 2;

    ActionRepository::upsert(&store, &updated_record)
        .await
        .expect("Failed to update action");

    let retrieved_updated = ActionRepository::get(&store, &action_trn)
        .await
        .expect("Failed to get updated action");
    assert!(retrieved_updated.is_some());
    let retrieved_updated_record = retrieved_updated.unwrap();
    assert_eq!(
        retrieved_updated_record.config_json["path"],
        "/user/profile"
    );
    assert_eq!(retrieved_updated_record.version, 2);

    // Test delete
    let deleted = ActionRepository::delete(&store, &action_trn)
        .await
        .expect("Failed to delete action");
    assert!(deleted);

    let after_delete = ActionRepository::get(&store, &action_trn)
        .await
        .expect("Failed to get after delete");
    assert!(after_delete.is_none());

    // Test delete non-existent
    let deleted_again = ActionRepository::delete(&store, &action_trn)
        .await
        .expect("Failed to delete non-existent");
    assert!(!deleted_again);
}

#[tokio::test]
async fn test_run_store_crud() {
    let (store, _dir) = create_test_db().await;

    let checkpoint = Checkpoint {
        run_id: "test-run-123".to_string(),
        paused_state: "waiting-for-oauth".to_string(),
        context_json: json!({
            "tenant": "tenant1",
            "user_id": "user123",
            "provider": "github",
            "flow_step": "authorization"
        }),
        await_meta_json: Some(json!({
            "state": "random-state-abc",
            "code_verifier": "test-verifier"
        })),
    };

    // Test put (insert)
    RunStore::put(&store, checkpoint.clone())
        .await
        .expect("Failed to put checkpoint");

    // Test get
    let retrieved = RunStore::get(&store, &checkpoint.run_id)
        .await
        .expect("Failed to get checkpoint");
    assert!(retrieved.is_some());
    let retrieved_checkpoint = retrieved.unwrap();
    assert_eq!(retrieved_checkpoint.run_id, checkpoint.run_id);
    assert_eq!(retrieved_checkpoint.paused_state, checkpoint.paused_state);
    assert_eq!(retrieved_checkpoint.context_json["tenant"], "tenant1");
    assert_eq!(
        retrieved_checkpoint.context_json["flow_step"],
        "authorization"
    );
    assert!(retrieved_checkpoint.await_meta_json.is_some());
    assert_eq!(
        retrieved_checkpoint.await_meta_json.as_ref().unwrap()["state"],
        "random-state-abc"
    );

    // Test put (update)
    let updated_checkpoint = Checkpoint {
        run_id: checkpoint.run_id.clone(),
        paused_state: "waiting-for-token".to_string(),
        context_json: json!({
            "tenant": "tenant1",
            "user_id": "user123",
            "provider": "github",
            "flow_step": "token_exchange"  // Updated step
        }),
        await_meta_json: None, // No longer waiting
    };

    RunStore::put(&store, updated_checkpoint.clone())
        .await
        .expect("Failed to update checkpoint");

    let retrieved_updated = RunStore::get(&store, &checkpoint.run_id)
        .await
        .expect("Failed to get updated checkpoint");
    assert!(retrieved_updated.is_some());
    let retrieved_updated_checkpoint = retrieved_updated.unwrap();
    assert_eq!(
        retrieved_updated_checkpoint.paused_state,
        "waiting-for-token"
    );
    assert_eq!(
        retrieved_updated_checkpoint.context_json["flow_step"],
        "token_exchange"
    );
    assert!(retrieved_updated_checkpoint.await_meta_json.is_none());

    // Test delete
    let deleted = RunStore::delete(&store, &checkpoint.run_id)
        .await
        .expect("Failed to delete checkpoint");
    assert!(deleted);

    let after_delete = RunStore::get(&store, &checkpoint.run_id)
        .await
        .expect("Failed to get after delete");
    assert!(after_delete.is_none());

    // Test delete non-existent
    let deleted_again = RunStore::delete(&store, &checkpoint.run_id)
        .await
        .expect("Failed to delete non-existent");
    assert!(!deleted_again);
}

#[tokio::test]
async fn test_foreign_key_constraint() {
    let (store, _dir) = create_test_db().await;

    // Try to create an action without a corresponding connection
    let non_existent_connection_trn =
        Trn::new("trn:openact:tenant1:connection/http/non-existent@v1".to_string());
    let action_trn = Trn::new("trn:openact:tenant1:action/http/orphan-action@v1".to_string());
    let action_record = ActionRecord {
        trn: action_trn,
        connector: ConnectorKind::new("http".to_string()),
        name: "orphan-action".to_string(),
        connection_trn: non_existent_connection_trn,
        config_json: json!({"method": "GET", "path": "/test"}),
        mcp_enabled: false,
        mcp_overrides: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        version: 1,
    };

    // This should fail due to foreign key constraint
    let result = ActionRepository::upsert(&store, &action_record).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_unique_constraints() {
    let (store, _dir) = create_test_db().await;

    // Create first connection
    let connection_record1 = ConnectionRecord {
        trn: Trn::new("trn:openact:tenant1:connection/http/api1@v1".to_string()),
        connector: ConnectorKind::new("http".to_string()),
        name: "api1".to_string(),
        config_json: json!({"base_url": "https://api1.example.com"}),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        version: 1,
    };
    ConnectionStore::upsert(&store, &connection_record1)
        .await
        .expect("Failed to insert first connection");

    // Try to create another connection with same connector+name combination
    let connection_record2 = ConnectionRecord {
        trn: Trn::new("trn:openact:tenant1:connection/http/api1@v2".to_string()), // Different TRN
        connector: ConnectorKind::new("http".to_string()),                        // Same connector
        name: "api1".to_string(),                                                 // Same name
        config_json: json!({"base_url": "https://api2.example.com"}),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        version: 1,
    };

    // This should fail due to unique constraint on (connector, name)
    let result = ConnectionStore::upsert(&store, &connection_record2).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_migration_idempotency() {
    let dir = tempdir().expect("Failed to create temp dir");
    let db_path = dir.path().join("idempotent.sqlite");
    let db_url = format!("sqlite://{}", db_path.to_string_lossy());

    // Create first store (runs migrations)
    let store1 = SqlStore::new(&db_url)
        .await
        .expect("Failed to create first store");

    // Create second store with same database (should not fail)
    let store2 = SqlStore::new(&db_url)
        .await
        .expect("Failed to create second store");

    // Manually run migrations again (should be idempotent)
    store1
        .migrate()
        .await
        .expect("Failed to run migrations again");
    store2
        .migrate()
        .await
        .expect("Failed to run migrations on second store");

    // Verify database still works
    let connection_record = ConnectionRecord {
        trn: Trn::new("trn:openact:tenant1:connection/http/test@v1".to_string()),
        connector: ConnectorKind::new("http".to_string()),
        name: "test".to_string(),
        config_json: json!({"test": "value"}),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        version: 1,
    };

    ConnectionStore::upsert(&store1, &connection_record)
        .await
        .expect("Failed to insert after migrations");

    let retrieved = ConnectionStore::get(&store2, &connection_record.trn)
        .await
        .expect("Failed to get from second store");
    assert!(retrieved.is_some());
}
