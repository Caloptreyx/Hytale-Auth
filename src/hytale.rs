//! Hytale's OAuth and session services, as documented in the Server Provider Authentication
//! Guide: the device code flow of the `hytale-server` client, token refreshes (both tokens
//! rotate on every refresh), the account's game profiles and game sessions.
//!
//! Tokens are [`Secret`]s: their `Debug` output is redacted and they are never serialized, so
//! they cannot end up in logs or API responses by accident.
use serde::Deserialize;
use std::{fmt, time::Duration};

pub const CLIENT_ID: &str = "hytale-server";
pub const SCOPE: &str = "openid offline auth:server";

const DEVICE_AUTH_URL: &str = "https://oauth.accounts.hytale.com/oauth2/device/auth";
const TOKEN_URL: &str = "https://oauth.accounts.hytale.com/oauth2/token";
const PROFILES_URL: &str = "https://account-data.hytale.com/my-account/get-profiles";
const NEW_SESSION_URL: &str = "https://sessions.hytale.com/game-session/new";
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

const TIMEOUT: Duration = Duration::from_secs(10);
/// Longest error text kept from Hytale's answers.
const MAX_MESSAGE: usize = 300;

/// A token or device code. Never printed, never serialized.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(**redacted**)")
    }
}

impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Secret)
    }
}

/// What went wrong talking to Hytale, in the terms the panel shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HytaleError {
    /// The refresh token expired (30 days unused) or was revoked: the account must be linked again.
    Relink(String),
    /// 403 on a game session: the account's concurrent server sessions are used up, or it has no
    /// server entitlement.
    Limit(String),
    /// The selected game profile does not exist (any more).
    Profile(String),
    /// The account has no game profile at all.
    NoProfile,
    /// Hytale could not be reached or answered with an error of its own.
    Unavailable(String),
}

impl HytaleError {
    /// The code the frontend translates.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Relink(_) => "relink",
            Self::Limit(_) => "limit",
            Self::Profile(_) => "profile",
            Self::NoProfile => "no_profile",
            Self::Unavailable(_) => "unavailable",
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Relink(message)
            | Self::Limit(message)
            | Self::Profile(message)
            | Self::Unavailable(message) => message,
            Self::NoProfile => "the Hytale account has no game profile",
        }
    }
}

impl fmt::Display for HytaleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code(), self.message())
    }
}

impl std::error::Error for HytaleError {}

fn unavailable(err: reqwest::Error) -> HytaleError {
    // reqwest errors carry the URL (no tokens: those travel in bodies and headers)
    HytaleError::Unavailable(if err.is_timeout() {
        "Hytale did not answer in time".into()
    } else {
        format!("could not reach Hytale: {}", err.without_url())
    })
}

fn truncate(text: &str) -> String {
    let text = text.trim();
    match text.char_indices().nth(MAX_MESSAGE) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_string(),
    }
}

/// The answer to a device code request (RFC 8628).
#[derive(Debug, Deserialize)]
pub struct DeviceAuthorization {
    pub device_code: Secret,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    pub expires_in: u64,
    #[serde(default = "default_interval")]
    pub interval: u64,
}

fn default_interval() -> u64 {
    5
}

pub fn parse_device_authorization(
    status: u16,
    body: &str,
) -> Result<DeviceAuthorization, HytaleError> {
    if !(200..300).contains(&status) {
        return Err(HytaleError::Unavailable(oauth_error_text(status, body)));
    }
    let authorization: DeviceAuthorization = serde_json::from_str(body)
        .map_err(|err| HytaleError::Unavailable(format!("unexpected device code answer: {err}")))?;
    if authorization.user_code.is_empty() || authorization.device_code.expose().is_empty() {
        return Err(HytaleError::Unavailable(
            "the device code answer has no code".into(),
        ));
    }
    Ok(authorization)
}

/// Access and refresh token from the token endpoint.
#[derive(Debug, Deserialize)]
pub struct TokenSet {
    pub access_token: Secret,
    #[serde(default)]
    pub refresh_token: Option<Secret>,
    #[serde(default = "default_expires_in")]
    pub expires_in: u64,
}

fn default_expires_in() -> u64 {
    3600
}

/// The token endpoint's answer to a device code poll or a refresh.
#[derive(Debug)]
pub enum TokenOutcome {
    Tokens(TokenSet),
    /// The user has not approved the device code yet.
    Pending,
    /// Polling too fast: wait 5 seconds longer from now on.
    SlowDown,
    /// The device code expired.
    Expired,
    /// The user declined.
    Denied,
    /// The grant (device code or refresh token) is invalid, used or expired.
    InvalidGrant(String),
    Failed(HytaleError),
}

#[derive(Deserialize)]
struct OAuthError {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

fn oauth_error_text(status: u16, body: &str) -> String {
    match serde_json::from_str::<OAuthError>(body) {
        Ok(error) => truncate(&match error.error_description {
            Some(description) if !description.is_empty() => {
                format!("{}: {description}", error.error)
            }
            _ => error.error,
        }),
        Err(_) => format!("Hytale answered with HTTP {status}"),
    }
}

pub fn parse_token_response(status: u16, body: &str) -> TokenOutcome {
    if (200..300).contains(&status) {
        return match serde_json::from_str::<TokenSet>(body) {
            Ok(tokens) if !tokens.access_token.expose().is_empty() => TokenOutcome::Tokens(tokens),
            Ok(_) => TokenOutcome::Failed(HytaleError::Unavailable(
                "the token answer has no access token".into(),
            )),
            Err(err) => TokenOutcome::Failed(HytaleError::Unavailable(format!(
                "unexpected token answer: {err}"
            ))),
        };
    }
    let Ok(error) = serde_json::from_str::<OAuthError>(body) else {
        return TokenOutcome::Failed(HytaleError::Unavailable(format!(
            "Hytale answered with HTTP {status}"
        )));
    };
    match error.error.as_str() {
        "authorization_pending" => TokenOutcome::Pending,
        "slow_down" => TokenOutcome::SlowDown,
        "expired_token" => TokenOutcome::Expired,
        "access_denied" => TokenOutcome::Denied,
        "invalid_grant" => TokenOutcome::InvalidGrant(oauth_error_text(status, body)),
        _ => TokenOutcome::Failed(HytaleError::Unavailable(oauth_error_text(status, body))),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, serde::Serialize)]
pub struct Profile {
    pub uuid: uuid::Uuid,
    pub username: String,
}

/// `/my-account/get-profiles`; the account's own id (`owner`) is not needed.
#[derive(Debug, Deserialize)]
pub struct Profiles {
    #[serde(default)]
    pub profiles: Vec<Profile>,
}

/// Errors of the account and session services, which answer with plain HTTP statuses.
pub fn service_error(status: u16, body: &str) -> HytaleError {
    let detail = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            ["message", "error_description", "error"]
                .iter()
                .find_map(|key| value.get(key)?.as_str().map(truncate))
        })
        .unwrap_or_else(|| format!("HTTP {status}"));
    match status {
        401 => HytaleError::Relink(format!("Hytale rejected the account's login ({detail})")),
        403 => HytaleError::Limit(detail),
        404 => HytaleError::Profile(detail),
        _ => HytaleError::Unavailable(format!("Hytale answered with {detail}")),
    }
}

pub fn parse_profiles(status: u16, body: &str) -> Result<Profiles, HytaleError> {
    if !(200..300).contains(&status) {
        return Err(service_error(status, body));
    }
    serde_json::from_str(body)
        .map_err(|err| HytaleError::Unavailable(format!("unexpected profile list: {err}")))
}

/// The tokens a server starts with (`HYTALE_SERVER_SESSION_TOKEN`, `HYTALE_SERVER_IDENTITY_TOKEN`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameSession {
    pub session_token: Secret,
    pub identity_token: Secret,
    #[serde(default)]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub fn parse_game_session(status: u16, body: &str) -> Result<GameSession, HytaleError> {
    if !(200..300).contains(&status) {
        return Err(service_error(status, body));
    }
    let session: GameSession = serde_json::from_str(body)
        .map_err(|err| HytaleError::Unavailable(format!("unexpected game session: {err}")))?;
    if session.session_token.expose().is_empty() || session.identity_token.expose().is_empty() {
        return Err(HytaleError::Unavailable(
            "the game session has no tokens".into(),
        ));
    }
    Ok(session)
}

async fn read(response: reqwest::Response) -> Result<(u16, String), HytaleError> {
    let status = response.status().as_u16();
    let body = response.text().await.map_err(unavailable)?;
    Ok((status, body))
}

pub async fn request_device_code(
    client: &reqwest::Client,
) -> Result<DeviceAuthorization, HytaleError> {
    let response = client
        .post(DEVICE_AUTH_URL)
        .timeout(TIMEOUT)
        .form(&[("client_id", CLIENT_ID), ("scope", SCOPE)])
        .send()
        .await
        .map_err(unavailable)?;
    let (status, body) = read(response).await?;
    parse_device_authorization(status, &body)
}

async fn token_request(client: &reqwest::Client, form: &[(&str, &str)]) -> TokenOutcome {
    let response = match client
        .post(TOKEN_URL)
        .timeout(TIMEOUT)
        .form(form)
        .send()
        .await
    {
        Ok(response) => response,
        Err(err) => return TokenOutcome::Failed(unavailable(err)),
    };
    match read(response).await {
        Ok((status, body)) => parse_token_response(status, &body),
        Err(err) => TokenOutcome::Failed(err),
    }
}

pub async fn poll_device_code(client: &reqwest::Client, device_code: &Secret) -> TokenOutcome {
    token_request(
        client,
        &[
            ("client_id", CLIENT_ID),
            ("grant_type", DEVICE_GRANT),
            ("device_code", device_code.expose()),
        ],
    )
    .await
}

/// Exchanges the refresh token. On success the old refresh token is no longer valid.
pub async fn refresh(client: &reqwest::Client, refresh_token: &Secret) -> TokenOutcome {
    token_request(
        client,
        &[
            ("client_id", CLIENT_ID),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.expose()),
        ],
    )
    .await
}

pub async fn profiles(client: &reqwest::Client, access: &Secret) -> Result<Profiles, HytaleError> {
    let response = client
        .get(PROFILES_URL)
        .timeout(TIMEOUT)
        .bearer_auth(access.expose())
        .send()
        .await
        .map_err(unavailable)?;
    let (status, body) = read(response).await?;
    parse_profiles(status, &body)
}

pub async fn new_game_session(
    client: &reqwest::Client,
    access: &Secret,
    profile: uuid::Uuid,
) -> Result<GameSession, HytaleError> {
    let response = client
        .post(NEW_SESSION_URL)
        .timeout(TIMEOUT)
        .bearer_auth(access.expose())
        .json(&serde_json::json!({ "uuid": profile }))
        .send()
        .await
        .map_err(unavailable)?;
    let (status, body) = read(response).await?;
    parse_game_session(status, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_never_show_in_debug_output() {
        let tokens = parse_token_response(
            200,
            r#"{"access_token":"eyJ.access.sig","token_type":"Bearer","expires_in":3600,
                "refresh_token":"xreEsdDGrfIaQc","scope":"openid offline auth:server"}"#,
        );
        let printed = format!("{tokens:?}");
        assert!(!printed.contains("eyJ.access.sig"), "{printed}");
        assert!(!printed.contains("xreEsdDGrfIaQc"), "{printed}");
        let TokenOutcome::Tokens(tokens) = tokens else {
            panic!("expected tokens")
        };
        assert_eq!(tokens.access_token.expose(), "eyJ.access.sig");
        assert_eq!(tokens.refresh_token.unwrap().expose(), "xreEsdDGrfIaQc");
        assert_eq!(tokens.expires_in, 3600);
    }

    #[test]
    fn device_code_answers_are_read_as_hytale_sends_them() {
        // recorded from oauth.accounts.hytale.com, codes replaced; note the extra `Header` key
        let body = r#"{"Header":null,"device_code":"GmRhmhcxhwAzkoEqiMEg_DnyEysNkuNhszIySk9eS",
            "user_code":"ABCD1234","verification_uri":"https://oauth.accounts.hytale.com/oauth2/device/verify",
            "verification_uri_complete":"https://oauth.accounts.hytale.com/oauth2/device/verify?user_code=ABCD1234",
            "expires_in":600,"interval":5}"#;
        let device = parse_device_authorization(200, body).unwrap();
        assert_eq!(device.user_code, "ABCD1234");
        assert_eq!(device.expires_in, 600);
        assert_eq!(device.interval, 5);
        assert_eq!(
            device.verification_uri_complete.as_deref(),
            Some("https://oauth.accounts.hytale.com/oauth2/device/verify?user_code=ABCD1234")
        );
        assert!(!format!("{device:?}").contains("GmRhmhcx"));

        let without_interval = r#"{"device_code":"d","user_code":"u","verification_uri":"https://x",
            "expires_in":900}"#;
        assert_eq!(
            parse_device_authorization(200, without_interval)
                .unwrap()
                .interval,
            5
        );
        assert!(matches!(
            parse_device_authorization(400, r#"{"error":"invalid_client"}"#),
            Err(HytaleError::Unavailable(message)) if message == "invalid_client"
        ));
    }

    #[test]
    fn device_code_polls_map_every_rfc_8628_answer() {
        assert!(matches!(
            parse_token_response(400, r#"{"error":"authorization_pending"}"#),
            TokenOutcome::Pending
        ));
        assert!(matches!(
            parse_token_response(400, r#"{"error":"slow_down"}"#),
            TokenOutcome::SlowDown
        ));
        assert!(matches!(
            parse_token_response(400, r#"{"error":"expired_token"}"#),
            TokenOutcome::Expired
        ));
        assert!(matches!(
            parse_token_response(400, r#"{"error":"access_denied"}"#),
            TokenOutcome::Denied
        ));
        // recorded: Hytale's answer to a used, expired or unknown refresh token
        let invalid = r#"{"error":"invalid_grant","error_description":"The provided authorization grant (e.g., authorization code, resource owner credentials) or refresh token is invalid, expired, revoked, does not match the redirection URI used in the authorization request, or was issued to another client. The refresh token is malformed or not valid."}"#;
        match parse_token_response(400, invalid) {
            TokenOutcome::InvalidGrant(message) => {
                assert!(message.starts_with("invalid_grant: The provided"));
                assert!(message.chars().count() <= MAX_MESSAGE + 1);
            }
            other => panic!("expected invalid_grant, got {other:?}"),
        }
        assert!(matches!(
            parse_token_response(502, "<html>bad gateway</html>"),
            TokenOutcome::Failed(HytaleError::Unavailable(message)) if message == "Hytale answered with HTTP 502"
        ));
        assert!(matches!(
            parse_token_response(200, r#"{"token_type":"Bearer"}"#),
            TokenOutcome::Failed(_)
        ));
    }

    #[test]
    fn profiles_and_game_sessions() {
        let profiles = parse_profiles(
            200,
            r#"{"owner":"550e8400-e29b-41d4-a716-446655440000","profiles":[
                {"uuid":"123e4567-e89b-12d3-a456-426614174000","username":"ServerOperator"}]}"#,
        )
        .unwrap();
        assert_eq!(profiles.profiles.len(), 1);
        assert_eq!(profiles.profiles[0].username, "ServerOperator");

        let session = parse_game_session(
            200,
            r#"{"sessionToken":"eyJ.session","identityToken":"eyJ.identity","expiresAt":"2026-01-07T15:00:00Z"}"#,
        )
        .unwrap();
        assert_eq!(session.session_token.expose(), "eyJ.session");
        assert_eq!(session.identity_token.expose(), "eyJ.identity");
        assert_eq!(
            session.expires_at.unwrap().to_rfc3339(),
            "2026-01-07T15:00:00+00:00"
        );
        assert!(!format!("{session:?}").contains("eyJ"));
    }

    #[test]
    fn service_errors_say_what_the_user_can_do() {
        assert_eq!(
            parse_game_session(403, r#"{"message":"session limit reached"}"#).unwrap_err(),
            HytaleError::Limit("session limit reached".into())
        );
        assert_eq!(parse_game_session(403, "").unwrap_err().code(), "limit");
        assert_eq!(parse_game_session(404, "").unwrap_err().code(), "profile");
        assert_eq!(parse_profiles(401, "").unwrap_err().code(), "relink");
        assert_eq!(
            parse_profiles(500, "oops").unwrap_err().code(),
            "unavailable"
        );
        assert!(parse_game_session(200, r#"{"sessionToken":"","identityToken":"x"}"#).is_err());
    }
}
