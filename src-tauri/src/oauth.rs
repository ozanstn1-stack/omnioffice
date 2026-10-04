//! OAuth 2.0 PKCE connections for the cloud sync providers.
//!
//! The flow is the one `synccore::oauth` implements: a loopback listener on
//! `127.0.0.1:<random port>`, the system browser, and a code exchange with the
//! PKCE verifier. Tokens are stored in the OS credential vault on the desktop
//! (Windows Credential Manager / macOS Keychain / Secret Service); where no
//! store exists the app's DPAPI-or-plain secret file is used and the UI says
//! so. Client IDs are configured per provider in `oauth.json` (never tokens).

use crate::secret;
use pdfcore::error::{ErrorCode, PdfError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use synccore::oauth::{self, OAuthEndpoints, TokenSet};
use tauri::{AppHandle, Manager};

const OAUTH_CONFIG_FILE: &str = "oauth.json";
const SECRET_SERVICE: &str = "office-swiss-army-knife.sync";
/// Sign-in must finish within this window.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct ProviderConfig {
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
    #[serde(default)]
    pub tenant: String,
    /// Human-readable account label captured at sign-in.
    #[serde(default)]
    pub account: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OAuthConfigFile {
    pub providers: BTreeMap<String, ProviderConfig>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthStatus {
    pub provider: String,
    pub configured: bool,
    pub connected: bool,
    pub account: String,
    /// Configured client id / tenant (never the secret).
    pub client_id: String,
    pub tenant: String,
    /// Where tokens live: `keychain` or `file`.
    pub store: String,
}

fn now_unix() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|value| value.as_secs() as i64).unwrap_or(0)
}

fn normalize_provider(value: &str) -> Result<String, PdfError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "onedrive" | "one-drive" => Ok("onedrive".into()),
        "google-drive" | "google_drive" | "googledrive" => Ok("google-drive".into()),
        other => Err(PdfError::coded(ErrorCode::InvalidInput, format!("Unknown provider: {other}"))),
    }
}

fn endpoints_for(provider: &str, tenant: &str) -> Result<OAuthEndpoints, PdfError> {
    match provider {
        "google-drive" => Ok(oauth::google_endpoints()),
        "onedrive" => Ok(oauth::microsoft_endpoints(tenant)),
        other => Err(PdfError::coded(ErrorCode::InvalidInput, format!("Unknown provider: {other}"))),
    }
}

fn config_path(app: &AppHandle) -> Result<PathBuf, PdfError> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|error| PdfError::Internal(format!("could not resolve the config folder: {error}")))?;
    Ok(dir.join(OAUTH_CONFIG_FILE))
}

pub fn load_config(app: &AppHandle) -> OAuthConfigFile {
    let Ok(path) = config_path(app) else { return OAuthConfigFile::default() };
    let Ok(bytes) = std::fs::read(&path) else { return OAuthConfigFile::default() };
    let text = String::from_utf8_lossy(&bytes);
    let text = text.trim_start_matches('\u{feff}');
    serde_json::from_str(text).unwrap_or_default()
}

fn save_config(app: &AppHandle, config: &OAuthConfigFile) -> Result<(), PdfError> {
    let path = config_path(app)?;
    let bytes = serde_json::to_vec_pretty(config)
        .map_err(|error| PdfError::Internal(format!("could not serialize OAuth config: {error}")))?;
    officecore::io::write_atomic(&path, &bytes)
        .map_err(|error| PdfError::Internal(format!("could not save OAuth config: {error}")))
}

fn token_file(app: &AppHandle, provider: &str) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join(format!("oauth-{provider}.token")))
}

/// The keyring entry for a provider, when the platform has a credential vault.
#[cfg(not(target_os = "android"))]
fn keyring_entry(provider: &str) -> Option<keyring::Entry> {
    keyring::Entry::new(SECRET_SERVICE, &format!("oauth-{provider}")).ok()
}

/// Tries the OS vault first; returns `true` when the value was stored there.
fn store_secret(app: &AppHandle, provider: &str, value: &str) -> bool {
    #[cfg(not(target_os = "android"))]
    {
        if let Some(entry) = keyring_entry(provider) {
            if entry.set_password(value).is_ok() {
                return true;
            }
        }
    }
    let _ = app;
    false
}

fn read_secret(app: &AppHandle, provider: &str) -> Option<String> {
    #[cfg(not(target_os = "android"))]
    {
        if let Some(entry) = keyring_entry(provider) {
            if let Ok(value) = entry.get_password() {
                return Some(value);
            }
        }
    }
    token_file(app, provider).and_then(|path| secret::load_api_key(&path).ok()).filter(|value| !value.is_empty())
}

fn clear_secret(app: &AppHandle, provider: &str) {
    #[cfg(not(target_os = "android"))]
    {
        if let Some(entry) = keyring_entry(provider) {
            let _ = entry.delete_credential();
        }
    }
    if let Some(path) = token_file(app, provider) {
        let _ = secret::delete_api_key(&path);
    }
}

fn save_tokens(app: &AppHandle, provider: &str, tokens: &TokenSet) -> Result<(), PdfError> {
    let json = serde_json::json!({
        "accessToken": tokens.access_token,
        "refreshToken": tokens.refresh_token,
        "tokenType": tokens.token_type,
        "scope": tokens.scope,
        "expiresAt": tokens.expires_at,
    });
    let text = json.to_string();
    if store_secret(app, provider, &text) {
        return Ok(());
    }
    let path = token_file(app, provider)
        .ok_or_else(|| PdfError::Internal("could not resolve the token storage folder".into()))?;
    secret::save_api_key(&path, &text)?;
    Ok(())
}

fn load_tokens(app: &AppHandle, provider: &str) -> Option<TokenSet> {
    let text = read_secret(app, provider)?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(TokenSet {
        access_token: json.get("accessToken")?.as_str()?.to_string(),
        refresh_token: json.get("refreshToken").and_then(|value| value.as_str()).map(str::to_string),
        token_type: json.get("tokenType").and_then(|value| value.as_str()).unwrap_or("Bearer").to_string(),
        scope: json.get("scope").and_then(|value| value.as_str()).map(str::to_string),
        expires_at: json.get("expiresAt").and_then(|value| value.as_i64()),
    })
}

fn token_store_kind(app: &AppHandle, provider: &str) -> &'static str {
    #[cfg(not(target_os = "android"))]
    {
        if keyring_entry(provider).is_some() && read_secret_from_keyring(provider).is_some() {
            return "keychain";
        }
    }
    let _ = app;
    "file"
}

#[cfg(not(target_os = "android"))]
fn read_secret_from_keyring(provider: &str) -> Option<String> {
    keyring_entry(provider)?.get_password().ok()
}

/// Returns a valid access token, refreshing (and persisting) an expired one.
pub(crate) fn access_token(app: &AppHandle, provider: &str) -> Result<String, PdfError> {
    let provider = normalize_provider(provider)?;
    let mut tokens = load_tokens(app, &provider).ok_or_else(|| {
        PdfError::coded(ErrorCode::InvalidInput, format!("Not signed in to {provider}. Connect the account first."))
    })?;
    if tokens.is_expired(now_unix()) {
        let config = load_config(app);
        let provider_config = config.providers.get(&provider).cloned().unwrap_or_default();
        let endpoints = endpoints_for(&provider, &provider_config.tenant)?;
        let client = http_client()?;
        let refresh = tokens.refresh_token.clone().ok_or_else(|| {
            PdfError::coded(
                ErrorCode::InvalidInput,
                format!("The {provider} session expired; connect the account again."),
            )
        })?;
        let refreshed = oauth::refresh_token(
            &client,
            &endpoints,
            &provider_config.client_id,
            Some(provider_config.client_secret.as_str()),
            &refresh,
            now_unix(),
        )
        .map_err(|error| PdfError::Internal(error.to_string()))?;
        let mut refreshed = refreshed;
        if refreshed.refresh_token.is_none() {
            refreshed.refresh_token = tokens.refresh_token.take();
        }
        save_tokens(app, &provider, &refreshed)?;
        tokens = refreshed;
    }
    Ok(tokens.access_token)
}

fn http_client() -> Result<reqwest::blocking::Client, PdfError> {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(60))
        .user_agent("OfficeSwissArmyKnife/3.7 (oauth)")
        .build()
        .map_err(|error| PdfError::Internal(format!("could not build the HTTP client: {error}")))
}

/// A `TokenSource` for the sync providers backed by the stored tokens.
pub(crate) struct AppTokenSource {
    pub app: AppHandle,
    pub provider: String,
}

impl synccore::cloud::TokenSource for AppTokenSource {
    fn access_token(&self) -> Result<String, synccore::SyncError> {
        access_token(&self.app, &self.provider).map_err(|error| synccore::SyncError::Auth(error.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn oauth_status(app: AppHandle) -> Vec<OAuthStatus> {
    let config = load_config(&app);
    ["google-drive", "onedrive"]
        .into_iter()
        .map(|provider| {
            let entry = config.providers.get(provider).cloned().unwrap_or_default();
            let connected = load_tokens(&app, provider).is_some();
            OAuthStatus {
                provider: provider.to_string(),
                configured: !entry.client_id.trim().is_empty(),
                connected,
                account: entry.account,
                client_id: entry.client_id,
                tenant: entry.tenant,
                store: if connected { token_store_kind(&app, provider).to_string() } else { String::new() },
            }
        })
        .collect()
}

#[tauri::command]
pub fn oauth_save_client(
    app: AppHandle,
    provider: String,
    client_id: String,
    client_secret: Option<String>,
    tenant: Option<String>,
) -> Result<Vec<OAuthStatus>, PdfError> {
    let provider = normalize_provider(&provider)?;
    let mut config = load_config(&app);
    let entry = config.providers.entry(provider).or_default();
    entry.client_id = client_id.trim().to_string();
    if let Some(secret) = client_secret {
        entry.client_secret = secret.trim().to_string();
    }
    if let Some(tenant) = tenant {
        entry.tenant = tenant.trim().to_string();
    }
    save_config(&app, &config)?;
    Ok(oauth_status(app))
}

#[tauri::command]
pub async fn oauth_connect(app: AppHandle, provider: String) -> Result<Vec<OAuthStatus>, PdfError> {
    let provider = normalize_provider(&provider)?;
    let config = load_config(&app);
    let provider_config = config.providers.get(&provider).cloned().unwrap_or_default();
    if provider_config.client_id.trim().is_empty() {
        return Err(PdfError::coded(
            ErrorCode::InvalidInput,
            format!("Add an OAuth client ID for {provider} before connecting."),
        ));
    }
    let endpoints = endpoints_for(&provider, &provider_config.tenant)?;
    let server = oauth::LoopbackServer::start().map_err(|error| PdfError::Internal(error.to_string()))?;
    let redirect_uri = server.redirect_uri();
    let pkce = oauth::pkce_pair();
    let auth_url =
        oauth::authorization_url(&endpoints, &provider_config.client_id, &redirect_uri, &server.state, &pkce.challenge);
    let browser_url = auth_url.clone();
    tauri_plugin_opener::open_url(browser_url, None::<&str>)
        .map_err(|error| PdfError::Internal(format!("could not open the sign-in page: {error}")))?;

    let client_id = provider_config.client_id.clone();
    let client_secret = provider_config.client_secret.clone();
    let account_provider = provider.clone();
    let result = tauri::async_runtime::spawn_blocking(move || -> Result<(TokenSet, String), PdfError> {
        let code = server.wait_for_code(SIGN_IN_TIMEOUT).map_err(|error| PdfError::Internal(error.to_string()))?;
        let client = http_client()?;
        let tokens = oauth::exchange_code(
            &client,
            &endpoints,
            &client_id,
            Some(client_secret.as_str()),
            &code,
            &pkce.verifier,
            &redirect_uri,
            now_unix(),
        )
        .map_err(|error| PdfError::Internal(error.to_string()))?;
        let account = fetch_account_label(&client, &account_provider, &tokens.access_token);
        Ok((tokens, account))
    })
    .await
    .map_err(|error| PdfError::Internal(format!("sign-in worker failed: {error}")))??;

    let (tokens, account) = result;
    save_tokens(&app, &provider, &tokens)?;
    let mut config = load_config(&app);
    if let Some(entry) = config.providers.get_mut(&provider) {
        entry.account = account;
    }
    save_config(&app, &config)?;
    Ok(oauth_status(app))
}

#[tauri::command]
pub fn oauth_disconnect(app: AppHandle, provider: String) -> Result<Vec<OAuthStatus>, PdfError> {
    let provider = normalize_provider(&provider)?;
    clear_secret(&app, &provider);
    let mut config = load_config(&app);
    if let Some(entry) = config.providers.get_mut(&provider) {
        entry.account.clear();
    }
    save_config(&app, &config)?;
    Ok(oauth_status(app))
}

/// Best-effort account label for the UI; a failure never fails the sign-in.
fn fetch_account_label(client: &reqwest::blocking::Client, provider: &str, token: &str) -> String {
    let (url, field) = match provider {
        "google-drive" => ("https://www.googleapis.com/oauth2/v3/userinfo", "email"),
        _ => ("https://graph.microsoft.com/v1.0/me", "displayName"),
    };
    let response = client.get(url).bearer_auth(token).send();
    let Ok(response) = response else { return String::new() };
    if !response.status().is_success() {
        return String::new();
    }
    let Ok(body) = response.json::<serde_json::Value>() else { return String::new() };
    body.get(field)
        .and_then(|value| value.as_str())
        .or_else(|| body.get("userPrincipalName").and_then(|value| value.as_str()))
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_names_normalize() {
        assert_eq!(normalize_provider("Google-Drive").unwrap(), "google-drive");
        assert_eq!(normalize_provider("onedrive").unwrap(), "onedrive");
        assert!(normalize_provider("dropbox").is_err());
    }

    #[test]
    fn config_round_trips_without_tokens() {
        let mut config = OAuthConfigFile::default();
        config.providers.insert(
            "onedrive".into(),
            ProviderConfig {
                client_id: "client-1".into(),
                client_secret: "secret-1".into(),
                tenant: "contoso".into(),
                account: "Ada".into(),
            },
        );
        let text = serde_json::to_string(&config).unwrap();
        assert!(!text.contains("accessToken"));
        let back: OAuthConfigFile = serde_json::from_str(&text).unwrap();
        assert_eq!(back.providers["onedrive"].client_id, "client-1");
        assert_eq!(back.providers["onedrive"].tenant, "contoso");
    }
}
