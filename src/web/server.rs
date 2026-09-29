use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use anyhow::Result;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use colored::*;
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;

use crate::core::config::ServerConfig;
use crate::core::process::ServerManager;
use crate::downloader::modrinth::ModrinthClient;
use crate::network::port_check::{generate_diagnostics_report, ServerCheckReport};

#[derive(RustEmbed)]
#[folder = "src/web/static/"]
struct Assets;

#[derive(Clone)]
pub struct AppState {
    pub server_dir: PathBuf,
    pub manager: Arc<ServerManager>,
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
}

#[derive(Deserialize)]
struct CommandPayload {
    command: String,
}

#[derive(Deserialize)]
struct ModPayload {
    slug: String,
}

#[derive(Deserialize)]
struct ModRemovePayload {
    filename: String,
}

#[derive(Serialize)]
struct StatusResponse {
    running: bool,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

pub async fn start_web_server(server_dir: &Path, port: u16, open_browser: bool) -> Result<()> {
    let manager = Arc::new(ServerManager::new());
    let state = AppState {
        server_dir: server_dir.to_path_buf(),
        manager,
    };

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/api/config", get(get_config))
        .route("/api/status", get(get_status))
        .route("/api/start", post(start_server))
        .route("/api/stop", post(stop_server))
        .route("/api/command", post(send_command))
        .route("/api/mods/search", get(search_mods))
        .route("/api/mods/install", post(install_mod))
        .route("/api/mods/list", get(list_mods))
        .route("/api/mods/remove", post(remove_mod))
        .route("/api/diagnostics", get(get_diagnostics))
        .route("/api/server/upgrade", post(upgrade_server))
        .route("/api/server/backup", post(trigger_backup))
        .route("/api/server/ram", post(update_ram))
        .fallback(get(static_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let url = format!("http://localhost:{}", port);

    println!("{}", "═══════════════════════════════════════════════════════════".cyan());
    println!("{}", "       🌐 GSMM WEB INTERFACE AVVIATA CON SUCCESSO         ".green().bold());
    println!("{}", "═══════════════════════════════════════════════════════════".cyan());
    println!("• Interfaccia locale:   {}", url.cyan().bold());
    println!("• Accessibile in LAN:   http://0.0.0.0:{}", port);
    println!("• Premi Ctrl+C nel terminale per arrestare il server web.");
    println!("{}", "═══════════════════════════════════════════════════════════\n".cyan());

    if open_browser {
        let _ = open::that(&url);
    }

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn static_handler(uri: axum::http::Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    match Assets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            ([(header::CONTENT_TYPE, HeaderValue::from_str(mime.as_ref()).unwrap())], content.data).into_response()
        }
        None => {
            if let Some(index) = Assets::get("index.html") {
                ([(header::CONTENT_TYPE, HeaderValue::from_static("text/html"))], index.data).into_response()
            } else {
                (StatusCode::NOT_FOUND, "404 Not Found").into_response()
            }
        }
    }
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut rx = state.manager.subscribe_logs();
    while let Ok(line) = rx.recv().await {
        if socket.send(Message::Text(line)).await.is_err() {
            break;
        }
    }
}

async fn get_config(State(state): State<AppState>) -> Result<Json<ServerConfig>, (StatusCode, Json<ErrorResponse>)> {
    match ServerConfig::load_from_dir(&state.server_dir).await {
        Ok(cfg) => Ok(Json(cfg)),
        Err(e) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse { error: e.to_string() }),
        )),
    }
}

async fn get_status(State(state): State<AppState>) -> Json<StatusResponse> {
    let running = state.manager.is_running().await;
    Json(StatusResponse { running })
}

async fn start_server(State(state): State<AppState>) -> Result<Json<StatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    let config = ServerConfig::load_from_dir(&state.server_dir).await.map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() }))
    })?;

    state.manager.start(&state.server_dir, &config).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() }))
    })?;

    Ok(Json(StatusResponse { running: true }))
}

async fn stop_server(State(state): State<AppState>) -> Result<Json<StatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    state.manager.stop().await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() }))
    })?;
    Ok(Json(StatusResponse { running: false }))
}

async fn send_command(
    State(state): State<AppState>,
    Json(payload): Json<CommandPayload>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    state.manager.send_command(&payload.command).await.map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() }))
    })?;
    Ok(StatusCode::OK)
}

async fn search_mods(
    State(state): State<AppState>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Vec<crate::downloader::modrinth::ModrinthSearchHit>>, (StatusCode, Json<ErrorResponse>)> {
    let config = ServerConfig::load_from_dir(&state.server_dir).await.ok();
    let mc_ver = config.as_ref().map(|c| c.mc_version.as_str());
    let loader_str = config.as_ref().map(|c| c.loader.to_string());
    let loader_ref = loader_str.as_deref();

    let modrinth = ModrinthClient::new();
    match modrinth.search_mods(&params.q, mc_ver, loader_ref, 8).await {
        Ok(hits) => Ok(Json(hits)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() }))),
    }
}

async fn install_mod(
    State(state): State<AppState>,
    Json(payload): Json<ModPayload>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<ErrorResponse>)> {
    let config = ServerConfig::load_from_dir(&state.server_dir).await.map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() }))
    })?;

    let loader_str = format!("{}", config.loader).to_lowercase();
    let mods_dir = state.server_dir.join("mods");
    let modrinth = ModrinthClient::new();
    let mut installed = HashSet::new();

    match modrinth.install_mod_with_dependencies(&payload.slug, &config.mc_version, &loader_str, &mods_dir, &mut installed).await {
        Ok(files) => Ok(Json(files)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() }))),
    }
}

async fn list_mods(State(state): State<AppState>) -> Result<Json<Vec<String>>, (StatusCode, Json<ErrorResponse>)> {
    let mods_dir = state.server_dir.join("mods");
    match ModrinthClient::list_installed_mods(&mods_dir).await {
        Ok(list) => Ok(Json(list)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() }))),
    }
}

async fn remove_mod(
    State(state): State<AppState>,
    Json(payload): Json<ModRemovePayload>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let mods_dir = state.server_dir.join("mods");
    let target = mods_dir.join(&payload.filename);
    if target.exists() {
        let _ = tokio::fs::remove_file(target).await;
        Ok(StatusCode::OK)
    } else {
        Err((StatusCode::NOT_FOUND, Json(ErrorResponse { error: "File non trovato".to_string() })))
    }
}

async fn get_diagnostics(State(state): State<AppState>) -> Json<ServerCheckReport> {
    let config_port = ServerConfig::load_from_dir(&state.server_dir)
        .await
        .map(|c| c.port)
        .unwrap_or(25565);
    let report = generate_diagnostics_report(config_port).await;
    Json(report)
}

#[derive(Deserialize)]
struct UpgradeRequest {
    version: String,
    loader: Option<String>,
    backup: bool,
}

#[derive(Serialize)]
struct MessageResponse {
    message: String,
}

#[derive(Deserialize)]
struct RamRequest {
    ram_mb: u32,
}

async fn upgrade_server(
    State(state): State<AppState>,
    Json(payload): Json<UpgradeRequest>,
) -> Result<Json<MessageResponse>, (StatusCode, Json<ErrorResponse>)> {
    if state.manager.is_running().await {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error: "Arresta il server prima di effettuare l'aggiornamento!".to_string() }),
        ));
    }

    let loader_enum = if let Some(ref l) = payload.loader {
        match l.parse::<crate::core::config::LoaderType>() {
            Ok(parsed) => Some(parsed),
            Err(e) => return Err((StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() }))),
        }
    } else {
        None
    };

    match crate::cli::commands::upgrade_server_core(&state.server_dir, &payload.version, loader_enum, payload.backup).await {
        Ok(msg) => Ok(Json(MessageResponse { message: msg })),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() }))),
    }
}

async fn trigger_backup(
    State(state): State<AppState>,
) -> Result<Json<MessageResponse>, (StatusCode, Json<ErrorResponse>)> {
    match crate::core::backup::create_world_backup(&state.server_dir) {
        Ok(p) => Ok(Json(MessageResponse { message: format!("Backup creato con successo: {}", p.display()) })),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() }))),
    }
}

async fn update_ram(
    State(state): State<AppState>,
    Json(payload): Json<RamRequest>,
) -> Result<Json<MessageResponse>, (StatusCode, Json<ErrorResponse>)> {
    let mut config = ServerConfig::load_from_dir(&state.server_dir).await.map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() }))
    })?;

    config.ram_max_mb = payload.ram_mb;
    config.ram_min_mb = (payload.ram_mb / 2).max(1024);
    config.save_to_dir(&state.server_dir).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() }))
    })?;

    Ok(Json(MessageResponse { message: format!("RAM aggiornata a {} MB (attiva al prossimo avvio)", payload.ram_mb) }))
}
