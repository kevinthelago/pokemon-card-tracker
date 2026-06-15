use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    app::AppState,
    error::AppError,
    jobs::{enqueue_initial_sync, InitialSyncJob},
    pos::{
        adapter::Provider,
        connect::get_adapter,
        connection::{ConnectionRepo, PosConnectionDto},
    },
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/workspaces/:wid/pos/connect/:provider",
            post(start_connect),
        )
        .route(
            "/api/workspaces/:wid/pos/connections",
            get(list_connections),
        )
        .route(
            "/api/workspaces/:wid/pos/connections/:id",
            delete(disconnect),
        )
        .route("/pos/oauth/callback", get(oauth_callback))
        .route("/webhooks/pos/:provider", post(webhook_receiver))
}

#[derive(Deserialize)]
struct OAuthCallbackParams {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Serialize)]
struct StartConnectResponse {
    authorization_url: String,
}

#[derive(Serialize)]
struct ListConnectionsResponse {
    connections: Vec<PosConnectionDto>,
}

async fn start_connect(
    State(state): State<AppState>,
    Path((workspace_id, provider_str)): Path<(Uuid, String)>,
) -> Result<Json<StartConnectResponse>, AppError> {
    let provider = Provider::from_str_ci(&provider_str)
        .ok_or_else(|| AppError::BadRequest(format!("Unknown POS provider: {provider_str}")))?;

    let adapter = get_adapter(provider);
    let redirect_uri = oauth_redirect_uri(&state.base_url, &provider_str);
    let oauth = adapter.oauth_start(workspace_id, &redirect_uri);

    let repo = ConnectionRepo::new(state.pool.clone(), state.encryption_key.clone());
    repo.create_pending(
        workspace_id,
        provider.as_str(),
        &oauth.state,
        &oauth.pkce_verifier,
    )
    .await?;

    Ok(Json(StartConnectResponse {
        authorization_url: oauth.authorization_url,
    }))
}

async fn oauth_callback(
    State(state): State<AppState>,
    Query(params): Query<OAuthCallbackParams>,
) -> impl IntoResponse {
    if let Some(error) = params.error {
        let desc = params.error_description.unwrap_or_else(|| error.clone());
        tracing::warn!("OAuth denied: {error}: {desc}");
        return Redirect::to("/settings/pos?error=access_denied").into_response();
    }

    let code = match params.code {
        Some(c) => c,
        None => return Redirect::to("/settings/pos?error=missing_code").into_response(),
    };
    let oauth_state = match params.state {
        Some(s) => s,
        None => return Redirect::to("/settings/pos?error=missing_state").into_response(),
    };

    match process_oauth_callback(state, code, oauth_state).await {
        Ok(()) => Redirect::to("/settings/pos?connected=1").into_response(),
        Err(e) => {
            tracing::error!("OAuth callback error: {e}");
            Redirect::to("/settings/pos?error=callback_failed").into_response()
        }
    }
}

async fn process_oauth_callback(
    state: AppState,
    code: String,
    oauth_state: String,
) -> Result<(), AppError> {
    let repo = ConnectionRepo::new(state.pool.clone(), state.encryption_key.clone());

    let conn = repo
        .find_by_state(&oauth_state)
        .await?
        .ok_or_else(|| AppError::BadRequest("Unknown or expired OAuth state".into()))?;

    let provider = Provider::from_str_ci(&conn.provider).ok_or_else(|| {
        AppError::Other(anyhow::anyhow!("Invalid provider in DB: {}", conn.provider))
    })?;

    let adapter = get_adapter(provider);
    let redirect_uri = oauth_redirect_uri(&state.base_url, &conn.provider);
    let pkce_verifier = conn.pkce_verifier.as_deref().unwrap_or("");

    let tokens = match adapter
        .oauth_exchange(&code, pkce_verifier, &redirect_uri)
        .await
    {
        Ok(t) => t,
        Err(e) => {
            let _ = repo.set_error(conn.id, &e.to_string()).await;
            return Err(e);
        }
    };

    let notification_url = format!("{}/webhooks/pos/{}", state.base_url, conn.provider);
    let webhook_id = adapter
        .register_webhook(&tokens.access_token, &notification_url)
        .await
        .unwrap_or(None);
    let uses_polling = webhook_id.is_none();

    if tokens.merchant_id.is_some() {
        repo.upsert_by_merchant(
            conn.workspace_id,
            &conn.provider,
            &tokens,
            webhook_id,
            uses_polling,
        )
        .await?;
    } else {
        repo.activate(conn.id, &tokens, webhook_id, uses_polling)
            .await?;
    }

    enqueue_initial_sync(InitialSyncJob {
        connection_id: conn.id,
        workspace_id: conn.workspace_id,
        provider: conn.provider.clone(),
    });

    Ok(())
}

async fn list_connections(
    State(state): State<AppState>,
    Path(workspace_id): Path<Uuid>,
) -> Result<Json<ListConnectionsResponse>, AppError> {
    let repo = ConnectionRepo::new(state.pool.clone(), state.encryption_key.clone());
    let connections = repo.list_active(workspace_id).await?;
    let dtos = connections.iter().map(PosConnectionDto::from).collect();
    Ok(Json(ListConnectionsResponse { connections: dtos }))
}

async fn disconnect(
    State(state): State<AppState>,
    Path((workspace_id, id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    let repo = ConnectionRepo::new(state.pool.clone(), state.encryption_key.clone());

    let conn = repo
        .find_by_id(id, workspace_id)
        .await?
        .ok_or(AppError::NotFound)?;

    if let Ok(access_token) = repo.decrypt_access_token(&conn) {
        let provider = Provider::from_str_ci(&conn.provider).unwrap_or(Provider::Square);
        let adapter = get_adapter(provider);
        let _ = adapter.revoke_token(&access_token).await;
    }

    repo.disconnect(id, workspace_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn webhook_receiver(
    State(state): State<AppState>,
    Path(provider_str): Path<String>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let provider = match Provider::from_str_ci(&provider_str) {
        Some(p) => p,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "unknown provider"})),
            )
                .into_response()
        }
    };

    let signing_key = match provider {
        Provider::Square => std::env::var("SQUARE_WEBHOOK_SIGNATURE_KEY").unwrap_or_default(),
        Provider::Shopify => std::env::var("SHOPIFY_API_SECRET").unwrap_or_default(),
        Provider::Clover => String::new(),
    };

    let adapter = get_adapter(provider);
    if !signing_key.is_empty() && !adapter.verify_webhook_signature(&headers, &body, &signing_key) {
        return (StatusCode::UNAUTHORIZED, "Invalid signature").into_response();
    }

    tracing::info!(provider = %provider_str, bytes = body.len(), "POS webhook received");
    let _ = state;

    StatusCode::ACCEPTED.into_response()
}

fn oauth_redirect_uri(base_url: &str, provider: &str) -> String {
    format!("{}/pos/oauth/callback?provider={}", base_url, provider)
}
