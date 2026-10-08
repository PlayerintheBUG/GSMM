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
    http::{header, HeaderValue, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use colored::*;
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};
use tower_http::cors::CorsLayer;
use uuid::Uuid;

use crate::core::config::ServerConfig;
use crate::core::process::ServerManager;
use crate::downloader::modrinth::ModrinthClient;
use crate::network::port_check::{generate_diagnostics_report, ServerCheckReport};
use crate::registry::Registry;

#[derive(RustEmbed)]
#[folder = "src/web/static/"]
struct Assets;

// ─── Stato condiviso ─────────────────────────────────────────────────────────

/// Stato globale della Web UI:
/// - `token`: token di accesso generato all'avvio (cambia a ogni `gsmm web`)
/// - `active_session_ip`: IP del dispositivo attualmente connesso (None = nessuno)
/// - `managers`: mappa nome_server → ServerManager per il multi-server
#[derive(Clone)]
pub struct AppState {
    /// Directory del server "locale" (usata quando gsmm web viene avviato dentro una cartella server)
    pub default_server_dir: Option<PathBuf>,
    /// Token di accesso generato all'avvio, richiesto per tutte le API e WebSocket
    pub token: Arc<String>,
    /// IP del solo dispositivo autorizzato a usare la Web UI (sessione singola)
    pub active_session_ip: Arc<Mutex<Option<String>>>,
    /// Manager per il server "locale" default
    pub manager: Arc<ServerManager>,
    /// Manager per i server del registro globale (nome → manager)
    pub registry_managers: Arc<RwLock<std::collections::HashMap<String, Arc<ServerManager>>>>,
}

// ─── Payload / Response types ────────────────────────────────────────────────

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    #[serde(default)]
    server: Option<String>,
}

#[derive(Deserialize)]
struct ServerQuery {
    server: Option<String>,
}

#[derive(Deserialize)]
struct CommandPayload {
    command: String,
    #[serde(default)]
    server: Option<String>,
}

#[derive(Deserialize)]
struct ModPayload {
    slug: String,
    #[serde(default)]
    server: Option<String>,
}

#[derive(Deserialize)]
struct ModRemovePayload {
    filename: String,
    #[serde(default)]
    server: Option<String>,
}

#[derive(Serialize)]
struct StatusResponse {
    running: bool,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Serialize)]
struct MessageResponse {
    message: String,
}

#[derive(Deserialize)]
struct UpgradeRequest {
    version: String,
    loader: Option<String>,
    backup: bool,
    #[serde(default)]
    server: Option<String>,
}

#[derive(Deserialize)]
struct RamRequest {
    ram_mb: u32,
    #[serde(default)]
    server: Option<String>,
}

#[derive(Deserialize)]
struct LoginPayload {
    token: String,
}

#[derive(Serialize)]
struct LoginResponse {
    ok: bool,
    message: String,
}

// Risposta per /api/servers
#[derive(Serialize)]
struct ServerListEntry {
    name: String,
    path: String,
    running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    mc_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    loader: Option<String>,
}

// ─── Helper ───────────────────────────────────────────────────────────────────

/// Restituisce la directory del server specificato (per nome nel registro o default).
async fn resolve_server_dir(state: &AppState, server_name: Option<&str>) -> Result<PathBuf, (StatusCode, Json<ErrorResponse>)> {
    if let Some(name) = server_name {
        let reg = Registry::load().map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() }))
        })?;
        let entry = reg.find_by_name(name).ok_or_else(|| {
            (StatusCode::NOT_FOUND, Json(ErrorResponse { error: format!("Server '{}' non trovato nel registro.", name) }))
        })?;
        Ok(entry.path.clone())
    } else {
        state.default_server_dir.clone().ok_or_else(|| {
            (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "Nessun server specificato e nessuna directory server di default.".to_string() }))
        })
    }
}

/// Restituisce il ServerManager per il server indicato.
async fn resolve_manager(state: &AppState, server_name: Option<&str>) -> Arc<ServerManager> {
    if let Some(name) = server_name {
        let managers = state.registry_managers.read().await;
        if let Some(mgr) = managers.get(name) {
            return mgr.clone();
        }
        // Non ancora in mappa → crea e inserisci
        drop(managers);
        let mgr = Arc::new(ServerManager::new());
        state.registry_managers.write().await.insert(name.to_string(), mgr.clone());
        mgr
    } else {
        state.manager.clone()
    }
}

// ─── Middleware autenticazione ────────────────────────────────────────────────

/// Middleware che verifica la presenza del token GSMM nell'header X-GSMM-Token.
/// Bypass per: GET / (index), GET /api/login (POST), GET /login (pagina), asset statici.
async fn auth_middleware(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let path = req.uri().path();

    // Percorsi pubblici (non richiedono token)
    let is_public = path == "/" || path == "/login" || path == "/api/login"
        || path.starts_with("/static/")
        || path.ends_with(".css")
        || path.ends_with(".js")
        || path.ends_with(".ico")
        || path.ends_with(".png")
        || path.ends_with(".svg");

    if is_public {
        return next.run(req).await;
    }

    // Leggi il token dall'header X-GSMM-Token
    let mut provided = req
        .headers()
        .get("X-GSMM-Token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    // Se non c'è nell'header, cerca nella query string (indispensabile per il WebSocket)
    if provided.is_empty() {
        if let Some(query) = req.uri().query() {
            for pair in query.split('&') {
                if let Some(val) = pair.strip_prefix("token=") {
                    provided = val.to_string();
                    break;
                }
            }
        }
    }

    if provided != state.token.as_str() {
        return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "Token non valido o mancante.".to_string() })).into_response();
    }

    next.run(req).await
}

// ─── Avvio server web ─────────────────────────────────────────────────────────

pub async fn start_web_server(server_dir: Option<&Path>, port: u16, open_browser: bool) -> Result<()> {
    // Genera token univoco per questa sessione
    let token = Uuid::new_v4().to_string();

    let manager = Arc::new(ServerManager::new());

    let state = AppState {
        default_server_dir: server_dir.map(|p| p.to_path_buf()),
        token: Arc::new(token.clone()),
        active_session_ip: Arc::new(Mutex::new(None)),
        manager,
        registry_managers: Arc::new(RwLock::new(std::collections::HashMap::new())),
    };

    let app = Router::new()
        // Auth
        .route("/api/login", post(login_handler))
        // Servers
        .route("/api/servers", get(list_servers))
        .route("/api/servers/register", post(register_server))
        .route("/api/servers/unregister", post(unregister_server))
        // Per-server operations (server specificato in query param o body)
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
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let url = format!("http://localhost:{}", port);

    println!("{}", "═══════════════════════════════════════════════════════════".cyan());
    println!("{}", "       🌐 GSMM WEB INTERFACE AVVIATA CON SUCCESSO         ".green().bold());
    println!("{}", "═══════════════════════════════════════════════════════════".cyan());
    println!("• Indirizzo:  {}", url.cyan().bold());
    println!("• Porta:      {}", port);
    println!();
    println!("{}", "🔑 TOKEN DI ACCESSO (copialo nella pagina di login):".yellow().bold());
    println!("  {}", token.white().bold());
    println!();
    println!("{}", "  ⚠️  Il token cambia ad ogni avvio della Web UI.".yellow());
    println!("{}", "  ⚠️  Solo 1 dispositivo per volta può connettersi.".yellow());
    println!("{}", "═══════════════════════════════════════════════════════════\n".cyan());

    if open_browser {
        let _ = open::that(&url);
    }

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await?;

    Ok(())
}

// ─── Handler statici ──────────────────────────────────────────────────────────

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

// ─── Login ────────────────────────────────────────────────────────────────────

/// POST /api/login — verifica il token e blocca se c'è già un dispositivo connesso
async fn login_handler(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    Json(payload): Json<LoginPayload>,
) -> Json<LoginResponse> {
    if payload.token != *state.token {
        return Json(LoginResponse {
            ok: false,
            message: "Token non valido.".to_string(),
        });
    }

    let client_ip = addr.ip().to_string();
    let mut session = state.active_session_ip.lock().await;

    match session.as_deref() {
        None => {
            // Nessuna sessione attiva → autorizza questo dispositivo
            *session = Some(client_ip.clone());
            Json(LoginResponse {
                ok: true,
                message: format!("Accesso autorizzato. Sessione attiva per {}.", client_ip),
            })
        }
        Some(existing_ip) if existing_ip == client_ip => {
            // Stesso dispositivo → già autorizzato
            Json(LoginResponse {
                ok: true,
                message: "Sessione già attiva per questo dispositivo.".to_string(),
            })
        }
        Some(existing_ip) => {
            // Dispositivo diverso → rifiuta
            Json(LoginResponse {
                ok: false,
                message: format!("Un altro dispositivo ({}) è già connesso alla Web UI.", existing_ip),
            })
        }
    }
}

// ─── WebSocket ────────────────────────────────────────────────────────────────

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(params): Query<ServerQuery>,
) -> Response {
    let server_name = params.server.clone();
    ws.on_upgrade(move |socket| handle_socket(socket, state, server_name))
}

async fn handle_socket(mut socket: WebSocket, state: AppState, server_name: Option<String>) {
    let manager = resolve_manager(&state, server_name.as_deref()).await;
    let mut rx = manager.subscribe_logs();
    while let Ok(line) = rx.recv().await {
        if socket.send(Message::Text(line)).await.is_err() {
            break;
        }
    }
}

// ─── Server list & registry ───────────────────────────────────────────────────

async fn list_servers(State(state): State<AppState>) -> Json<Vec<ServerListEntry>> {
    let reg = Registry::load().unwrap_or_default();
    let mut entries: Vec<ServerListEntry> = Vec::new();

    // Aggiungi il server default se esiste
    if let Some(ref dir) = state.default_server_dir {
        let cfg = ServerConfig::load_from_dir(dir).await.ok();
        let name = cfg.as_ref().map(|c| c.name.clone()).unwrap_or_else(|| "default".to_string());
        let running = state.manager.is_running().await;
        entries.push(ServerListEntry {
            name,
            path: dir.to_string_lossy().to_string(),
            running,
            mc_version: cfg.as_ref().map(|c| c.mc_version.clone()),
            loader: cfg.as_ref().map(|c| c.loader.to_string()),
        });
    }

    // Aggiungi i server del registro
    let managers = state.registry_managers.read().await;
    for server_entry in &reg.servers {
        // Salta se è lo stesso del default
        if let Some(ref dir) = state.default_server_dir {
            if server_entry.path == *dir { continue; }
        }
        let cfg = ServerConfig::load_from_dir(&server_entry.path).await.ok();
        let running = if let Some(mgr) = managers.get(&server_entry.name) {
            mgr.is_running().await
        } else {
            false
        };
        entries.push(ServerListEntry {
            name: server_entry.name.clone(),
            path: server_entry.path.to_string_lossy().to_string(),
            running,
            mc_version: cfg.as_ref().map(|c| c.mc_version.clone()),
            loader: cfg.as_ref().map(|c| c.loader.to_string()),
        });
    }

    Json(entries)
}

#[derive(Deserialize)]
struct RegisterPayload {
    name: String,
    path: String,
}

async fn register_server(
    Json(payload): Json<RegisterPayload>,
) -> Result<Json<MessageResponse>, (StatusCode, Json<ErrorResponse>)> {
    let path = PathBuf::from(&payload.path);
    if !path.exists() {
        return Err((StatusCode::BAD_REQUEST, Json(ErrorResponse {
            error: format!("Il percorso '{}' non esiste.", payload.path),
        })));
    }
    let mut reg = Registry::load().map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() }))
    })?;
    reg.register(&payload.name, &path).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() }))
    })?;
    Ok(Json(MessageResponse {
        message: format!("Server '{}' registrato con successo.", payload.name),
    }))
}

#[derive(Deserialize)]
struct UnregisterPayload {
    name: String,
}

async fn unregister_server(
    Json(payload): Json<UnregisterPayload>,
) -> Result<Json<MessageResponse>, (StatusCode, Json<ErrorResponse>)> {
    let mut reg = Registry::load().map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() }))
    })?;
    let removed = reg.unregister(&payload.name).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() }))
    })?;
    if removed {
        Ok(Json(MessageResponse { message: format!("Server '{}' rimosso dal registro.", payload.name) }))
    } else {
        Err((StatusCode::NOT_FOUND, Json(ErrorResponse { error: format!("Server '{}' non trovato.", payload.name) })))
    }
}

// ─── Config / Status / Start / Stop ──────────────────────────────────────────

async fn get_config(
    State(state): State<AppState>,
    Query(params): Query<ServerQuery>,
) -> Result<Json<ServerConfig>, (StatusCode, Json<ErrorResponse>)> {
    let dir = resolve_server_dir(&state, params.server.as_deref()).await?;
    match ServerConfig::load_from_dir(&dir).await {
        Ok(cfg) => Ok(Json(cfg)),
        Err(e) => Err((StatusCode::NOT_FOUND, Json(ErrorResponse { error: e.to_string() }))),
    }
}

async fn get_status(
    State(state): State<AppState>,
    Query(params): Query<ServerQuery>,
) -> Json<StatusResponse> {
    let manager = resolve_manager(&state, params.server.as_deref()).await;
    Json(StatusResponse { running: manager.is_running().await })
}

async fn start_server(
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<StatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    let server_name = payload.get("server").and_then(|v| v.as_str()).map(|s| s.to_string());
    let dir = resolve_server_dir(&state, server_name.as_deref()).await?;
    let manager = resolve_manager(&state, server_name.as_deref()).await;

    let config = ServerConfig::load_from_dir(&dir).await.map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() }))
    })?;

    manager.start(&dir, &config).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() }))
    })?;

    Ok(Json(StatusResponse { running: true }))
}

async fn stop_server(
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<StatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    let server_name = payload.get("server").and_then(|v| v.as_str()).map(|s| s.to_string());
    let manager = resolve_manager(&state, server_name.as_deref()).await;
    manager.stop().await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() }))
    })?;
    Ok(Json(StatusResponse { running: false }))
}

async fn send_command(
    State(state): State<AppState>,
    Json(payload): Json<CommandPayload>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let manager = resolve_manager(&state, payload.server.as_deref()).await;
    manager.send_command(&payload.command).await.map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() }))
    })?;
    Ok(StatusCode::OK)
}

// ─── Mods ─────────────────────────────────────────────────────────────────────

async fn search_mods(
    State(state): State<AppState>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Vec<crate::downloader::modrinth::ModrinthSearchHit>>, (StatusCode, Json<ErrorResponse>)> {
    let dir = resolve_server_dir(&state, params.server.as_deref()).await?;
    let config = ServerConfig::load_from_dir(&dir).await.ok();
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
    let dir = resolve_server_dir(&state, payload.server.as_deref()).await?;
    let config = ServerConfig::load_from_dir(&dir).await.map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() }))
    })?;

    let loader_str = config.loader.to_string().to_lowercase();
    let mods_dir = dir.join("mods");
    let modrinth = ModrinthClient::new();
    let mut installed = HashSet::new();

    match modrinth.install_mod_with_dependencies(&payload.slug, &config.mc_version, &loader_str, &mods_dir, &mut installed).await {
        Ok(files) => Ok(Json(files)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() }))),
    }
}

async fn list_mods(
    State(state): State<AppState>,
    Query(params): Query<ServerQuery>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<ErrorResponse>)> {
    let dir = resolve_server_dir(&state, params.server.as_deref()).await?;
    let mods_dir = dir.join("mods");
    match ModrinthClient::list_installed_mods(&mods_dir).await {
        Ok(list) => Ok(Json(list)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() }))),
    }
}

async fn remove_mod(
    State(state): State<AppState>,
    Json(payload): Json<ModRemovePayload>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let dir = resolve_server_dir(&state, payload.server.as_deref()).await?;
    let mods_dir = dir.join("mods");
    let target = mods_dir.join(&payload.filename);
    if target.exists() {
        let _ = tokio::fs::remove_file(target).await;
        Ok(StatusCode::OK)
    } else {
        Err((StatusCode::NOT_FOUND, Json(ErrorResponse { error: "File non trovato".to_string() })))
    }
}

// ─── Diagnostics / Upgrade / Backup / RAM ────────────────────────────────────

async fn get_diagnostics(
    State(state): State<AppState>,
    Query(params): Query<ServerQuery>,
) -> Result<Json<ServerCheckReport>, (StatusCode, Json<ErrorResponse>)> {
    let dir = resolve_server_dir(&state, params.server.as_deref()).await?;
    let config_port = ServerConfig::load_from_dir(&dir)
        .await
        .map(|c| c.port)
        .unwrap_or(25565);
    let report = generate_diagnostics_report(config_port).await;
    Ok(Json(report))
}

async fn upgrade_server(
    State(state): State<AppState>,
    Json(payload): Json<UpgradeRequest>,
) -> Result<Json<MessageResponse>, (StatusCode, Json<ErrorResponse>)> {
    let dir = resolve_server_dir(&state, payload.server.as_deref()).await?;
    let manager = resolve_manager(&state, payload.server.as_deref()).await;

    if manager.is_running().await {
        return Err((StatusCode::BAD_REQUEST, Json(ErrorResponse {
            error: "Arresta il server prima di effettuare l'aggiornamento!".to_string(),
        })));
    }

    let loader_enum = if let Some(ref l) = payload.loader {
        match l.parse::<crate::core::config::LoaderType>() {
            Ok(parsed) => Some(parsed),
            Err(e) => return Err((StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() }))),
        }
    } else {
        None
    };

    match crate::cli::commands::upgrade_server_core(&dir, &payload.version, loader_enum, payload.backup).await {
        Ok(msg) => Ok(Json(MessageResponse { message: msg })),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() }))),
    }
}

async fn trigger_backup(
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<MessageResponse>, (StatusCode, Json<ErrorResponse>)> {
    let server_name = payload.get("server").and_then(|v| v.as_str()).map(|s| s.to_string());
    let dir = resolve_server_dir(&state, server_name.as_deref()).await?;
    match crate::core::backup::create_world_backup(&dir) {
        Ok(p) => Ok(Json(MessageResponse { message: format!("Backup creato: {}", p.display()) })),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() }))),
    }
}

async fn update_ram(
    State(state): State<AppState>,
    Json(payload): Json<RamRequest>,
) -> Result<Json<MessageResponse>, (StatusCode, Json<ErrorResponse>)> {
    let dir = resolve_server_dir(&state, payload.server.as_deref()).await?;
    let mut config = ServerConfig::load_from_dir(&dir).await.map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() }))
    })?;

    config.ram_max_mb = payload.ram_mb;
    config.ram_min_mb = (payload.ram_mb / 2).max(1024);
    config.save_to_dir(&dir).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() }))
    })?;

    Ok(Json(MessageResponse {
        message: format!("RAM aggiornata a {} MB (attiva al prossimo avvio)", payload.ram_mb),
    }))
}
