//! Admin API, mounted at `/api/admin/extensions/dev.caloptreyx.hytaleauth`: the settings and the
//! host account. Reading needs `extensions.read`, changing `extensions.manage`.
use super::link::{self, ProfileChoice};
use crate::{
    accounts::{Owner, PollResult},
    settings::{self, Settings},
};
use axum::routing::{get, post, put};
use serde_json::json;
use shared::{
    GetState, Payload, State,
    models::{
        admin_activity::GetAdminActivityLogger,
        user::{GetPermissionManager, GetUser},
    },
    response::{ApiResponse, ApiResponseResult},
};
use utoipa_axum::router::OpenApiRouter;

const READ: &str = "extensions.read";
const MANAGE: &str = "extensions.manage";

async fn get_settings(state: GetState, permissions: GetPermissionManager) -> ApiResponseResult {
    permissions.has_admin_permission(READ)?;
    ApiResponse::new_serialized(json!({ "settings": settings::load(&state).await? })).ok()
}

async fn put_settings(
    state: GetState,
    permissions: GetPermissionManager,
    activity_logger: GetAdminActivityLogger,
    Payload(data): Payload<Settings>,
) -> ApiResponseResult {
    permissions.has_admin_permission(MANAGE)?;
    {
        let mut panel_settings = state.settings.get_mut().await?;
        let stored: &mut Settings = panel_settings.get_mut_extension_settings(crate::PACKAGE)?;
        *stored = data.clone();
        panel_settings.save().await?;
    }
    activity_logger
        .log(
            "settings:extensions:update",
            json!({ "extension": crate::PACKAGE, "settings": data }),
        )
        .await;
    ApiResponse::new_serialized(json!({ "settings": data })).ok()
}

async fn host(state: GetState, permissions: GetPermissionManager) -> ApiResponseResult {
    permissions.has_admin_permission(READ)?;
    ApiResponse::new_serialized(link::overview(&state, Owner::Host).await?).ok()
}

async fn start_link(
    state: GetState,
    permissions: GetPermissionManager,
    user: GetUser,
) -> ApiResponseResult {
    permissions.has_admin_permission(MANAGE)?;
    link::start(&state, Owner::Host, user.uuid).await
}

async fn poll_link(
    state: GetState,
    permissions: GetPermissionManager,
    activity_logger: GetAdminActivityLogger,
) -> ApiResponseResult {
    permissions.has_admin_permission(MANAGE)?;
    let result = link::poll(&state, Owner::Host).await?;
    if let PollResult::Linked { account } = &result {
        activity_logger
            .log(
                "extensions:hytale-auth:host.link",
                json!({ "profile": account.profile.as_ref().map(|p| &p.username) }),
            )
            .await;
    }
    ApiResponse::new_serialized(result).ok()
}

async fn cancel_link(state: GetState, permissions: GetPermissionManager) -> ApiResponseResult {
    permissions.has_admin_permission(MANAGE)?;
    link::cancel(&state, Owner::Host).await
}

async fn select_profile(
    state: GetState,
    permissions: GetPermissionManager,
    activity_logger: GetAdminActivityLogger,
    Payload(choice): Payload<ProfileChoice>,
) -> ApiResponseResult {
    permissions.has_admin_permission(MANAGE)?;
    let response = link::select_profile(&state, Owner::Host, choice.profile).await?;
    activity_logger
        .log(
            "extensions:hytale-auth:host.profile",
            json!({ "profile": choice.profile }),
        )
        .await;
    Ok(response)
}

async fn reload_profiles(state: GetState, permissions: GetPermissionManager) -> ApiResponseResult {
    permissions.has_admin_permission(MANAGE)?;
    link::reload_profiles(&state, Owner::Host).await
}

async fn unlink(
    state: GetState,
    permissions: GetPermissionManager,
    activity_logger: GetAdminActivityLogger,
) -> ApiResponseResult {
    permissions.has_admin_permission(MANAGE)?;
    let response = link::unlink(&state, Owner::Host).await?;
    activity_logger
        .log("extensions:hytale-auth:host.unlink", json!({}))
        .await;
    Ok(response)
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .route("/settings", get(get_settings).put(put_settings))
        .route("/host", get(host).delete(unlink))
        .route("/host/link", post(start_link).delete(cancel_link))
        .route("/host/link/poll", post(poll_link))
        .route("/host/profile", put(select_profile))
        .route("/host/profiles", post(reload_profiles))
        .with_state(state.clone())
}
