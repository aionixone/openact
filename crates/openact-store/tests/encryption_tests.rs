#[cfg(feature = "encryption")]
mod tests {
    use openact_core::{store::AuthConnectionStore, AuthConnection};
    use openact_store::SqlStore;
    use sqlx::sqlite::SqliteConnectOptions;
    use sqlx::{Row, SqlitePool};
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_auth_connection_encryption_roundtrip() {
        // Fixed 32-byte zero key (base64) for reproducibility
        std::env::set_var("OPENACT_ENC_KEY", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=");

        // Use file-backed SQLite so we can verify raw DB content via a separate pool
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("enc_test.sqlite");
        let url = format!("sqlite://{}", db_path.to_string_lossy());

        let store = SqlStore::new(&url).await.unwrap();

        // Prepare an auth connection
        let mut ac = AuthConnection::new("tenantX", "providerY", "userZ", "tok_123");
        ac.update_refresh_token(Some("rt_456".to_string()));
        ac.token_type = "Bearer".to_string();

        // Put and then read back (should return plaintext)
        store.put(&ac.trn, &ac).await.unwrap();
        let fetched = store.get(&ac.trn).await.unwrap().expect("not found");
        assert_eq!(fetched.access_token, "tok_123");
        assert_eq!(fetched.refresh_token.as_deref(), Some("rt_456"));

        // Verify ciphertext stored in DB
        let pool = SqlitePool::connect_with(
            SqliteConnectOptions::new().filename(PathBuf::from(&db_path)).create_if_missing(true),
        )
        .await
        .unwrap();

        let row = sqlx::query(
            "SELECT access_token_encrypted, access_token_nonce, refresh_token_encrypted, refresh_token_nonce FROM auth_connections WHERE trn = ?",
        )
        .bind(&ac.trn)
        .fetch_one(&pool)
        .await
        .unwrap();

        let at_ct: String = row.get("access_token_encrypted");
        let at_nonce: String = row.get("access_token_nonce");
        let rt_ct: Option<String> = row.get("refresh_token_encrypted");
        let rt_nonce: String = row.get("refresh_token_nonce");

        assert_ne!(at_ct, "tok_123", "access token should be encrypted");
        assert!(!at_nonce.is_empty(), "nonce should be set for access token");
        assert!(rt_ct.is_some(), "refresh token ciphertext should be present");
        assert!(!rt_nonce.is_empty(), "nonce should be set for refresh token");
    }
}
