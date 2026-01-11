use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

use crate::AppState;

// Health check
pub async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

// Server endpoints

#[derive(Deserialize)]
pub struct AddServerRequest {
    pub name: String,
    pub address: String,
}

pub async fn add_server(
    State(state): State<AppState>,
    Json(req): Json<AddServerRequest>,
) -> impl IntoResponse {
    match state.db.add_server(&req.name, &req.address).await {
        Ok(server) => (StatusCode::CREATED, Json(server)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn list_servers(State(state): State<AppState>) -> impl IntoResponse {
    match state.db.list_servers().await {
        Ok(servers) => Json(servers).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn remove_server(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.remove_server(&id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

// Tenant endpoints

#[derive(Deserialize)]
pub struct AddTenantRequest {
    pub id: String,
    pub server: Option<String>,
    pub config: Option<String>,
}

pub async fn add_tenant(
    State(state): State<AppState>,
    Json(req): Json<AddTenantRequest>,
) -> impl IntoResponse {
    match state
        .db
        .add_tenant(&req.id, req.server.as_deref(), req.config.as_deref())
        .await
    {
        Ok(tenant) => (StatusCode::CREATED, Json(tenant)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn list_tenants(State(state): State<AppState>) -> impl IntoResponse {
    match state.db.list_tenants().await {
        Ok(tenants) => Json(tenants).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn remove_tenant(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.remove_tenant(&id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}
