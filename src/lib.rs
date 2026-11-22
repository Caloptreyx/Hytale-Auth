use indexmap::IndexMap;
use shared::{
    State,
    extensions::{
        Extension, ExtensionPermissionsBuilder, ExtensionRouteBuilder, ExtensionUpdateInfo,
        background_tasks::BackgroundTaskBuilder, index_html::IndexHtmlBuilder,
        settings::ExtensionSettingsDeserializer,
    },
    permissions::PermissionGroup,
};
use std::{sync::Arc, time::Duration};

mod accounts;
mod hytale;
mod inject;
mod rotation;
mod routes;
mod servers;
mod settings;

pub const PACKAGE: &str = "dev.caloptreyx.hytaleauth";

const LATEST_RELEASE: &str = "https://api.github.com/repos/Caloptreyx/Hytale-Auth/releases/latest";

/// Refresh tokens die after 30 days unused; accounts no server started with for this long are
/// refreshed by the panel, which extends them.
const KEEPALIVE_AFTER: chrono::Duration = chrono::Duration::days(7);

/// The subset of GitHub's release payload the updates page needs.
#[derive(serde::Serialize, serde::Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    body: String,
}

fn version_from_tag(tag: &str) -> Result<semver::Version, semver::Error> {
    semver::Version::parse(tag.trim_start_matches('v'))
}

/// Keeps idle logins alive and drops abandoned device code logins, once an hour.
async fn keepalive(state: &State) -> Result<(), anyhow::Error> {
    accounts::delete_expired_links(&state.database).await?;
    for id in accounts::idle_accounts(&state.database, KEEPALIVE_AFTER).await? {
        match accounts::access_token(state, id, true).await {
            Ok(_) => tracing::debug!("Hytale Auth: refreshed the idle login of account {id}"),
            Err(accounts::AccessError::Hytale(err)) => {
                tracing::warn!(
                    "Hytale Auth: could not refresh the idle login of account {id} ({}): {}",
                    err.code(),
                    err.message()
                );
                if !matches!(err, hytale::HytaleError::Relink(_)) {
                    accounts::record_error(&state.database, id, &err).await;
                }
            }
            Err(accounts::AccessError::Panel(err)) => {
                tracing::warn!(
                    "Hytale Auth: could not refresh the idle login of account {id}: {err:#}"
                );
            }
        }
    }
    tokio::time::sleep(Duration::from_secs(60 * 60)).await;
    Ok(())
}

#[derive(Default)]
pub struct ExtensionStruct;

#[async_trait::async_trait]
impl Extension for ExtensionStruct {
    async fn initialize(&mut self, _state: State) {
        inject::register();
    }

    async fn initialize_router(
        &mut self,
        state: State,
        builder: ExtensionRouteBuilder,
    ) -> ExtensionRouteBuilder {
        builder
            .add_client_server_api_router(|router| {
                router.nest("/hytale-auth", routes::server::router(&state))
            })
            .add_client_api_router(|router| {
                router.nest("/hytale-auth", routes::account::router(&state))
            })
            .add_admin_api_router(|router| {
                router.nest(
                    &format!("/extensions/{PACKAGE}"),
                    routes::admin::router(&state),
                )
            })
    }

    async fn initialize_permissions(
        &mut self,
        _state: State,
        builder: ExtensionPermissionsBuilder,
    ) -> ExtensionPermissionsBuilder {
        builder.add_server_permission_group(
            "hytale-auth",
            PermissionGroup {
                description: "Permissions for the automatic Hytale login of Hytale servers.",
                permissions: IndexMap::from([(
                    "manage",
                    "Allows turning the automatic login with a linked Hytale account on or off for the server.",
                )]),
            },
        )
    }

    async fn initialize_background_tasks(
        &mut self,
        _state: State,
        builder: BackgroundTaskBuilder,
    ) -> BackgroundTaskBuilder {
        builder
            .add_task("hytale-auth-keepalive", |state| async move {
                keepalive(&state).await
            })
            .await;
        builder
    }

    async fn initialize_index_html(
        &mut self,
        _state: State,
        builder: IndexHtmlBuilder,
    ) -> IndexHtmlBuilder {
        // whether the account page for linking one's own Hytale account is shown
        builder.add_variable("userAccounts", |app| {
            Ok(serde_json::Value::Bool(
                app.get_extension_settings::<settings::Settings>(PACKAGE)
                    .map(|settings| settings.user_accounts)
                    .unwrap_or(true),
            ))
        });
        builder
    }

    async fn settings_deserializer(&self, _state: State) -> ExtensionSettingsDeserializer {
        Arc::new(settings::SettingsDataDeserializer)
    }

    async fn check_for_updates(
        &self,
        state: State,
        current_version: &semver::Version,
    ) -> Result<Option<ExtensionUpdateInfo>, anyhow::Error> {
        // GitHub allows 60 unauthenticated requests per hour, and the panel rechecks on every
        // start and every 12 hours.
        let release: Release = state
            .cache
            .cached(
                "dev.caloptreyx.hytaleauth::latest-release",
                60 * 60,
                || async {
                    let response = state.client.get(LATEST_RELEASE).send().await?;
                    Ok::<Release, anyhow::Error>(
                        response.error_for_status()?.json::<Release>().await?,
                    )
                },
            )
            .await?;

        let latest = version_from_tag(&release.tag_name)?;

        Ok((latest > *current_version).then(|| ExtensionUpdateInfo {
            version: latest,
            changes: release
                .body
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(compact_str::CompactString::from)
                .collect(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_from_tag_accepts_prefixed_and_bare_tags() {
        assert_eq!(version_from_tag("v0.1.0").unwrap().to_string(), "0.1.0");
        assert_eq!(version_from_tag("1.2.3").unwrap().to_string(), "1.2.3");
        assert!(version_from_tag("release-1").is_err());
    }
}
