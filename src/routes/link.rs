//! Linking a Hytale account through the device code flow, shared by the host account (admin
//! routes) and users' own accounts (account routes). Responses never contain tokens or device
//! codes.
use crate::accounts::{self, AccessError, Owner};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::json;
use shared::{
    State,
    response::{ApiResponse, ApiResponseResult},
};

pub fn access_error(err: AccessError) -> ApiResponse {
    match err {
        AccessError::Hytale(err) => ApiResponse::error(format!("Hytale: {}", err.message()))
            .with_status(StatusCode::BAD_GATEWAY),
        AccessError::Panel(err) => ApiResponse::from(err),
    }
}

/// The account, the login in progress and, for the host account, how many servers use it.
pub async fn overview(state: &State, owner: Owner) -> Result<serde_json::Value, ApiResponse> {
    let account = accounts::get(&state.database, owner).await?;
    let servers = match (&account, owner) {
        (Some(account), Owner::Host) => {
            Some(accounts::servers_using(&state.database, account.id).await?)
        }
        _ => None,
    };
    let link = accounts::pending_link(&state.database, owner).await?;
    Ok(json!({ "account": account, "link": link, "servers": servers }))
}

pub async fn start(state: &State, owner: Owner, user: uuid::Uuid) -> ApiResponseResult {
    let link = accounts::start_link(state, owner, user)
        .await
        .map_err(access_error)?;
    ApiResponse::new_serialized(json!({ "link": link })).ok()
}

/// One poll of the device code login; the page calls it every `interval` seconds.
pub async fn poll(state: &State, owner: Owner) -> Result<accounts::PollResult, ApiResponse> {
    accounts::poll_link(state, owner)
        .await
        .map_err(access_error)
}

pub async fn cancel(state: &State, owner: Owner) -> ApiResponseResult {
    accounts::cancel_link(&state.database, owner).await?;
    ApiResponse::new_serialized(json!({})).ok()
}

#[derive(Deserialize)]
pub struct ProfileChoice {
    pub profile: uuid::Uuid,
}

pub async fn select_profile(state: &State, owner: Owner, profile: uuid::Uuid) -> ApiResponseResult {
    match accounts::set_profile(&state.database, owner, profile).await? {
        Some(account) => ApiResponse::new_serialized(json!({ "account": account })).ok(),
        None => ApiResponse::error("not a game profile of the linked account")
            .with_status(StatusCode::NOT_FOUND)
            .ok(),
    }
}

pub async fn reload_profiles(state: &State, owner: Owner) -> ApiResponseResult {
    match accounts::reload_profiles(state, owner)
        .await
        .map_err(access_error)?
    {
        Some(account) => ApiResponse::new_serialized(json!({ "account": account })).ok(),
        None => ApiResponse::error("no Hytale account is linked")
            .with_status(StatusCode::NOT_FOUND)
            .ok(),
    }
}

pub async fn unlink(state: &State, owner: Owner) -> ApiResponseResult {
    accounts::cancel_link(&state.database, owner).await?;
    accounts::delete(&state.database, owner).await?;
    ApiResponse::new_serialized(json!({})).ok()
}
