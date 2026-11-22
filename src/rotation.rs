//! Refresh token rotation. Every refresh returns a new refresh token and invalidates the old one,
//! so two refreshes of the same account must never run at the same time: the second would send a
//! token the first already burned, and the account would need linking again. Refreshes of one
//! account are serialized twice: by an in-process lock per account ([`KeyedLocks`]), and by the
//! row lock the database vault holds while it refreshes (for panels with several instances).
use crate::hytale::{HytaleError, Secret, TokenOutcome};
use chrono::{DateTime, Duration, Utc};
use std::{
    collections::HashMap,
    hash::Hash,
    sync::{Arc, Mutex, Weak},
};

/// An access token is refreshed when it has less than this left.
pub const ACCESS_MARGIN: Duration = Duration::minutes(5);

pub struct StoredTokens {
    pub refresh: Secret,
    pub access: Option<(Secret, DateTime<Utc>)>,
}

/// Where an account's tokens live while one refresh holds them.
#[async_trait::async_trait]
pub trait Vault: Send {
    async fn load(&mut self) -> Result<StoredTokens, anyhow::Error>;
    /// Stores rotated tokens. Must be durable before the new access token is used.
    async fn store(
        &mut self,
        tokens: &StoredTokens,
        now: DateTime<Utc>,
    ) -> Result<(), anyhow::Error>;
    /// The refresh token is dead: the account needs linking again.
    async fn needs_relink(&mut self, message: &str) -> Result<(), anyhow::Error>;
}

#[async_trait::async_trait]
pub trait Refresher: Sync {
    async fn refresh(&self, refresh_token: &Secret) -> TokenOutcome;
}

#[derive(Debug)]
pub enum RotateError {
    Hytale(HytaleError),
    Storage(anyhow::Error),
}

impl From<anyhow::Error> for RotateError {
    fn from(err: anyhow::Error) -> Self {
        Self::Storage(err)
    }
}

/// A valid access token: the stored one while it has more than [`ACCESS_MARGIN`] left (unless
/// `force`), otherwise a refreshed one, stored before it is returned. Call it with the account's
/// lock held.
pub async fn fresh_access<V: Vault, R: Refresher>(
    vault: &mut V,
    refresher: &R,
    now: DateTime<Utc>,
    force: bool,
) -> Result<Secret, RotateError> {
    let stored = vault.load().await?;
    if !force
        && let Some((access, expires)) = &stored.access
        && *expires - ACCESS_MARGIN > now
    {
        return Ok(access.clone());
    }
    match refresher.refresh(&stored.refresh).await {
        TokenOutcome::Tokens(tokens) => {
            let rotated = StoredTokens {
                // a server that does not rotate keeps the old refresh token valid
                refresh: tokens.refresh_token.unwrap_or(stored.refresh),
                access: Some((
                    tokens.access_token,
                    now + Duration::seconds(tokens.expires_in.min(86_400) as i64),
                )),
            };
            vault.store(&rotated, now).await?;
            Ok(rotated.access.map(|(access, _)| access).expect("set above"))
        }
        TokenOutcome::InvalidGrant(message) => {
            vault.needs_relink(&message).await?;
            Err(RotateError::Hytale(HytaleError::Relink(message)))
        }
        TokenOutcome::Expired | TokenOutcome::Denied => {
            let message = "the login expired or was revoked".to_string();
            vault.needs_relink(&message).await?;
            Err(RotateError::Hytale(HytaleError::Relink(message)))
        }
        TokenOutcome::Pending | TokenOutcome::SlowDown => Err(RotateError::Hytale(
            HytaleError::Unavailable("unexpected answer to a token refresh".into()),
        )),
        // the old refresh token was not used up: keep it for the next try
        TokenOutcome::Failed(err) => Err(RotateError::Hytale(err)),
    }
}

/// One async lock per key, dropped once nobody holds or waits for it.
pub struct KeyedLocks<K> {
    locks: Mutex<HashMap<K, Weak<tokio::sync::Mutex<()>>>>,
}

impl<K: Eq + Hash + Clone> Default for KeyedLocks<K> {
    fn default() -> Self {
        Self {
            locks: Mutex::new(HashMap::new()),
        }
    }
}

impl<K: Eq + Hash + Clone> KeyedLocks<K> {
    pub async fn lock(&self, key: &K) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = {
            let mut locks = self
                .locks
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            locks.retain(|_, lock| lock.strong_count() > 0);
            match locks.get(key).and_then(Weak::upgrade) {
                Some(lock) => lock,
                None => {
                    let lock = Arc::new(tokio::sync::Mutex::new(()));
                    locks.insert(key.clone(), Arc::downgrade(&lock));
                    lock
                }
            }
        };
        lock.lock_owned().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hytale::TokenSet;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// What Hytale does: only the newest refresh token works, and every use rotates it.
    struct FakeHytale {
        current: Mutex<u32>,
        refreshes: AtomicUsize,
        burned: AtomicUsize,
    }

    impl FakeHytale {
        fn new() -> Self {
            Self {
                current: Mutex::new(1),
                refreshes: AtomicUsize::new(0),
                burned: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl Refresher for FakeHytale {
        async fn refresh(&self, refresh_token: &Secret) -> TokenOutcome {
            // a slow network, so overlapping refreshes would really overlap
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let mut current = self.current.lock().unwrap();
            if refresh_token.expose() != format!("refresh-{current}") {
                self.burned.fetch_add(1, Ordering::SeqCst);
                return TokenOutcome::InvalidGrant("invalid_grant".into());
            }
            *current += 1;
            self.refreshes.fetch_add(1, Ordering::SeqCst);
            TokenOutcome::Tokens(TokenSet {
                access_token: Secret::new(format!("access-{current}")),
                refresh_token: Some(Secret::new(format!("refresh-{current}"))),
                expires_in: 3600,
            })
        }
    }

    #[derive(Default)]
    struct Row {
        refresh: String,
        access: Option<(String, DateTime<Utc>)>,
        relink: Option<String>,
    }

    /// The database row, read and written through clones like the real vault does.
    struct MemoryVault(Arc<Mutex<Row>>);

    #[async_trait::async_trait]
    impl Vault for MemoryVault {
        async fn load(&mut self) -> Result<StoredTokens, anyhow::Error> {
            let row = self.0.lock().unwrap();
            Ok(StoredTokens {
                refresh: Secret::new(row.refresh.clone()),
                access: row
                    .access
                    .as_ref()
                    .map(|(access, expires)| (Secret::new(access.clone()), *expires)),
            })
        }

        async fn store(
            &mut self,
            tokens: &StoredTokens,
            _now: DateTime<Utc>,
        ) -> Result<(), anyhow::Error> {
            let mut row = self.0.lock().unwrap();
            row.refresh = tokens.refresh.expose().to_string();
            row.access = tokens
                .access
                .as_ref()
                .map(|(access, expires)| (access.expose().to_string(), *expires));
            Ok(())
        }

        async fn needs_relink(&mut self, message: &str) -> Result<(), anyhow::Error> {
            self.0.lock().unwrap().relink = Some(message.to_string());
            Ok(())
        }
    }

    fn row(access: Option<(&str, DateTime<Utc>)>) -> Arc<Mutex<Row>> {
        Arc::new(Mutex::new(Row {
            refresh: "refresh-1".into(),
            access: access.map(|(token, expires)| (token.to_string(), expires)),
            relink: None,
        }))
    }

    #[tokio::test]
    async fn concurrent_starts_refresh_once_and_never_burn_the_token() {
        let hytale = Arc::new(FakeHytale::new());
        let row = row(None);
        let locks = Arc::new(KeyedLocks::<u32>::default());
        let now = Utc::now();

        let starts = (0..16).map(|_| {
            let (hytale, row, locks) = (hytale.clone(), row.clone(), locks.clone());
            tokio::spawn(async move {
                let _guard = locks.lock(&7).await;
                fresh_access(&mut MemoryVault(row), &*hytale, now, false)
                    .await
                    .map(|access| access.expose().to_string())
            })
        });
        for start in starts {
            assert_eq!(start.await.unwrap().unwrap(), "access-2");
        }
        assert_eq!(hytale.refreshes.load(Ordering::SeqCst), 1);
        assert_eq!(hytale.burned.load(Ordering::SeqCst), 0);
        let row = row.lock().unwrap();
        assert_eq!(row.refresh, "refresh-2");
        assert!(row.relink.is_none());
    }

    #[tokio::test]
    async fn without_the_lock_overlapping_refreshes_burn_the_token() {
        // the failure the lock prevents, so the test above cannot pass by accident
        let hytale = Arc::new(FakeHytale::new());
        let row = row(None);
        let now = Utc::now();
        let starts: Vec<_> = (0..4)
            .map(|_| {
                let (hytale, row) = (hytale.clone(), row.clone());
                tokio::spawn(async move {
                    fresh_access(&mut MemoryVault(row), &*hytale, now, false).await
                })
            })
            .collect();
        for start in starts {
            let _ = start.await.unwrap();
        }
        assert!(hytale.burned.load(Ordering::SeqCst) > 0);
    }

    #[tokio::test]
    async fn a_fresh_access_token_is_reused_and_force_refreshes() {
        let hytale = FakeHytale::new();
        let now = Utc::now();
        let row = row(Some(("access-1", now + Duration::minutes(30))));
        let access = fresh_access(&mut MemoryVault(row.clone()), &hytale, now, false)
            .await
            .unwrap();
        assert_eq!(access.expose(), "access-1");
        assert_eq!(hytale.refreshes.load(Ordering::SeqCst), 0);

        // within the margin it is refreshed
        let late = now + Duration::minutes(26);
        let access = fresh_access(&mut MemoryVault(row.clone()), &hytale, late, false)
            .await
            .unwrap();
        assert_eq!(access.expose(), "access-2");
        let expires = row.lock().unwrap().access.as_ref().unwrap().1;
        assert_eq!(expires, late + Duration::seconds(3600));

        let access = fresh_access(&mut MemoryVault(row.clone()), &hytale, late, true)
            .await
            .unwrap();
        assert_eq!(access.expose(), "access-3");
        assert_eq!(row.lock().unwrap().refresh, "refresh-3");
    }

    #[tokio::test]
    async fn a_dead_refresh_token_asks_for_relinking() {
        let hytale = FakeHytale::new();
        *hytale.current.lock().unwrap() = 9;
        let row = row(None);
        let result = fresh_access(&mut MemoryVault(row.clone()), &hytale, Utc::now(), false).await;
        assert!(matches!(
            result,
            Err(RotateError::Hytale(HytaleError::Relink(_)))
        ));
        assert_eq!(row.lock().unwrap().relink.as_deref(), Some("invalid_grant"));
        assert_eq!(
            row.lock().unwrap().refresh,
            "refresh-1",
            "nothing overwritten"
        );
    }

    #[tokio::test]
    async fn network_failures_keep_the_old_token() {
        struct Down;
        #[async_trait::async_trait]
        impl Refresher for Down {
            async fn refresh(&self, _: &Secret) -> TokenOutcome {
                TokenOutcome::Failed(HytaleError::Unavailable("timeout".into()))
            }
        }
        let row = row(None);
        let result = fresh_access(&mut MemoryVault(row.clone()), &Down, Utc::now(), false).await;
        assert!(matches!(
            result,
            Err(RotateError::Hytale(HytaleError::Unavailable(_)))
        ));
        let row = row.lock().unwrap();
        assert_eq!(row.refresh, "refresh-1");
        assert!(row.relink.is_none());
    }

    #[tokio::test]
    async fn locks_are_per_key_and_cleaned_up() {
        let locks = KeyedLocks::<u32>::default();
        let first = locks.lock(&1).await;
        // another key is not blocked
        let _second = tokio::time::timeout(std::time::Duration::from_millis(50), locks.lock(&2))
            .await
            .expect("other keys are independent");
        // the same key waits
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), locks.lock(&1))
                .await
                .is_err()
        );
        drop(first);
        drop(_second);
        let _again = locks.lock(&3).await;
        assert_eq!(locks.locks.lock().unwrap().len(), 1);
    }
}
