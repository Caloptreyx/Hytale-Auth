//! Per server: whether it signs in automatically, and how its last start went.
use crate::{accounts::Source, hytale::HytaleError};
use chrono::{DateTime, Utc};
use serde::Serialize;
use shared::database::Database;
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct LastStart {
    pub source: Option<Source>,
    pub profile_name: Option<String>,
    /// When the panel minted the game session the server started with.
    pub minted: Option<DateTime<Utc>>,
    pub session_expires: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub error_message: Option<String>,
    pub error_at: Option<DateTime<Utc>>,
}

/// Automatic login is on unless the server turned it off.
pub async fn automatic(db: &Database, server: Uuid) -> Result<bool, sqlx::Error> {
    let automatic: Option<bool> = sqlx::query_scalar(
        "SELECT automatic FROM dev_caloptreyx_hytaleauth_servers WHERE server_uuid = $1",
    )
    .bind(server)
    .fetch_optional(db.read())
    .await?;
    Ok(automatic.unwrap_or(true))
}

pub async fn set_automatic(
    db: &Database,
    server: Uuid,
    automatic: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO dev_caloptreyx_hytaleauth_servers (server_uuid, automatic) VALUES ($1, $2)
         ON CONFLICT (server_uuid) DO UPDATE SET automatic = EXCLUDED.automatic",
    )
    .bind(server)
    .bind(automatic)
    .execute(db.write())
    .await?;
    Ok(())
}

pub async fn last_start(db: &Database, server: Uuid) -> Result<Option<LastStart>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT source, profile_name, minted, session_expires, error, error_message, error_at
         FROM dev_caloptreyx_hytaleauth_servers
         WHERE server_uuid = $1 AND (minted IS NOT NULL OR error IS NOT NULL)",
    )
    .bind(server)
    .fetch_optional(db.read())
    .await?;
    let Some(row) = row else { return Ok(None) };
    let source: Option<String> = row.try_get("source")?;
    Ok(Some(LastStart {
        source: source.map(|source| {
            if source == "user" {
                Source::User
            } else {
                Source::Host
            }
        }),
        profile_name: row.try_get("profile_name")?,
        minted: row.try_get("minted")?,
        session_expires: row.try_get("session_expires")?,
        error: row.try_get("error")?,
        error_message: row.try_get("error_message")?,
        error_at: row.try_get("error_at")?,
    }))
}

fn source_name(source: Source) -> &'static str {
    match source {
        Source::User => "user",
        Source::Host => "host",
    }
}

/// The server started with a game session of `account`.
pub async fn record_session(
    db: &Database,
    server: Uuid,
    source: Source,
    account: Uuid,
    profile_name: &str,
    expires: Option<DateTime<Utc>>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO dev_caloptreyx_hytaleauth_servers
             (server_uuid, account_id, source, profile_name, minted, session_expires)
         VALUES ($1, $2, $3, $4, now(), $5)
         ON CONFLICT (server_uuid) DO UPDATE SET
             account_id = EXCLUDED.account_id, source = EXCLUDED.source,
             profile_name = EXCLUDED.profile_name, minted = EXCLUDED.minted,
             session_expires = EXCLUDED.session_expires,
             error = NULL, error_message = NULL, error_at = NULL",
    )
    .bind(server)
    .bind(account)
    .bind(source_name(source))
    .bind(profile_name)
    .bind(expires)
    .execute(db.write())
    .await?;
    Ok(())
}

/// The server started without a login because minting its session failed.
pub async fn record_error(
    db: &Database,
    server: Uuid,
    source: Source,
    account: Uuid,
    error: &HytaleError,
) {
    let result = sqlx::query(
        "INSERT INTO dev_caloptreyx_hytaleauth_servers
             (server_uuid, account_id, source, error, error_message, error_at)
         VALUES ($1, $2, $3, $4, $5, now())
         ON CONFLICT (server_uuid) DO UPDATE SET
             account_id = EXCLUDED.account_id, source = EXCLUDED.source,
             minted = NULL, session_expires = NULL,
             error = EXCLUDED.error, error_message = EXCLUDED.error_message,
             error_at = EXCLUDED.error_at",
    )
    .bind(server)
    .bind(account)
    .bind(source_name(source))
    .bind(error.code())
    .bind(error.message())
    .execute(db.write())
    .await;
    if let Err(err) = result {
        tracing::warn!(server = %server, "could not record a Hytale login error: {err}");
    }
}
