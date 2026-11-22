//! Per-server API, mounted at `/api/client/servers/{server}/hytale-auth`: which account the
//! server signs in with at its next start, how its last start went (`/status`), and the switch
//! for automatic login (`/automatic`, needs `hytale-auth.manage`). The panel resolves the server
//! and the user's access first.
use crate::{accounts, inject, servers, settings};
use axum::routing::{get, put};
use serde::Deserialize;
use serde_json::json;
use shared::{
    GetState, Payload, State,
    models::{
        server::{GetServer, GetServerActivityLogger},
        user::{GetPermissionManager, GetUser},
    },
    response::{ApiResponse, ApiResponseResult},
};
use utoipa_axum::router::OpenApiRouter;

pub const MANAGE: &str = "hytale-auth.manage";

async fn status(
    state: GetState,
    permissions: GetPermissionManager,
    user: GetUser,
    server: GetServer,
) -> ApiResponseResult {
    let hytale = inject::is_hytale(&server.startup, &server.image);
    if !hytale {
        return ApiResponse::new_serialized(json!({ "hytale": false })).ok();
    }
    let db = &state.database;
    let settings = settings::load(&state).await?;
    let automatic = servers::automatic(db, server.uuid).await?;
    let (own, host) = accounts::for_owner(db, server.owner.uuid).await?;
    let next = accounts::choose(&settings, automatic, own.as_ref(), host.as_ref()).map(
        |(source, account)| {
            json!({
                "source": source,
                "profile_name": account.profile.as_ref().map(|p| &p.username),
            })
        },
    );
    let is_owner = server.owner.uuid == user.uuid;
    // problems the owner can fix by linking again, shown next to the fallback
    let own_problem = own
        .as_ref()
        .filter(|account| is_owner && !account.usable())
        .map(|account| json!({ "status": account.status, "error": account.error }));
    let host_problem = host
        .as_ref()
        .filter(|account| settings.host_account && !account.usable())
        .map(|account| json!({ "status": account.status, "error": account.error }));
    let last = servers::last_start(db, server.uuid).await?;
    let can_manage = permissions.has_server_permission(MANAGE).is_ok();
    ApiResponse::new_serialized(json!({
        "hytale": true,
        "automatic": automatic,
        "can_manage": can_manage,
        "is_owner": is_owner,
        "user_accounts": settings.user_accounts,
        "owner_linked": own.is_some(),
        "host_linked": host.is_some() && settings.host_account,
        "next": next,
        "own_problem": own_problem,
        "host_problem": host_problem,
        "last": last,
    }))
    .ok()
}

#[derive(Deserialize)]
struct Automatic {
    automatic: bool,
}

async fn set_automatic(
    state: GetState,
    permissions: GetPermissionManager,
    server: GetServer,
    activity_logger: GetServerActivityLogger,
    Payload(data): Payload<Automatic>,
) -> ApiResponseResult {
    permissions.has_server_permission(MANAGE)?;
    servers::set_automatic(&state.database, server.uuid, data.automatic).await?;
    activity_logger
        .log(
            "server:hytale-auth.automatic",
            json!({ "automatic": data.automatic }),
        )
        .await;
    ApiResponse::new_serialized(json!({ "automatic": data.automatic })).ok()
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .route("/status", get(status))
        .route("/automatic", put(set_automatic))
        .with_state(state.clone())
}
