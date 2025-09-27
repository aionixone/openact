use anyhow::{Context, Result};
use oauth2::basic::BasicClient;
use oauth2::reqwest::http_client;
use oauth2::{AuthUrl, ClientId, ClientSecret, RefreshToken, TokenResponse, TokenUrl};
use serde_json::{json, Value};

use crate::engine::TaskHandler;

#[derive(Default)]
pub struct OAuth2RefreshTokenHandler;

impl TaskHandler for OAuth2RefreshTokenHandler {
    fn execute(&self, _resource: &str, _state_name: &str, ctx: &Value) -> Result<Value> {
        // ctx: { tokenUrl, clientId, clientSecret, refresh_token }
        let token_url = ctx
            .get("tokenUrl")
            .and_then(|v| v.as_str())
            .context("tokenUrl required")?;
        let client_id = ctx
            .get("clientId")
            .and_then(|v| v.as_str())
            .context("clientId required")?;
        let client_secret = ctx
            .get("clientSecret")
            .and_then(|v| v.as_str())
            .context("clientSecret required")?;
        let refresh_token = ctx
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .context("refresh_token required")?;

        let client = BasicClient::new(
            ClientId::new(client_id.to_string()),
            Some(ClientSecret::new(client_secret.to_string())),
            AuthUrl::new("https://invalid.example/auth".to_string()).expect("static"),
            Some(TokenUrl::new(token_url.to_string()).context("invalid tokenUrl")?),
        );

        let rt = RefreshToken::new(refresh_token.to_string());
        let req = client.exchange_refresh_token(&rt);
        let token = req
            .request(http_client)
            .context("oauth2 refresh_token request failed")?;

        let access_token = token.access_token().secret().to_string();
        let new_refresh = token.refresh_token().map(|t| t.secret().to_string());
        let expires_in = token.expires_in().map(|d| d.as_secs() as i64);

        Ok(json!({
            "access_token": access_token,
            "refresh_token": new_refresh,
            "expires_in": expires_in,
            "token_type": token.token_type().as_ref(),
        }))
    }
}
