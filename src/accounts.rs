//! Linked Hytale accounts: the host account an admin links for every server, and the accounts
//! users link for the servers they own. Refresh and access tokens are stored encrypted with the
//! panel's encryption key (`APP_ENCRYPTION_KEY`) and never leave the backend.
use crate::{
    hytale::{self, HytaleError, Profile, Secret, TokenOutcome},
    rotation::{self, KeyedLocks, RotateError, StoredTokens},
    settings::Settings,
};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use shared::{State, database::Database};
use sqlx::{Postgres, Row, postgres::PgRow};
use std::sync::LazyLock;
use uuid::Uuid;

/// Whose account: the host account, or a panel user's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Owner {
    Host,
    User(Uuid),
}

impl Owner {
    fn user(self) -> Option<Uuid> {
        match self {
            Self::Host => None,
            Self::User(user) => Some(user),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    Linked,
    /// The refresh token died (30 days unused, or revoked): link the account again.
    Relink,
}

#[derive(Clone, Debug, Serialize)]
pub struct ErrorInfo {
    pub code: String,
    pub message: String,
    pub at: DateTime<Utc>,
}

/// An account as the API shows it: no tokens.
#[derive(Clone, Debug, Serialize)]
pub struct Account {
    pub id: Uuid,
    #[serde(skip)]
    pub user_uuid: Option<Uuid>,
    /// The game profile servers run as.
    pub profile: Option<Profile>,
    pub profiles: Vec<Profile>,
    pub status: AccountStatus,
    pub error: Option<ErrorInfo>,
    pub refreshed: DateTime<Utc>,
    pub created: DateTime<Utc>,
}

impl Account {
    /// Whether servers can start with it.
    pub fn usable(&self) -> bool {
        self.status == AccountStatus::Linked && self.profile.is_some()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    User,
    Host,
}

/// The account a server starts with: the owner's own when they linked one (and users may), else
/// the host account (when servers may use it). An account that cannot be used is skipped, so a
/// user whose own login died falls back to the host account rather than to no login.
pub fn choose<'a>(
    settings: &Settings,
    automatic: bool,
    user: Option<&'a Account>,
    host: Option<&'a Account>,
) -> Option<(Source, &'a Account)> {
    if !automatic {
        return None;
    }
    if settings.user_accounts
        && let Some(account) = user.filter(|account| account.usable())
    {
        return Some((Source::User, account));
    }
    if settings.host_account
        && let Some(account) = host.filter(|account| account.usable())
    {
        return Some((Source::Host, account));
    }
    None
}

/// The account columns, for `concat!` (sqlx 0.9 only takes `'static` SQL).
macro_rules! account_columns {
    () => {
        "id, user_uuid, profile_uuid, profile_name, profiles, status, error, error_message, \
         error_at, refreshed, created"
    };
}

fn account_from_row(row: &PgRow) -> Result<Account, sqlx::Error> {
    let profile_uuid: Option<Uuid> = row.try_get("profile_uuid")?;
    let profile_name: Option<String> = row.try_get("profile_name")?;
    let profiles: Vec<Profile> =
        serde_json::from_value(row.try_get::<serde_json::Value, _>("profiles")?)
            .unwrap_or_default();
    let error: Option<String> = row.try_get("error")?;
    let status: String = row.try_get("status")?;
    Ok(Account {
        id: row.try_get("id")?,
        user_uuid: row.try_get("user_uuid")?,
        profile: profile_uuid.map(|uuid| Profile {
            uuid,
            username: profile_name.unwrap_or_default(),
        }),
        profiles,
        status: if status == "relink" {
            AccountStatus::Relink
        } else {
            AccountStatus::Linked
        },
        error: match (error, row.try_get::<Option<DateTime<Utc>>, _>("error_at")?) {
            (Some(code), Some(at)) => Some(ErrorInfo {
                code,
                message: row
                    .try_get::<Option<String>, _>("error_message")?
                    .unwrap_or_default(),
                at,
            }),
            _ => None,
        },
        refreshed: row.try_get("refreshed")?,
        created: row.try_get("created")?,
    })
}

pub async fn get(db: &Database, owner: Owner) -> Result<Option<Account>, sqlx::Error> {
    let row = sqlx::query(concat!(
        "SELECT ",
        account_columns!(),
        " FROM dev_caloptreyx_hytaleauth_accounts WHERE user_uuid IS NOT DISTINCT FROM $1"
    ))
    .bind(owner.user())
    .fetch_optional(db.read())
    .await?;
    row.as_ref().map(account_from_row).transpose()
}

/// The server owner's own account and the host account.
pub async fn for_owner(
    db: &Database,
    user: Uuid,
) -> Result<(Option<Account>, Option<Account>), sqlx::Error> {
    let rows = sqlx::query(concat!(
        "SELECT ",
        account_columns!(),
        " FROM dev_caloptreyx_hytaleauth_accounts WHERE user_uuid = $1 OR user_uuid IS NULL"
    ))
    .bind(user)
    .fetch_all(db.read())
    .await?;
    let mut own = None;
    let mut host = None;
    for row in &rows {
        let account = account_from_row(row)?;
        if account.user_uuid.is_some() {
            own = Some(account);
        } else {
            host = Some(account);
        }
    }
    Ok((own, host))
}

pub async fn delete(db: &Database, owner: Owner) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "DELETE FROM dev_caloptreyx_hytaleauth_accounts WHERE user_uuid IS NOT DISTINCT FROM $1",
    )
    .bind(owner.user())
    .execute(db.write())
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Selects one of the account's game profiles. `None` when the profile is not one of them.
pub async fn set_profile(
    db: &Database,
    owner: Owner,
    profile: Uuid,
) -> Result<Option<Account>, sqlx::Error> {
    let Some(account) = get(db, owner).await? else {
        return Ok(None);
    };
    let Some(chosen) = account.profiles.iter().find(|p| p.uuid == profile) else {
        return Ok(None);
    };
    sqlx::query(
        "UPDATE dev_caloptreyx_hytaleauth_accounts
         SET profile_uuid = $2, profile_name = $3,
             error = CASE WHEN error IN ('profile', 'no_profile') THEN NULL ELSE error END
         WHERE id = $1",
    )
    .bind(account.id)
    .bind(chosen.uuid)
    .bind(&chosen.username)
    .execute(db.write())
    .await?;
    get(db, owner).await
}

pub async fn record_error(db: &Database, id: Uuid, error: &HytaleError) {
    let result = sqlx::query(
        "UPDATE dev_caloptreyx_hytaleauth_accounts
         SET error = $2, error_message = $3, error_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(error.code())
    .bind(error.message())
    .execute(db.write())
    .await;
    if let Err(err) = result {
        tracing::warn!(account = %id, "could not record a Hytale account error: {err}");
    }
}

pub async fn clear_error(db: &Database, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE dev_caloptreyx_hytaleauth_accounts
         SET error = NULL, error_message = NULL, error_at = NULL
         WHERE id = $1 AND error IS NOT NULL",
    )
    .bind(id)
    .execute(db.write())
    .await?;
    Ok(())
}

/// Servers whose last start signed in with the account.
pub async fn servers_using(db: &Database, id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT count(*) FROM dev_caloptreyx_hytaleauth_servers
         WHERE account_id = $1 AND error IS NULL AND minted IS NOT NULL",
    )
    .bind(id)
    .fetch_one(db.read())
    .await
}

/// Accounts whose refresh token was not used for `age`: refreshing extends its 30 days.
pub async fn idle_accounts(db: &Database, age: Duration) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM dev_caloptreyx_hytaleauth_accounts
         WHERE status = 'linked' AND refreshed < now() - make_interval(secs => $1)",
    )
    .bind(age.num_seconds() as f64)
    .fetch_all(db.read())
    .await
}

/// A failure that is either Hytale's or the panel's own.
#[derive(Debug)]
pub enum AccessError {
    Hytale(HytaleError),
    Panel(anyhow::Error),
}

impl From<sqlx::Error> for AccessError {
    fn from(err: sqlx::Error) -> Self {
        Self::Panel(err.into())
    }
}

impl From<anyhow::Error> for AccessError {
    fn from(err: anyhow::Error) -> Self {
        Self::Panel(err)
    }
}

impl From<HytaleError> for AccessError {
    fn from(err: HytaleError) -> Self {
        Self::Hytale(err)
    }
}

/// The account's row, locked (`FOR UPDATE`) for the transaction a refresh runs in.
struct PgVault<'a> {
    tx: sqlx::Transaction<'static, Postgres>,
    db: &'a Database,
    id: Uuid,
}

#[async_trait::async_trait]
impl rotation::Vault for PgVault<'_> {
    async fn load(&mut self) -> Result<StoredTokens, anyhow::Error> {
        let row = sqlx::query(
            "SELECT refresh_token, access_token, access_expires
             FROM dev_caloptreyx_hytaleauth_accounts WHERE id = $1 FOR UPDATE",
        )
        .bind(self.id)
        .fetch_optional(&mut *self.tx)
        .await?
        .ok_or_else(|| anyhow::anyhow!("the Hytale account was unlinked"))?;
        let refresh: Vec<u8> = row.try_get("refresh_token")?;
        let access: Option<Vec<u8>> = row.try_get("access_token")?;
        let expires: Option<DateTime<Utc>> = row.try_get("access_expires")?;
        let access = match (access, expires) {
            (Some(access), Some(expires)) => Some((
                Secret::new(self.db.decrypt(access).await?.to_string()),
                expires,
            )),
            _ => None,
        };
        Ok(StoredTokens {
            refresh: Secret::new(self.db.decrypt(refresh).await?.to_string()),
            access,
        })
    }

    async fn store(
        &mut self,
        tokens: &StoredTokens,
        now: DateTime<Utc>,
    ) -> Result<(), anyhow::Error> {
        let refresh = self.db.encrypt(tokens.refresh.expose().to_string()).await?;
        let (access, expires) = match &tokens.access {
            Some((access, expires)) => (
                Some(self.db.encrypt(access.expose().to_string()).await?),
                Some(*expires),
            ),
            None => (None, None),
        };
        sqlx::query(
            "UPDATE dev_caloptreyx_hytaleauth_accounts
             SET refresh_token = $2, access_token = $3, access_expires = $4, refreshed = $5,
                 status = 'linked'
             WHERE id = $1",
        )
        .bind(self.id)
        .bind(refresh)
        .bind(access)
        .bind(expires)
        .bind(now)
        .execute(&mut *self.tx)
        .await?;
        Ok(())
    }

    async fn needs_relink(&mut self, message: &str) -> Result<(), anyhow::Error> {
        sqlx::query(
            "UPDATE dev_caloptreyx_hytaleauth_accounts
             SET status = 'relink', access_token = NULL, access_expires = NULL,
                 error = 'relink', error_message = $2, error_at = now()
             WHERE id = $1",
        )
        .bind(self.id)
        .bind(message)
        .execute(&mut *self.tx)
        .await?;
        Ok(())
    }
}

struct HytaleRefresher<'a>(&'a reqwest::Client);

#[async_trait::async_trait]
impl rotation::Refresher for HytaleRefresher<'_> {
    async fn refresh(&self, refresh_token: &Secret) -> TokenOutcome {
        hytale::refresh(self.0, refresh_token).await
    }
}

static REFRESH_LOCKS: LazyLock<KeyedLocks<Uuid>> = LazyLock::new(KeyedLocks::default);

/// A valid access token for the account, refreshing (and rotating) its tokens when needed.
/// Refreshes of one account never overlap, across tasks and across panel instances.
pub async fn access_token(state: &State, id: Uuid, force: bool) -> Result<Secret, AccessError> {
    let _guard = REFRESH_LOCKS.lock(&id).await;
    let tx = state.database.write().begin().await?;
    let mut vault = PgVault {
        tx,
        db: &state.database,
        id,
    };
    let result = rotation::fresh_access(
        &mut vault,
        &HytaleRefresher(&state.client),
        Utc::now(),
        force,
    )
    .await;
    // committed whatever happened: rotated tokens and a dead login must both be kept
    if let Err(err) = vault.tx.commit().await {
        tracing::error!(account = %id, "could not store the Hytale account's tokens: {err}");
        return Err(AccessError::Panel(err.into()));
    }
    result.map_err(|err| match err {
        RotateError::Hytale(err) => AccessError::Hytale(err),
        RotateError::Storage(err) => AccessError::Panel(err),
    })
}

/// A device code login in progress, as the API shows it: no device code.
#[derive(Clone, Debug, Serialize)]
pub struct PendingLink {
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    pub expires: DateTime<Utc>,
    pub interval: i32,
}

fn link_from_row(row: &PgRow) -> Result<PendingLink, sqlx::Error> {
    Ok(PendingLink {
        user_code: row.try_get("user_code")?,
        verification_uri: row.try_get("verification_uri")?,
        verification_uri_complete: row.try_get("verification_uri_complete")?,
        expires: row.try_get("expires")?,
        interval: row.try_get("interval_seconds")?,
    })
}

pub async fn pending_link(db: &Database, owner: Owner) -> Result<Option<PendingLink>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT user_code, verification_uri, verification_uri_complete, expires, interval_seconds
         FROM dev_caloptreyx_hytaleauth_links
         WHERE user_uuid IS NOT DISTINCT FROM $1 AND expires > now()",
    )
    .bind(owner.user())
    .fetch_optional(db.read())
    .await?;
    row.as_ref().map(link_from_row).transpose()
}

pub async fn cancel_link(db: &Database, owner: Owner) -> Result<(), sqlx::Error> {
    sqlx::query(
        "DELETE FROM dev_caloptreyx_hytaleauth_links WHERE user_uuid IS NOT DISTINCT FROM $1",
    )
    .bind(owner.user())
    .execute(db.write())
    .await?;
    Ok(())
}

pub async fn delete_expired_links(db: &Database) -> Result<(), sqlx::Error> {
    sqlx::query(
        "DELETE FROM dev_caloptreyx_hytaleauth_links WHERE expires < now() - INTERVAL '1 hour'",
    )
    .execute(db.write())
    .await?;
    Ok(())
}

/// Starts a device code login for `owner`, replacing one in progress.
pub async fn start_link(
    state: &State,
    owner: Owner,
    started_by: Uuid,
) -> Result<PendingLink, AccessError> {
    let device = hytale::request_device_code(&state.client).await?;
    let device_code = state
        .database
        .encrypt(device.device_code.expose().to_string())
        .await?;
    let expires = Utc::now() + Duration::seconds(device.expires_in.clamp(60, 3600) as i64);
    let mut tx = state.database.write().begin().await?;
    sqlx::query(
        "DELETE FROM dev_caloptreyx_hytaleauth_links WHERE user_uuid IS NOT DISTINCT FROM $1",
    )
    .bind(owner.user())
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO dev_caloptreyx_hytaleauth_links
             (user_uuid, started_by, device_code, user_code, verification_uri,
              verification_uri_complete, interval_seconds, expires)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(owner.user())
    .bind(started_by)
    .bind(device_code)
    .bind(&device.user_code)
    .bind(&device.verification_uri)
    .bind(&device.verification_uri_complete)
    .bind(device.interval.clamp(1, 60) as i32)
    .bind(expires)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(PendingLink {
        user_code: device.user_code,
        verification_uri: device.verification_uri,
        verification_uri_complete: device.verification_uri_complete,
        expires,
        interval: device.interval.clamp(1, 60) as i32,
    })
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PollResult {
    /// No login in progress.
    None,
    Pending {
        link: PendingLink,
    },
    Expired,
    Denied,
    Linked {
        account: Account,
    },
    Failed {
        error: String,
        message: String,
    },
}

static LINK_LOCKS: LazyLock<KeyedLocks<Owner>> = LazyLock::new(KeyedLocks::default);

/// Asks Hytale whether the device code was approved, at most once per `interval`. On approval
/// the account replaces the owner's previous one, with its first game profile selected.
pub async fn poll_link(state: &State, owner: Owner) -> Result<PollResult, AccessError> {
    let _guard = LINK_LOCKS.lock(&owner).await;
    let db = &state.database;
    let Some(row) = sqlx::query(
        "SELECT id, started_by, device_code, user_code, verification_uri, verification_uri_complete,
                expires, interval_seconds, polled
         FROM dev_caloptreyx_hytaleauth_links WHERE user_uuid IS NOT DISTINCT FROM $1",
    )
    .bind(owner.user())
    .fetch_optional(db.read())
    .await?
    else {
        return Ok(PollResult::None);
    };
    let link = link_from_row(&row)?;
    let link_id: Uuid = row.try_get("id")?;
    let started_by: Option<Uuid> = row.try_get("started_by")?;
    let polled: Option<DateTime<Utc>> = row.try_get("polled")?;
    let now = Utc::now();
    if link.expires <= now {
        cancel_link(db, owner).await?;
        return Ok(PollResult::Expired);
    }
    if polled.is_some_and(|polled| now < polled + Duration::seconds(link.interval as i64)) {
        return Ok(PollResult::Pending { link });
    }
    sqlx::query("UPDATE dev_caloptreyx_hytaleauth_links SET polled = $2 WHERE id = $1")
        .bind(link_id)
        .bind(now)
        .execute(db.write())
        .await?;
    let device_code = Secret::new(
        db.decrypt(row.try_get::<Vec<u8>, _>("device_code")?)
            .await?
            .to_string(),
    );

    let tokens = match hytale::poll_device_code(&state.client, &device_code).await {
        TokenOutcome::Tokens(tokens) => tokens,
        TokenOutcome::Pending => return Ok(PollResult::Pending { link }),
        TokenOutcome::SlowDown => {
            let interval = (link.interval + 5).min(60);
            sqlx::query(
                "UPDATE dev_caloptreyx_hytaleauth_links SET interval_seconds = $2 WHERE id = $1",
            )
            .bind(link_id)
            .bind(interval)
            .execute(db.write())
            .await?;
            return Ok(PollResult::Pending {
                link: PendingLink { interval, ..link },
            });
        }
        TokenOutcome::Expired | TokenOutcome::InvalidGrant(_) => {
            cancel_link(db, owner).await?;
            return Ok(PollResult::Expired);
        }
        TokenOutcome::Denied => {
            cancel_link(db, owner).await?;
            return Ok(PollResult::Denied);
        }
        TokenOutcome::Failed(err) => {
            return Ok(PollResult::Failed {
                error: err.code().into(),
                message: err.message().into(),
            });
        }
    };
    let Some(refresh) = tokens.refresh_token else {
        cancel_link(db, owner).await?;
        return Ok(PollResult::Failed {
            error: "unavailable".into(),
            message: "Hytale sent no refresh token".into(),
        });
    };

    // the tokens are stored first: the device code is used up, a lost answer means linking again
    let refresh = db.encrypt(refresh.expose().to_string()).await?;
    let access = db.encrypt(tokens.access_token.expose().to_string()).await?;
    let access_expires = now + Duration::seconds(tokens.expires_in.min(86_400) as i64);
    let mut tx = db.write().begin().await?;
    sqlx::query(
        "DELETE FROM dev_caloptreyx_hytaleauth_accounts WHERE user_uuid IS NOT DISTINCT FROM $1",
    )
    .bind(owner.user())
    .execute(&mut *tx)
    .await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO dev_caloptreyx_hytaleauth_accounts
             (user_uuid, refresh_token, access_token, access_expires, refreshed, linked_by)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(owner.user())
    .bind(refresh)
    .bind(access)
    .bind(access_expires)
    .bind(now)
    .bind(started_by)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM dev_caloptreyx_hytaleauth_links WHERE id = $1")
        .bind(link_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    load_profiles(state, id, &tokens.access_token).await?;
    let account = get(db, owner)
        .await?
        .ok_or_else(|| anyhow::anyhow!("the new Hytale account disappeared"))?;
    Ok(PollResult::Linked { account })
}

/// Fetches the account's game profiles; keeps the selected one when it still exists, else selects
/// the first. A failure is recorded on the account, which stays linked.
async fn load_profiles(state: &State, id: Uuid, access: &Secret) -> Result<(), AccessError> {
    let db = &state.database;
    let profiles = match hytale::profiles(&state.client, access).await {
        Ok(profiles) => profiles.profiles,
        Err(err) => {
            record_error(db, id, &err).await;
            return Ok(());
        }
    };
    let current: Option<Uuid> = sqlx::query_scalar(
        "SELECT profile_uuid FROM dev_caloptreyx_hytaleauth_accounts WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(db.read())
    .await?
    .flatten();
    let selected = current
        .and_then(|uuid| profiles.iter().find(|p| p.uuid == uuid))
        .or(profiles.first());
    sqlx::query(
        "UPDATE dev_caloptreyx_hytaleauth_accounts
         SET profiles = $2, profile_uuid = $3, profile_name = $4,
             error = $5, error_message = $6, error_at = CASE WHEN $5::text IS NULL THEN NULL ELSE now() END
         WHERE id = $1",
    )
    .bind(id)
    .bind(serde_json::to_value(&profiles).map_err(anyhow::Error::from)?)
    .bind(selected.map(|p| p.uuid))
    .bind(selected.map(|p| p.username.clone()))
    .bind(selected.is_none().then(|| HytaleError::NoProfile.code()))
    .bind(selected.is_none().then(|| HytaleError::NoProfile.message().to_string()))
    .execute(db.write())
    .await?;
    Ok(())
}

/// Reloads the game profiles of the owner's account (after buying another profile, say).
pub async fn reload_profiles(state: &State, owner: Owner) -> Result<Option<Account>, AccessError> {
    let Some(account) = get(&state.database, owner).await? else {
        return Ok(None);
    };
    let access = access_token(state, account.id, false).await?;
    load_profiles(state, account.id, &access).await?;
    Ok(get(&state.database, owner).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(status: AccountStatus, profile: bool) -> Account {
        let now = Utc::now();
        Account {
            id: Uuid::new_v4(),
            user_uuid: None,
            profile: profile.then(|| Profile {
                uuid: Uuid::new_v4(),
                username: "Operator".into(),
            }),
            profiles: Vec::new(),
            status,
            error: None,
            refreshed: now,
            created: now,
        }
    }

    const BOTH: Settings = Settings {
        host_account: true,
        user_accounts: true,
    };

    #[test]
    fn the_owners_account_wins_over_the_host_account() {
        let own = account(AccountStatus::Linked, true);
        let host = account(AccountStatus::Linked, true);
        let (source, chosen) = choose(&BOTH, true, Some(&own), Some(&host)).unwrap();
        assert_eq!(source, Source::User);
        assert_eq!(chosen.id, own.id);

        let (source, chosen) = choose(&BOTH, true, None, Some(&host)).unwrap();
        assert_eq!(source, Source::Host);
        assert_eq!(chosen.id, host.id);
    }

    #[test]
    fn an_unusable_own_account_falls_back_to_the_host_account() {
        let host = account(AccountStatus::Linked, true);
        for own in [
            account(AccountStatus::Relink, true),
            account(AccountStatus::Linked, false),
        ] {
            let (source, _) = choose(&BOTH, true, Some(&own), Some(&host)).unwrap();
            assert_eq!(source, Source::Host);
        }
        let dead_host = account(AccountStatus::Relink, true);
        assert!(choose(&BOTH, true, None, Some(&dead_host)).is_none());
    }

    #[test]
    fn settings_and_the_server_switch_are_respected() {
        let own = account(AccountStatus::Linked, true);
        let host = account(AccountStatus::Linked, true);
        assert!(choose(&BOTH, false, Some(&own), Some(&host)).is_none());

        let no_users = Settings {
            user_accounts: false,
            ..BOTH
        };
        assert_eq!(
            choose(&no_users, true, Some(&own), Some(&host)).unwrap().0,
            Source::Host
        );
        let no_host = Settings {
            host_account: false,
            ..BOTH
        };
        assert!(choose(&no_host, true, None, Some(&host)).is_none());
        assert_eq!(
            choose(&no_host, true, Some(&own), Some(&host)).unwrap().0,
            Source::User
        );
        assert!(choose(&BOTH, true, None, None).is_none());
    }

    #[test]
    fn accounts_never_serialize_tokens_or_their_owner() {
        let value = serde_json::to_value(account(AccountStatus::Linked, true)).unwrap();
        let keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "id",
                "profile",
                "profiles",
                "status",
                "error",
                "refreshed",
                "created"
            ]
        );
    }
}
