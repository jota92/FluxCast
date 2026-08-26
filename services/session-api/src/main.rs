#![forbid(unsafe_code)]

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::post,
};
use serde::Serialize;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    sessions: Arc<Mutex<HashMap<String, Session>>>,
}
#[derive(Clone)]
struct Session {
    invite: String,
    publisher_token: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Created {
    session_id: String,
    invite_token: String,
    join_url: String,
    web_transport_url: String,
    expires_at: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Exchange {
    session_id: String,
    publisher_token: String,
    edge: Edge,
    expires_in: u16,
    requirements: Requirements,
}
#[derive(Serialize)]
struct Edge {
    url: String,
}
#[derive(Serialize)]
struct Requirements {
    width: u16,
    height: u16,
    fps: u8,
}

#[tokio::main]
async fn main() {
    let state = AppState {
        sessions: Arc::new(Mutex::new(HashMap::new())),
    };
    let app = Router::new()
        .route("/api/v1/sessions", post(create))
        .route("/api/v1/join/exchange/:invite", post(exchange))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080")
        .await
        .expect("bind session api");
    axum::serve(listener, app).await.expect("serve");
}
async fn create(State(state): State<AppState>) -> Json<Created> {
    let session_id = format!("fc_s_{}", Uuid::new_v4().simple());
    let invite = Uuid::new_v4().simple().to_string();
    let publisher_token = Uuid::new_v4().simple().to_string();
    state.sessions.lock().await.insert(
        session_id.clone(),
        Session {
            invite: invite.clone(),
            publisher_token,
        },
    );
    Json(Created {
        session_id,
        invite_token: invite.clone(),
        join_url: format!("/j/{invite}"),
        web_transport_url: "https://edge.example.invalid:443/fc".to_string(),
        expires_at: "short-lived; configure Edge URL in deployment".to_string(),
    })
}
async fn exchange(
    Path(invite): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Exchange>, StatusCode> {
    let sessions = state.sessions.lock().await;
    let (id, session) = sessions
        .iter()
        .find(|(_, session)| session.invite == invite)
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(Exchange {
        session_id: id.clone(),
        publisher_token: session.publisher_token.clone(),
        edge: Edge {
            url: "https://edge.example.invalid:443/fc".to_string(),
        },
        expires_in: 600,
        requirements: Requirements {
            width: 1920,
            height: 1080,
            fps: 30,
        },
    }))
}
