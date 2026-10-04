//! OAuth 2.0 Authorization Code + PKCE (RFC 7636) for Google Drive and
//! Microsoft Graph.
//!
//! The flow is built for a local-first desktop app:
//!
//! 1. the app binds a loopback listener on `127.0.0.1:0` (a random free port),
//! 2. it opens the provider's authorization URL in the system browser,
//! 3. the provider redirects back to `http://127.0.0.1:<port>/callback`, the
//!    listener answers the browser and hands the code to the app,
//! 4. the code is exchanged for tokens with the PKCE verifier (no client
//!    secret is required for the built-in default clients, though Google
//!    desktop clients may configure one).
//!
//! Token persistence and refresh scheduling live in the application layer;
//! this module is pure and testable. It never logs token values.

use crate::SyncError;
use base64::Engine;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

const B64: base64::engine::general_purpose::GeneralPurpose = base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// A PKCE verifier and its S256 challenge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

/// Generates a fresh PKCE pair. The verifier is 64 random bytes encoded as
/// base64url (86 characters, inside the 43..=128 range RFC 7636 allows).
pub fn pkce_pair() -> Pkce {
    let mut bytes = [0u8; 64];
    fill_random(&mut bytes);
    let verifier = B64.encode(bytes);
    let challenge = pkce_challenge(&verifier);
    Pkce { verifier, challenge }
}

/// S256 challenge for an existing verifier (exposed for tests and for
/// re-running a flow after a restart).
pub fn pkce_challenge(verifier: &str) -> String {
    B64.encode(Sha256::digest(verifier.as_bytes()))
}

/// Fills the buffer from the platform RNG via UUID v4 entropy. Four UUIDs give
/// 64 bytes; `uuid` uses the OS entropy source.
fn fill_random(bytes: &mut [u8]) {
    let mut offset = 0;
    while offset < bytes.len() {
        let uuid = uuid::Uuid::new_v4();
        let chunk = uuid.as_bytes();
        let take = (bytes.len() - offset).min(chunk.len());
        bytes[offset..offset + take].copy_from_slice(&chunk[..take]);
        offset += take;
    }
}

/// A random `state` value that the callback must echo back.
pub fn random_state() -> String {
    let mut bytes = [0u8; 24];
    fill_random(&mut bytes);
    B64.encode(bytes)
}

/// Provider endpoints and the scope the app requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthEndpoints {
    pub authorize_url: String,
    pub token_url: String,
    pub scope: String,
}

/// Google Drive (offline access so a refresh token is issued).
pub fn google_endpoints() -> OAuthEndpoints {
    OAuthEndpoints {
        authorize_url: "https://accounts.google.com/o/oauth2/v2/auth".into(),
        token_url: "https://oauth2.googleapis.com/token".into(),
        scope: "https://www.googleapis.com/auth/drive.file openid email profile".into(),
    }
}

/// Microsoft identity platform, `common` tenant by default.
pub fn microsoft_endpoints(tenant: &str) -> OAuthEndpoints {
    let tenant = if tenant.trim().is_empty() { "common" } else { tenant.trim() };
    OAuthEndpoints {
        authorize_url: format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0/authorize"),
        token_url: format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token"),
        scope: "offline_access Files.ReadWrite User.Read".into(),
    }
}

/// Builds the authorization URL the browser is opened with.
pub fn authorization_url(
    endpoints: &OAuthEndpoints,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    challenge: &str,
) -> String {
    let mut url = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
        endpoints.authorize_url,
        urlencode(client_id),
        urlencode(redirect_uri),
        urlencode(&endpoints.scope),
        urlencode(state),
        urlencode(challenge),
    );
    // Google needs these two to hand out a refresh token and to show the
    // consent screen when scopes changed.
    if endpoints.authorize_url.contains("accounts.google.com") {
        url.push_str("&access_type=offline&prompt=consent");
    }
    url
}

/// Query-value characters that must be percent-encoded. `-._~` stay readable,
/// which keeps the state and challenge values in their familiar form; the
/// redirect URI's `:` and `/` are encoded so the value cannot split the query.
const QUERY_ENCODE: &percent_encoding::AsciiSet = &percent_encoding::CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'&')
    .add(b'+')
    .add(b'=')
    .add(b'?')
    .add(b':')
    .add(b'/')
    .add(b'<')
    .add(b'>')
    .add(b'\\')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

fn urlencode(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value, QUERY_ENCODE).to_string()
}

/// Tokens as returned by the providers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub token_type: String,
    pub scope: Option<String>,
    /// Unix seconds at which the access token expires.
    pub expires_at: Option<i64>,
}

impl TokenSet {
    /// Parses the token endpoint's JSON response. `now` is Unix seconds.
    pub fn parse(json: &serde_json::Value, now: i64) -> Result<Self, SyncError> {
        let access_token = json
            .get("access_token")
            .and_then(|value| value.as_str())
            .ok_or_else(|| SyncError::Auth("the token response has no access_token".into()))?
            .to_string();
        let expires_at = json.get("expires_in").and_then(|value| value.as_i64()).map(|seconds| now + seconds - 30); // refresh a little early
        Ok(TokenSet {
            access_token,
            refresh_token: json.get("refresh_token").and_then(|value| value.as_str()).map(str::to_string),
            token_type: json.get("token_type").and_then(|value| value.as_str()).unwrap_or("Bearer").to_string(),
            scope: json.get("scope").and_then(|value| value.as_str()).map(str::to_string),
            expires_at,
        })
    }

    pub fn is_expired(&self, now: i64) -> bool {
        self.expires_at.map(|expiry| expiry <= now).unwrap_or(false)
    }
}

/// Form body shared by the code exchange and the refresh grant.
#[allow(clippy::too_many_arguments)]
pub fn token_form(
    grant: &str,
    code: &str,
    redirect_uri: &str,
    client_id: &str,
    client_secret: Option<&str>,
    verifier: &str,
) -> Vec<(&'static str, String)> {
    let mut form = vec![
        ("grant_type", grant.to_string()),
        ("client_id", client_id.to_string()),
        ("redirect_uri", redirect_uri.to_string()),
    ];
    if verifier.is_empty() {
        form.push(("refresh_token", code.to_string()));
    } else {
        form.push(("code", code.to_string()));
        form.push(("code_verifier", verifier.to_string()));
    }
    if let Some(secret) = client_secret.filter(|value| !value.trim().is_empty()) {
        form.push(("client_secret", secret.to_string()));
    }
    form
}

/// A loopback redirect listener for the authorization code.
pub struct LoopbackServer {
    listener: TcpListener,
    pub state: String,
    pub port: u16,
}

impl LoopbackServer {
    pub fn start() -> Result<Self, SyncError> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|error| SyncError::Io(format!("could not bind the OAuth loopback listener: {error}")))?;
        listener
            .set_nonblocking(true)
            .map_err(|error| SyncError::Io(format!("could not configure the OAuth listener: {error}")))?;
        let port = listener
            .local_addr()
            .map_err(|error| SyncError::Io(format!("could not read the OAuth listener address: {error}")))?
            .port();
        Ok(Self { listener, state: random_state(), port })
    }

    pub fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}/callback", self.port)
    }

    /// Waits for the browser to hit `/callback`, verifies the state and
    /// returns the authorization code. The browser receives a small closing
    /// page in every case.
    pub fn wait_for_code(self, timeout: Duration) -> Result<String, SyncError> {
        let started = Instant::now();
        loop {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    let response = read_callback(stream, &self.state);
                    return response;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if started.elapsed() > timeout {
                        return Err(SyncError::Auth("the sign-in window was not completed in time".into()));
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(error) => return Err(SyncError::Io(format!("OAuth loopback accept failed: {error}"))),
            }
        }
    }
}

/// Reads one HTTP request line, answers the browser and extracts the code.
fn read_callback(mut stream: TcpStream, expected_state: &str) -> Result<String, SyncError> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buffer = [0u8; 4096];
    let read = stream.read(&mut buffer).unwrap_or(0);
    let request = String::from_utf8_lossy(&buffer[..read]);
    let first_line = request.lines().next().unwrap_or_default();
    // "GET /callback?code=...&state=... HTTP/1.1"
    let target = first_line.split_whitespace().nth(1).unwrap_or_default();
    let query = target.split_once('?').map(|(_, query)| query).unwrap_or_default();
    let mut code: Option<String> = None;
    let mut state: Option<String> = None;
    let mut error: Option<String> = None;
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else { continue };
        let decoded = percent_encoding::percent_decode_str(value).decode_utf8_lossy().to_string();
        match key {
            "code" => code = Some(decoded),
            "state" => state = Some(decoded),
            "error" => error = Some(decoded),
            _ => {}
        }
    }

    let (status, title, body) = if let Some(ref error) = error {
        ("400 Bad Request", "Sign-in was cancelled", format!("The provider returned: {error}"))
    } else if state.as_deref() != Some(expected_state) {
        ("400 Bad Request", "Sign-in could not be verified", "The state parameter did not match.".to_string())
    } else if code.is_none() {
        ("400 Bad Request", "Sign-in could not be verified", "The callback had no code.".to_string())
    } else {
        ("200 OK", "Sign-in complete", "You can close this window and return to the app.".to_string())
    };
    let page = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title></head><body style=\"font-family: sans-serif; padding: 2rem\"><h1>{title}</h1><p>{body}</p></body></html>"
    );
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
        page.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();

    if error.is_some() {
        return Err(SyncError::Auth("the sign-in was cancelled".into()));
    }
    if state.as_deref() != Some(expected_state) {
        return Err(SyncError::Auth("the OAuth state parameter did not match; the request was refused".into()));
    }
    code.ok_or_else(|| SyncError::Auth("the OAuth callback carried no authorization code".into()))
}

/// Exchanges an authorization code for tokens.
pub fn exchange_code(
    client: &reqwest::blocking::Client,
    endpoints: &OAuthEndpoints,
    client_id: &str,
    client_secret: Option<&str>,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
    now: i64,
) -> Result<TokenSet, SyncError> {
    let form = token_form("authorization_code", code, redirect_uri, client_id, client_secret, verifier);
    post_token(client, &endpoints.token_url, &form, now)
}

/// Refreshes an expired access token.
pub fn refresh_token(
    client: &reqwest::blocking::Client,
    endpoints: &OAuthEndpoints,
    client_id: &str,
    client_secret: Option<&str>,
    refresh_token: &str,
    now: i64,
) -> Result<TokenSet, SyncError> {
    let form = token_form("refresh_token", refresh_token, "", client_id, client_secret, "");
    post_token(client, &endpoints.token_url, &form, now)
}

fn post_token(
    client: &reqwest::blocking::Client,
    token_url: &str,
    form: &[(&'static str, String)],
    now: i64,
) -> Result<TokenSet, SyncError> {
    let response = client
        .post(token_url)
        .form(form)
        .header("Accept", "application/json")
        .send()
        .map_err(|error| SyncError::Network(format!("token endpoint unreachable: {error}")))?;
    let status = response.status();
    let body = response.text().unwrap_or_default();
    if !status.is_success() {
        // The provider's error body never contains our tokens; cap it anyway.
        let detail: String = body.chars().take(300).collect();
        return Err(SyncError::Auth(format!("token endpoint returned HTTP {status}: {detail}")));
    }
    let json: serde_json::Value =
        serde_json::from_str(&body).map_err(|error| SyncError::Protocol(format!("invalid token response: {error}")))?;
    TokenSet::parse(&json, now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_matches_the_rfc7636_vector() {
        // RFC 7636 appendix B.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(pkce_challenge(verifier), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        let pair = pkce_pair();
        assert_eq!(pair.verifier.len(), 86);
        assert_eq!(pair.challenge, pkce_challenge(&pair.verifier));
        assert_ne!(pair.verifier, pkce_pair().verifier);
    }

    #[test]
    fn authorization_url_carries_the_pkce_parameters() {
        let endpoints = google_endpoints();
        let url = authorization_url(&endpoints, "client-1", "http://127.0.0.1:1234/callback", "state-1", "chal-1");
        assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
        assert!(url.contains("code_challenge=chal-1"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("state=state-1"));
        assert!(url.contains("access_type=offline"));
        let microsoft = authorization_url(
            &microsoft_endpoints("common"),
            "client-2",
            "http://127.0.0.1:1234/callback",
            "state-2",
            "chal-2",
        );
        assert!(microsoft.starts_with("https://login.microsoftonline.com/common/oauth2/v2.0/authorize?"));
        assert!(!microsoft.contains("access_type=offline"));
    }

    #[test]
    fn token_response_parses_and_expires() {
        let json = serde_json::json!({
            "access_token": "at",
            "refresh_token": "rt",
            "token_type": "Bearer",
            "expires_in": 3600,
            "scope": "a b"
        });
        let tokens = TokenSet::parse(&json, 1000).expect("parse");
        assert_eq!(tokens.access_token, "at");
        assert_eq!(tokens.refresh_token.as_deref(), Some("rt"));
        assert_eq!(tokens.expires_at, Some(1000 + 3600 - 30));
        assert!(!tokens.is_expired(1000));
        assert!(tokens.is_expired(5000));
        assert!(TokenSet::parse(&serde_json::json!({}), 0).is_err());
    }

    #[test]
    fn loopback_server_receives_the_code() {
        let server = LoopbackServer::start().expect("start");
        let port = server.port;
        let state = server.state.clone();
        let handle = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
            let request = format!(
                "GET /callback?code=abc&state={state} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
            );
            stream.write_all(request.as_bytes()).expect("write");
            let mut response = String::new();
            let _ = stream.read_to_string(&mut response);
            response
        });
        let code = server.wait_for_code(Duration::from_secs(5)).expect("code");
        assert_eq!(code, "abc");
        let response = handle.join().expect("join");
        assert!(response.contains("200 OK"));
    }

    #[test]
    fn loopback_server_refuses_a_wrong_state() {
        let server = LoopbackServer::start().expect("start");
        let port = server.port;
        let handle = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
            let request = "GET /callback?code=abc&state=wrong HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
            let _ = stream.write_all(request.as_bytes());
            let mut response = String::new();
            let _ = stream.read_to_string(&mut response);
            response
        });
        let error = server.wait_for_code(Duration::from_secs(5)).expect_err("must refuse");
        assert!(format!("{error}").contains("state"));
        let response = handle.join().expect("join");
        assert!(response.contains("400 Bad Request"));
    }
}
