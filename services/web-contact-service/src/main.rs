mod broadcasts;
mod comments;

use std::{
    net::SocketAddr,
    time::{Duration, SystemTime},
};

use axum::{
    extract::{ConnectInfo, State},
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    routing::{get, post, put},
    Json, Router,
};
use chrono::{DateTime, Utc};
use deadpool_postgres::{Config, Pool, Runtime};
use serde::{Deserialize, Serialize};
use tokio_postgres::NoTls;
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

const TEST_USERS: [(&str, &str); 5] = [
    ("alice", "alice123"),
    ("bob", "bob123"),
    ("carol", "carol123"),
    ("dave", "dave123"),
    ("eve", "eve123"),
];

const SESSION_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const ONLINE_WINDOW: Duration = Duration::from_secs(90);

#[derive(Clone)]
pub struct AppState {
    pub pool: Pool,
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Clone, Serialize)]
pub struct UserView {
    pub id: i32,
    pub username: String,
}

#[derive(Serialize)]
struct LoginResponse {
    token: Uuid,
    user: UserView,
}

#[derive(Deserialize)]
struct PresenceRequest {
    ip_address: Option<String>,
    port: Option<i32>,
}

#[derive(Serialize)]
struct ContactView {
    id: i32,
    username: String,
    ip_address: Option<String>,
    port: Option<i32>,
    last_seen: Option<String>,
    online: bool,
}

#[derive(Serialize)]
pub struct ErrorBody {
    error: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let pool = make_pool()?;
    wait_for_db(&pool).await?;
    seed_test_users(&pool).await?;
    broadcasts::ensure_schema(&pool).await?;

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/login", post(login))
        .route("/v1/logout", post(logout))
        .route("/v1/me", get(me))
        .route("/v1/presence", put(presence))
        .route("/v1/contacts", get(contacts))
        .route("/v1/studio", get(broadcasts::studio))
        .route("/v1/broadcasts", post(broadcasts::start_broadcast))
        .route("/v1/broadcasts/current/end", post(broadcasts::end_broadcast))
        .route("/v1/broadcasts/current/leave", post(broadcasts::leave_broadcast))
        .route("/v1/broadcasts/{id}/requests", post(broadcasts::request_join))
        .route(
            "/v1/broadcasts/{id}/requests/{req_id}/accept",
            post(broadcasts::accept_join),
        )
        .route(
            "/v1/broadcasts/{id}/requests/{req_id}/reject",
            post(broadcasts::reject_join),
        )
        .route("/v1/broadcasts/{id}/speaking", put(broadcasts::set_speaking))
        .route("/v1/broadcasts/{id}/reactions", post(broadcasts::add_reaction))
        .route("/v1/users/{id}/comments", get(comments::list_comments).post(comments::add_comment))
        .with_state(AppState { pool })
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    let listen: SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8081".into())
        .parse()?;
    tracing::info!("web-contact-service listening on {listen}");
    let listener = tokio::net::TcpListener::bind(listen).await?;
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await?;
    Ok(())
}

fn make_pool() -> anyhow::Result<Pool> {
    let mut cfg = Config::new();
    cfg.host = Some(std::env::var("PG_HOST").unwrap_or_else(|_| "127.0.0.1".into()));
    cfg.port = Some(
        std::env::var("PG_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(5432),
    );
    cfg.user = Some(std::env::var("PG_USER").unwrap_or_else(|_| "sosial".into()));
    cfg.password = Some(std::env::var("PG_PASSWORD").unwrap_or_else(|_| "sosial".into()));
    cfg.dbname = Some(std::env::var("PG_DB").unwrap_or_else(|_| "sosial_video".into()));
    Ok(cfg.create_pool(Some(Runtime::Tokio1), NoTls)?)
}

async fn wait_for_db(pool: &Pool) -> anyhow::Result<()> {
    let mut last_err = None;
    for attempt in 1..=30 {
        match pool.get().await {
            Ok(client) => {
                client.simple_query("SELECT 1").await?;
                tracing::info!("connected to frontend-db");
                return Ok(());
            }
            Err(err) => {
                tracing::warn!("frontend-db not ready (attempt {attempt}/30): {err}");
                last_err = Some(err);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
    anyhow::bail!("could not connect to frontend-db: {last_err:?}")
}

async fn seed_test_users(pool: &Pool) -> anyhow::Result<()> {
    let client = pool.get().await?;
    for (username, password) in TEST_USERS {
        let exists: bool = client
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM users WHERE username = $1)",
                &[&username],
            )
            .await?
            .get(0);
        if exists {
            continue;
        }
        let hash = bcrypt::hash(password, bcrypt::DEFAULT_COST)?;
        client
            .execute(
                "INSERT INTO users (username, password_hash) VALUES ($1, $2)",
                &[&username, &hash],
            )
            .await?;
        tracing::info!("seeded test user {username}");
    }
    Ok(())
}

async fn health(State(state): State<AppState>) -> Result<&'static str, StatusCode> {
    let client = state.pool.get().await.map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    client
        .simple_query("SELECT 1")
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok("ok")
}

async fn login(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, Json<ErrorBody>)> {
    let username = body.username.trim().to_string();
    if username.is_empty() || body.password.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "username and password required"));
    }

    let client = state.pool.get().await.map_err(internal)?;
    let row = client
        .query_opt(
            "SELECT id, password_hash FROM users WHERE username = $1",
            &[&username],
        )
        .await
        .map_err(internal)?;

    let Some(row) = row else {
        return Err(error(StatusCode::UNAUTHORIZED, "invalid credentials"));
    };
    let user_id: i32 = row.get(0);
    let password_hash: String = row.get(1);
    let ok = bcrypt::verify(&body.password, &password_hash).map_err(internal)?;
    if !ok {
        return Err(error(StatusCode::UNAUTHORIZED, "invalid credentials"));
    }

    let token = Uuid::new_v4();
    let expires = SystemTime::now() + SESSION_TTL;
    client
        .execute(
            "INSERT INTO sessions (token, user_id, expires_at) VALUES ($1, $2, $3)",
            &[&token, &user_id, &expires],
        )
        .await
        .map_err(internal)?;

    let ip = client_ip(&headers, addr);
    upsert_contact(&client, user_id, Some(ip), None)
        .await
        .map_err(internal)?;

    Ok(Json(LoginResponse {
        token,
        user: UserView {
            id: user_id,
            username,
        },
    }))
}

async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, (StatusCode, Json<ErrorBody>)> {
    let (user, token) = current_user(&state, &headers).await?;
    let client = state.pool.get().await.map_err(internal)?;
    let _ = broadcasts::leave_live_membership(&client, user.id).await;
    client
        .execute("DELETE FROM sessions WHERE token = $1", &[&token])
        .await
        .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<UserView>, (StatusCode, Json<ErrorBody>)> {
    let (user, _) = current_user(&state, &headers).await?;
    Ok(Json(user))
}

async fn presence(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<PresenceRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorBody>)> {
    let (user, _) = current_user(&state, &headers).await?;
    let client = state.pool.get().await.map_err(internal)?;
    let ip = body
        .ip_address
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| client_ip(&headers, addr));
    upsert_contact(&client, user.id, Some(ip), body.port)
        .await
        .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn contacts(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ContactView>>, (StatusCode, Json<ErrorBody>)> {
    let _ = current_user(&state, &headers).await?;
    let client = state.pool.get().await.map_err(internal)?;
    let rows = client
        .query(
            "SELECT u.id, u.username, c.ip_address, c.port, c.last_seen
             FROM users u
             LEFT JOIN contacts c ON c.user_id = u.id
             ORDER BY u.username",
            &[],
        )
        .await
        .map_err(internal)?;

    let now = SystemTime::now();
    let list = rows
        .into_iter()
        .map(|row| {
            let last_seen: Option<SystemTime> = row.get(4);
            let online = last_seen
                .and_then(|ts| now.duration_since(ts).ok())
                .map(|age| age <= ONLINE_WINDOW)
                .unwrap_or(false);
            ContactView {
                id: row.get(0),
                username: row.get(1),
                ip_address: row.get(2),
                port: row.get(3),
                last_seen: last_seen.map(rfc3339),
                online,
            }
        })
        .collect();
    Ok(Json(list))
}

pub async fn current_user(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(UserView, Uuid), (StatusCode, Json<ErrorBody>)> {
    let token = bearer_token(headers)?;
    let client = state.pool.get().await.map_err(internal)?;
    let row = client
        .query_opt(
            "SELECT u.id, u.username, s.expires_at
             FROM sessions s
             JOIN users u ON u.id = s.user_id
             WHERE s.token = $1",
            &[&token],
        )
        .await
        .map_err(internal)?;
    let Some(row) = row else {
        return Err(error(StatusCode::UNAUTHORIZED, "invalid session"));
    };
    let expires: SystemTime = row.get(2);
    if expires < SystemTime::now() {
        let _ = client
            .execute("DELETE FROM sessions WHERE token = $1", &[&token])
            .await;
        return Err(error(StatusCode::UNAUTHORIZED, "session expired"));
    }
    Ok((
        UserView {
            id: row.get(0),
            username: row.get(1),
        },
        token,
    ))
}

async fn upsert_contact(
    client: &deadpool_postgres::Object,
    user_id: i32,
    ip: Option<String>,
    port: Option<i32>,
) -> anyhow::Result<()> {
    client
        .execute(
            "INSERT INTO contacts (user_id, ip_address, port, last_seen)
             VALUES ($1, $2, $3, now())
             ON CONFLICT (user_id) DO UPDATE
             SET ip_address = COALESCE(EXCLUDED.ip_address, contacts.ip_address),
                 port = COALESCE(EXCLUDED.port, contacts.port),
                 last_seen = now()",
            &[&user_id, &ip, &port],
        )
        .await?;
    Ok(())
}

fn bearer_token(headers: &HeaderMap) -> Result<Uuid, (StatusCode, Json<ErrorBody>)> {
    let value = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "missing authorization"))?;
    let token = value
        .strip_prefix("Bearer ")
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "invalid authorization"))?;
    Uuid::parse_str(token).map_err(|_| error(StatusCode::UNAUTHORIZED, "invalid token"))
}

fn client_ip(headers: &HeaderMap, addr: SocketAddr) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| addr.ip().to_string())
}

pub fn rfc3339(ts: SystemTime) -> String {
    DateTime::<Utc>::from(ts).to_rfc3339()
}

pub fn error(status: StatusCode, message: &str) -> (StatusCode, Json<ErrorBody>) {
    (
        status,
        Json(ErrorBody {
            error: message.to_string(),
        }),
    )
}

pub fn internal<E: std::fmt::Display>(err: E) -> (StatusCode, Json<ErrorBody>) {
    tracing::error!("{err}");
    error(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
}
