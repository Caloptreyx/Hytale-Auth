//! A user's own Hytale account, mounted at `/api/client/hytale-auth/account`. It signs in the
//! servers the user owns, instead of the host account. Linking needs the `user_accounts` setting.
use super::link::{self, ProfileChoice};
use crate::{
    accounts::{Owner, PollResult},
    settings,
};
use axum::{
    http::StatusCode,
    routing::{get, post, put},
};
use serde_json::json;
use shared::{
    GetState, Payload, State,
    models::{user::GetUser, user_activity::GetUserActivityLogger},
    response::{ApiResponse, ApiResponseResult},
};
use utoipa_axum::router::OpenApiRouter;

async fn enabled(state: &State) -> Result<(), ApiResponse> {
    if settings::load(state).await?.user_accounts {
        Ok(())
    } else {
        Err(
            ApiResponse::error("linking your own Hytale account is turned off on this panel")
                .with_status(StatusCode::FORBIDDEN),
        )
    }
}

/// The user's account and login in progress, whether linking is allowed, and whether servers
/// without an account of their own fall back to the host account.
async fn overview(state: GetState, user: GetUser) -> ApiResponseResult {
    let settings = settings::load(&state).await?;
    let mut value = link::overview(&state, Owner::User(user.uuid)).await?;
    value["enabled"] = settings.user_accounts.into();
    value["host_account"] = settings.host_account.into();
    ApiResponse::new_serialized(value).ok()
}

async fn start_link(state: GetState, user: GetUser) -> ApiResponseResult {
    enabled(&state).await?;
    link::start(&state, Owner::User(user.uuid), user.uuid).await
}

async fn poll_link(
    state: GetState,
    user: GetUser,
    activity_logger: GetUserActivityLogger,
) -> ApiResponseResult {
    enabled(&state).await?;
    let result = link::poll(&state, Owner::User(user.uuid)).await?;
    if let PollResult::Linked { account } = &result {
        activity_logger
            .log(
                "hytale-auth:account.link",
                json!({ "profile": account.profile.as_ref().map(|p| &p.username) }),
            )
            .await;
    }
    ApiResponse::new_serialized(result).ok()
}

async fn cancel_link(state: GetState, user: GetUser) -> ApiResponseResult {
    link::cancel(&state, Owner::User(user.uuid)).await
}

async fn select_profile(
    state: GetState,
    user: GetUser,
    Payload(choice): Payload<ProfileChoice>,
) -> ApiResponseResult {
    link::select_profile(&state, Owner::User(user.uuid), choice.profile).await
}

async fn reload_profiles(state: GetState, user: GetUser) -> ApiResponseResult {
    link::reload_profiles(&state, Owner::User(user.uuid)).await
}

async fn unlink(
    state: GetState,
    user: GetUser,
    activity_logger: GetUserActivityLogger,
) -> ApiResponseResult {
    let response = link::unlink(&state, Owner::User(user.uuid)).await?;
    activity_logger
        .log("hytale-auth:account.unlink", json!({}))
        .await;
    Ok(response)
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .route("/account", get(overview).delete(unlink))
        .route("/account/link", post(start_link).delete(cancel_link))
        .route("/account/link/poll", post(poll_link))
        .route("/account/profile", put(select_profile))
        .route("/account/profiles", post(reload_profiles))
        .with_state(state.clone())
}
