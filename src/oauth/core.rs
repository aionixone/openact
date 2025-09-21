use anyhow::{Result, anyhow};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use oauth2::TokenResponse;
use oauth2::basic::BasicClient;
use oauth2::url::Url;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, PkceCodeVerifier, RedirectUrl, TokenUrl,
};
use rand::RngCore;
use sha2::{Digest, Sha256};

pub struct PkcePair {
    pub code_verifier: String,
    pub code_challenge: String,
}

pub fn generate_state() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn generate_pkce() -> PkcePair {
    let mut verifier_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut verifier_bytes);
    let code_verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);
    let hash = Sha256::digest(code_verifier.as_bytes());
    let code_challenge = URL_SAFE_NO_PAD.encode(hash);
    PkcePair {
        code_verifier,
        code_challenge,
    }
}

pub fn build_authorize_url(
    authorize_url: &str,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    scope: Option<&str>,
    pkce: Option<&PkcePair>,
) -> Result<String> {
    let mut url = Url::parse(authorize_url)?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("response_type", "code");
        qp.append_pair("client_id", client_id);
        qp.append_pair("redirect_uri", redirect_uri);
        qp.append_pair("state", state);
        if let Some(s) = scope {
            qp.append_pair("scope", s);
        }
        if let Some(p) = pkce {
            qp.append_pair("code_challenge_method", "S256");
            qp.append_pair("code_challenge", &p.code_challenge);
        }
    }
    Ok(url.to_string())
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct TokenResponseLite {
    pub access_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

pub async fn exchange_code_for_token(
    token_url: &str,
    client_id: &str,
    client_secret: &str,
    code: &str,
    redirect_uri: &str,
    code_verifier: Option<&str>,
    scope: Option<&str>,
) -> Result<TokenResponseLite> {
    let client = BasicClient::new(
        ClientId::new(client_id.to_string()),
        Some(ClientSecret::new(client_secret.to_string())),
        AuthUrl::new("https://invalid/".to_string()).unwrap(),
        Some(TokenUrl::new(token_url.to_string())?),
    )
    .set_redirect_uri(RedirectUrl::new(redirect_uri.to_string())?);

    let mut req = client.exchange_code(AuthorizationCode::new(code.to_string()));
    if let Some(v) = code_verifier {
        req = req.set_pkce_verifier(PkceCodeVerifier::new(v.to_string()));
    }
    let _ = scope; // scope not added on token request for simplicity (MVP)
    let token = req
        .request_async(oauth2::reqwest::async_http_client)
        .await
        .map_err(|e| anyhow!("token exchange failed: {}", e))?;

    Ok(TokenResponseLite {
        access_token: token.access_token().secret().to_string(),
        refresh_token: token.refresh_token().map(|t| t.secret().to_string()),
        expires_in: token.expires_in().map(|d| d.as_secs() as i64),
        token_type: Some(format!("{:?}", token.token_type())),
        scope: token.scopes().map(|scopes| {
            scopes
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        }),
    })
}
