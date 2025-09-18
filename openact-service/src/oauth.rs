use anyhow::{Context, Result};
use oauth2::basic::BasicClient;
use oauth2::reqwest::http_client;
use oauth2::{AuthUrl, ClientId, ClientSecret, TokenUrl, AuthorizationCode, RedirectUrl, CsrfToken, PkceCodeChallenge, PkceCodeVerifier, TokenResponse, Scope};
use openact_storage::{config::DatabaseConfig, pool::get_pool, migrate, repos::AuthConnectionRepository, encryption::FieldEncryption, models::AuthConnection};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartAuthorizeArgs {
    pub authorize_url: String,
    pub token_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    #[serde(default)] pub scope: Option<String>,
    #[serde(default)] pub use_pkce: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartAuthorizeResult {
    pub authorize_url: String,
    pub state: String,
    #[serde(default)] pub code_verifier: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeCallbackArgs {
    pub token_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub code: String,
    pub redirect_uri: String,
    pub tenant: String,
    pub provider: String,
    pub user_id: String,
    #[serde(default)] pub code_verifier: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeCallbackResult {
    pub auth_trn: String,
    pub access_token: String,
    #[serde(default)] pub refresh_token: Option<String>,
    #[serde(default)] pub expires_in: Option<i64>,
}

pub fn start_authorize(args: &StartAuthorizeArgs) -> Result<StartAuthorizeResult> {
    let client = BasicClient::new(
        ClientId::new(args.client_id.clone()),
        Some(ClientSecret::new(args.client_secret.clone())),
        AuthUrl::new(args.authorize_url.clone()).context("invalid authorize_url")?,
        Some(TokenUrl::new(args.token_url.clone()).context("invalid token_url")?),
    )
    .set_redirect_uri(RedirectUrl::new(args.redirect_uri.clone()).context("invalid redirect_uri")?);

    let (auth_url, state, code_verifier_opt) = if args.use_pkce {
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let mut req = client.authorize_url(CsrfToken::new_random).set_pkce_challenge(challenge);
        if let Some(scope) = &args.scope { for s in scope.split_whitespace() { req = req.add_scope(Scope::new(s.to_string())); } }
        let (url, state) = req.url();
        (url.to_string(), state.secret().to_string(), Some(verifier.secret().to_string()))
    } else {
        let mut req = client.authorize_url(CsrfToken::new_random);
        if let Some(scope) = &args.scope { for s in scope.split_whitespace() { req = req.add_scope(Scope::new(s.to_string())); } }
        let (url, state) = req.url();
        (url.to_string(), state.secret().to_string(), None)
    };

    Ok(StartAuthorizeResult { authorize_url: auth_url, state, code_verifier: code_verifier_opt })
}

pub async fn resume_callback(args: &ResumeCallbackArgs) -> Result<ResumeCallbackResult> {
    let client = BasicClient::new(
        ClientId::new(args.client_id.clone()),
        Some(ClientSecret::new(args.client_secret.clone())),
        AuthUrl::new("https://invalid.example/auth".to_string()).expect("static"),
        Some(TokenUrl::new(args.token_url.clone()).context("invalid token_url")?),
    ).set_redirect_uri(RedirectUrl::new(args.redirect_uri.clone()).context("invalid redirect_uri")?);

    let mut req = client.exchange_code(AuthorizationCode::new(args.code.clone()));
    if let Some(verifier) = &args.code_verifier { req = req.set_pkce_verifier(PkceCodeVerifier::new(verifier.clone())); }
    let token = req.request(http_client).context("oauth2 authorization_code exchange failed")?;

    let access_token = token.access_token().secret().to_string();
    let refresh_token = token.refresh_token().map(|t| t.secret().to_string());
    let expires_in = token.expires_in().map(|d| d.as_secs() as i64);

    // Persist
    let cfg = DatabaseConfig::from_env();
    let pool = get_pool(&cfg).await?;
    migrate::run(&pool).await?;
    let enc = FieldEncryption::from_env().ok();
    let repo = AuthConnectionRepository::new(pool, enc);
    let trn = format!("trn:authflow:{}:{}/{}@v1", args.tenant, args.provider, args.user_id);
    let model = AuthConnection {
        tenant: args.tenant.clone(),
        provider: args.provider.clone(),
        user_id: args.user_id.clone(),
        access_token: access_token.clone(),
        refresh_token: refresh_token.clone(),
        expires_at: expires_in.map(|sec| chrono::Utc::now() + chrono::Duration::seconds(sec)),
        token_type: "Bearer".to_string(),
        scope: None,
        extra: Value::Null,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    repo.upsert(&trn, &model).await?;

    Ok(ResumeCallbackResult { auth_trn: trn, access_token, refresh_token, expires_in })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindAuthArgs { pub connection_trn: String, pub auth_trn: String }

pub async fn bind_auth_to_connection(args: &BindAuthArgs) -> Result<()> {
    use openact_storage::repos::OpenActConnectionRepository;
    let cfg = DatabaseConfig::from_env();
    let pool = get_pool(&cfg).await?;
    migrate::run(&pool).await?;
    let enc = FieldEncryption::from_env().ok();
    let repo = OpenActConnectionRepository::new(pool, enc);
    if let Some(mut conn) = repo.get(&args.connection_trn).await? {
        conn.auth_ref = Some(args.auth_trn.clone());
        repo.upsert(&conn).await?;
        Ok(())
    } else {
        anyhow::bail!("connection not found: {}", args.connection_trn)
    }
}


