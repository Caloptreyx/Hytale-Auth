//! Signs Hytale servers in when they start (Method C of Hytale's provider guide: token
//! passthrough). Wings fetches a server's configuration from the panel right before every start;
//! a `RemoteApiServer` hook adds a fresh game session of the linked account to its environment as
//! `HYTALE_SERVER_SESSION_TOKEN` and `HYTALE_SERVER_IDENTITY_TOKEN`, which `HytaleServer.jar`
//! reads by itself. No egg or image change is needed, and the tokens never appear in the start
//! command the console echoes.
//!
//! The same configuration is also built when the panel syncs a server to Wings (schedule and
//! startup changes, …) and when Wings boots. Only a start may get a new session: Wings reports
//! the server as `starting` while it fetches the configuration for a start. Any other time the
//! tokens the running container already has are passed on unchanged, so a sync does not flag the
//! server for a restart, and no session is minted that no server would use.
//!
//! `starting` also lasts while the server boots (and while the downloader waits for its login), so
//! a sync can arrive then too. The session the container got is therefore reused while it has more
//! than [`REUSE_MARGIN`] left and is still the one of the chosen account and profile; that also
//! covers quick restarts. Sessions are never ended by the panel: a running server might hold them,
//! and they expire within the hour anyway.
//!
//! Whatever goes wrong here, the server still starts: without tokens, and the console helper takes
//! over. An error from the hook would fail Wings' request and the start with it.
use crate::{
    accounts::{self, AccessError},
    hytale::{self, GameSession, HytaleError, Secret},
    servers, settings,
};
use compact_str::CompactString;
use indexmap::IndexMap;
use shared::{
    State,
    crypt::token_digest,
    models::server::{RemoteApiServer, Server},
};
use std::time::Duration;

pub const SESSION_ENV: &str = "HYTALE_SERVER_SESSION_TOKEN";
pub const IDENTITY_ENV: &str = "HYTALE_SERVER_IDENTITY_TOKEN";

/// Wings waits for the configuration; a slow Hytale must not hold the start up for long.
const HOOK_TIMEOUT: Duration = Duration::from_secs(15);
const WINGS_TIMEOUT: Duration = Duration::from_secs(4);
/// A session handed out at a start is handed out again while it has this much left: the server
/// refreshes it 5 minutes before it expires, so until then it is exactly as minted.
const REUSE_MARGIN: chrono::Duration = chrono::Duration::minutes(30);

/// The session and identity token the server gets, if any.
pub struct Plan(Option<(Secret, Secret)>);

impl Plan {
    fn apply(self, environment: &mut IndexMap<CompactString, serde_json::Value>) {
        if let Some((session, identity)) = self.0 {
            environment.insert(
                SESSION_ENV.into(),
                serde_json::Value::String(session.expose().to_string()),
            );
            environment.insert(
                IDENTITY_ENV.into(),
                serde_json::Value::String(identity.expose().to_string()),
            );
        }
    }
}

pub fn register() {
    RemoteApiServer::mutate(
        |server, state| Box::pin(async move { Ok(plan(state, server).await) }),
        |remote, plan: Plan, _state| {
            plan.apply(&mut remote.settings.environment);
            Ok(())
        },
    );
}

/// Whether a server runs Hytale: `HytaleServer.jar` in its start command, or an image named
/// `hytale` (the Caloptreyx egg's `ghcr.io/caloptreyx/games:hytale`). The frontend uses the same
/// rule (`frontend/src/lib/console.ts`).
pub fn is_hytale(startup: &str, image: &str) -> bool {
    if startup.contains("HytaleServer.jar") {
        return true;
    }
    let image = image.to_ascii_lowercase();
    image.match_indices("hytale").any(|(start, word)| {
        let before = image[..start].chars().next_back();
        let after = image[start + word.len()..].chars().next();
        before.is_none_or(|c| matches!(c, '/' | ':' | '_' | '-'))
            && after.is_none_or(|c| matches!(c, '/' | ':' | '_' | '@' | '-'))
    })
}

async fn plan(state: &State, server: &Server) -> Plan {
    if !is_hytale(&server.startup, &server.image) {
        return Plan(None);
    }
    match tokio::time::timeout(HOOK_TIMEOUT, decide(state, server)).await {
        Ok(Ok(plan)) => plan,
        Ok(Err(err)) => {
            tracing::warn!(server = %server.uuid, "Hytale Auth: no automatic login: {err:#}");
            Plan(None)
        }
        Err(_) => {
            tracing::warn!(server = %server.uuid, "Hytale Auth: no automatic login: timed out");
            Plan(None)
        }
    }
}

/// `--auth-mode offline` needs no login.
async fn offline(state: &State, server: &Server) -> Result<bool, sqlx::Error> {
    if server.startup.contains("--auth-mode offline") {
        return Ok(true);
    }
    let mode: Option<Option<String>> = sqlx::query_scalar(
        "SELECT COALESCE(server_variables.value, nest_egg_variables.default_value)
         FROM nest_egg_variables
         LEFT JOIN server_variables ON server_variables.variable_uuid = nest_egg_variables.uuid
             AND server_variables.server_uuid = $1
         WHERE nest_egg_variables.egg_uuid = $2 AND nest_egg_variables.env_variable = 'AUTH_MODE'",
    )
    .bind(server.uuid)
    .bind(server.egg.uuid)
    .fetch_optional(state.database.read())
    .await?;
    Ok(mode
        .flatten()
        .is_some_and(|mode| mode.trim().eq_ignore_ascii_case("offline")))
}

async fn decide(state: &State, server: &Server) -> Result<Plan, anyhow::Error> {
    let db = &state.database;
    if !servers::automatic(db, server.uuid).await? || offline(state, server).await? {
        return Ok(Plan(None));
    }
    let settings = settings::load(state).await?;
    let (own, host) = accounts::for_owner(db, server.owner.uuid).await?;
    let Some((source, account)) = accounts::choose(&settings, true, own.as_ref(), host.as_ref())
    else {
        return Ok(Plan(None));
    };
    let wings = match wings_view(state, server).await {
        Ok(view) => view,
        Err(err) => {
            // Wings booting or the server not on the node yet: nothing is starting
            tracing::debug!(server = %server.uuid, "Hytale Auth: no power state from Wings: {err:#}");
            return Ok(Plan(None));
        }
    };
    if wings.state != "starting" {
        return Ok(Plan(wings.tokens));
    }

    let profile = account
        .profile
        .clone()
        .ok_or_else(|| anyhow::anyhow!("the account has no game profile"))?;
    if let Some((session, _)) = &wings.tokens
        && servers::reusable(
            db,
            server.uuid,
            account.id,
            &profile.username,
            &token_digest(session.expose()),
            REUSE_MARGIN,
        )
        .await?
    {
        return Ok(Plan(wings.tokens));
    }
    match mint(state, account.id, profile.uuid).await {
        Ok(session) => {
            servers::record_session(
                db,
                server.uuid,
                source,
                account.id,
                &profile.username,
                &token_digest(session.session_token.expose()),
                session.expires_at,
            )
            .await?;
            if account.error.is_some() {
                accounts::clear_error(db, account.id).await?;
            }
            Ok(Plan(Some((session.session_token, session.identity_token))))
        }
        Err(AccessError::Hytale(err)) => {
            tracing::warn!(
                "Hytale Auth: server {} starts without a login ({}, account {}): {}",
                server.uuid,
                err.code(),
                account.id,
                err.message()
            );
            servers::record_error(db, server.uuid, source, account.id, &err).await;
            if !matches!(err, HytaleError::Relink(_)) {
                accounts::record_error(db, account.id, &err).await;
            }
            Ok(Plan(None))
        }
        Err(AccessError::Panel(err)) => Err(err),
    }
}

async fn mint(
    state: &State,
    account: uuid::Uuid,
    profile: uuid::Uuid,
) -> Result<GameSession, AccessError> {
    let access = accounts::access_token(state, account, false).await?;
    match hytale::new_game_session(&state.client, &access, profile).await {
        // an access token revoked before its time: refresh once and try again
        Err(HytaleError::Relink(_)) => {
            let access = accounts::access_token(state, account, true).await?;
            Ok(hytale::new_game_session(&state.client, &access, profile).await?)
        }
        result => Ok(result?),
    }
}

/// The server as Wings has it right now.
struct WingsView {
    state: String,
    /// The tokens its configuration (the running container's environment) carries.
    tokens: Option<(Secret, Secret)>,
}

/// Reads Wings' answer leniently: only the power state and two environment variables matter, and
/// the rest of the payload differs between Wings versions.
fn parse_wings_view(value: &serde_json::Value) -> Option<WingsView> {
    let state = value.get("state")?.as_str()?.to_string();
    let environment = value
        .get("configuration")
        .and_then(|c| c.get("environment"));
    let token = |name: &str| {
        environment
            .and_then(|env| env.get(name))
            .and_then(serde_json::Value::as_str)
            .filter(|token| !token.is_empty())
            .map(Secret::new)
    };
    Some(WingsView {
        state,
        tokens: token(SESSION_ENV).zip(token(IDENTITY_ENV)),
    })
}

async fn wings_view(state: &State, server: &Server) -> Result<WingsView, anyhow::Error> {
    let node = server.node.fetch_cached(&state.database).await?;
    let token = node.token.decrypt(&state.database).await?;
    let value: serde_json::Value = state
        .client
        .get(node.url(&format!("/api/servers/{}", server.uuid)))
        .timeout(WINGS_TIMEOUT)
        .bearer_auth(token.as_str())
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    parse_wings_view(&value).ok_or_else(|| anyhow::anyhow!("Wings sent no power state"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hytale_servers_are_recognised_like_the_frontend_does() {
        let java = "java -Xms128M -jar Server/HytaleServer.jar --assets Assets.zip";
        assert!(is_hytale(java, "ghcr.io/caloptreyx/java:25"));
        assert!(is_hytale("./start.sh", "ghcr.io/caloptreyx/games:hytale"));
        assert!(is_hytale("./start.sh", "ghcr.io/example/hytale:latest"));
        assert!(!is_hytale(
            "java -jar server.jar",
            "ghcr.io/ptero-eggs/yolks:java_21"
        ));
        assert!(!is_hytale("java -jar server.jar", "example/hytalesque:1"));
    }

    #[test]
    fn tokens_go_into_the_environment_only_when_there_are_some() {
        let mut environment = IndexMap::new();
        environment.insert(CompactString::from("AUTH_MODE"), "authenticated".into());
        Plan(None).apply(&mut environment);
        assert_eq!(environment.len(), 1);
        Plan(Some((
            Secret::new("eyJ.session"),
            Secret::new("eyJ.identity"),
        )))
        .apply(&mut environment);
        assert_eq!(environment[SESSION_ENV], "eyJ.session");
        assert_eq!(environment[IDENTITY_ENV], "eyJ.identity");
        assert_eq!(environment["AUTH_MODE"], "authenticated");
    }

    #[test]
    fn wings_answers_are_read_leniently() {
        let starting = serde_json::json!({
            "state": "starting",
            "is_suspended": false,
            "configuration": { "environment": {
                "AUTH_MODE": "authenticated",
                SESSION_ENV: "old.session",
                IDENTITY_ENV: "old.identity",
            }},
        });
        let view = parse_wings_view(&starting).unwrap();
        assert_eq!(view.state, "starting");
        let (session, identity) = view.tokens.unwrap();
        assert_eq!(session.expose(), "old.session");
        assert_eq!(identity.expose(), "old.identity");

        let running = serde_json::json!({ "state": "running", "configuration": {} });
        let view = parse_wings_view(&running).unwrap();
        assert!(view.tokens.is_none());

        // one token alone is not passed on
        let half = serde_json::json!({ "state": "running", "configuration": { "environment": { SESSION_ENV: "x" } } });
        assert!(parse_wings_view(&half).unwrap().tokens.is_none());
        assert!(parse_wings_view(&serde_json::json!({ "error": "not found" })).is_none());
    }
}
