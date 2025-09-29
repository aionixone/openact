use anyhow::{Context, Result};
use oauth2::basic::BasicClient;
use oauth2::reqwest::http_client;
use oauth2::{AuthUrl, ClientId, ClientSecret, Scope, TokenResponse, TokenUrl};
use serde_json::{json, Value};

use crate::engine::TaskHandler;

#[derive(Default)]
pub struct OAuth2ClientCredentialsHandler;

impl TaskHandler for OAuth2ClientCredentialsHandler {
    fn execute(&self, _resource: &str, _state_name: &str, ctx: &Value) -> Result<Value> {
        // ctx from mapping.input: { tokenUrl, clientId, clientSecret, scopes?: [..], extra?: {audience?:..} }
        let token_url =
            ctx.get("tokenUrl").and_then(|v| v.as_str()).context("tokenUrl required")?;
        let client_id =
            ctx.get("clientId").and_then(|v| v.as_str()).context("clientId required")?;
        let client_secret =
            ctx.get("clientSecret").and_then(|v| v.as_str()).context("clientSecret required")?;

        let client = BasicClient::new(
            ClientId::new(client_id.to_string()),
            Some(ClientSecret::new(client_secret.to_string())),
            AuthUrl::new("https://invalid.example/auth".to_string()).expect("static"),
            Some(TokenUrl::new(token_url.to_string()).context("invalid tokenUrl")?),
        );

        let mut req = client.exchange_client_credentials();
        if let Some(scopes) = ctx.get("scopes").and_then(|v| v.as_array()) {
            for s in scopes.iter().filter_map(|x| x.as_str()) {
                req = req.add_scope(Scope::new(s.to_string()));
            }
        }
        let token = req.request(http_client).context("oauth2 client_credentials request failed")?;

        let access_token = token.access_token().secret().to_string();
        let refresh_token = token.refresh_token().map(|t| t.secret().to_string());
        let expires_in = token.expires_in().map(|d| d.as_secs() as i64);

        Ok(json!({
            "access_token": access_token,
            "refresh_token": refresh_token,
            "expires_in": expires_in,
            "token_type": token.token_type().as_ref(),
            "scopes": token.scopes().map(|s| s.iter().map(|x| x.as_str().to_string()).collect::<Vec<_>>())
        }))
    }
}
