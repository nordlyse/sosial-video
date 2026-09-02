use std::time::{Duration, SystemTime};

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::Redirect,
    Json,
};
use lettre::{
    message::Mailbox, transport::smtp::authentication::Credentials, AsyncSmtpTransport,
    AsyncTransport, Message, Tokio1Executor,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{error, internal, AppState, ErrorBody};

pub const CONFIRM_TTL: Duration = Duration::from_secs(24 * 60 * 60);

const USERNAME_MIN: usize = 3;
const USERNAME_MAX: usize = 32;
const PASSWORD_MIN: usize = 8;
const PASSWORD_MAX: usize = 128;

#[derive(Deserialize)]
pub struct RegisterRequest {
    username: String,
    email: String,
    password: String,
    locale: Option<String>,
}

#[derive(Serialize)]
pub struct RegisterResponse {
    message: String,
}

#[derive(Deserialize)]
pub struct VerifyRequest {
    token: Option<Uuid>,
    code: Option<String>,
}

#[derive(Serialize)]
pub struct VerifyResponse {
    message: String,
}

pub async fn ensure_schema(pool: &deadpool_postgres::Pool) -> anyhow::Result<()> {
    let client = pool.get().await?;
    client
        .batch_execute(
            "
            ALTER TABLE users ADD COLUMN IF NOT EXISTS email TEXT;
            ALTER TABLE users ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'active';
            ALTER TABLE users ADD COLUMN IF NOT EXISTS locale TEXT NOT NULL DEFAULT 'en';
            UPDATE users SET status = 'active' WHERE status IS NULL OR status = '';
            CREATE UNIQUE INDEX IF NOT EXISTS users_email_unique_idx
                ON users (lower(email))
                WHERE email IS NOT NULL AND email <> '';
            CREATE TABLE IF NOT EXISTS email_verifications (
                token      UUID PRIMARY KEY,
                code       TEXT UNIQUE NOT NULL,
                user_id    INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
                expires_at TIMESTAMPTZ NOT NULL,
                used_at    TIMESTAMPTZ
            );
            CREATE INDEX IF NOT EXISTS email_verifications_user_idx
                ON email_verifications (user_id);
            ",
        )
        .await?;
    Ok(())
}

pub async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<RegisterResponse>), (StatusCode, Json<ErrorBody>)> {
    let username = normalize_username(&body.username)?;
    let email = normalize_email(&body.email)?;
    let password = body.password;
    if password.len() < PASSWORD_MIN || password.len() > PASSWORD_MAX {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "password must be between 8 and 128 characters",
        ));
    }

    let hash = bcrypt::hash(&password, bcrypt::DEFAULT_COST).map_err(internal)?;
    let client = state.pool.get().await.map_err(internal)?;

    let by_name = client
        .query_opt(
            "SELECT id, status FROM users WHERE lower(username) = lower($1)",
            &[&username],
        )
        .await
        .map_err(internal)?;
    let by_email = client
        .query_opt(
            "SELECT id, status FROM users WHERE email IS NOT NULL AND lower(email) = $1",
            &[&email],
        )
        .await
        .map_err(internal)?;

    let user_id = match (by_name, by_email) {
        (Some(name_row), Some(email_row)) => {
            let name_id: i32 = name_row.get(0);
            let email_id: i32 = email_row.get(0);
            if name_id != email_id {
                return Err(error(
                    StatusCode::CONFLICT,
                    "username or email already in use",
                ));
            }
            reuse_pending_user(&client, name_id, name_row.get(1), &username, &email, &hash).await?
        }
        (Some(row), None) | (None, Some(row)) => {
            reuse_pending_user(&client, row.get(0), row.get(1), &username, &email, &hash).await?
        }
        (None, None) => {
            let row = client
                .query_one(
                    "INSERT INTO users (username, password_hash, email, status)
                     VALUES ($1, $2, $3, 'pending')
                     RETURNING id",
                    &[&username, &hash, &email],
                )
                .await
                .map_err(internal)?;
            row.get(0)
        }
    };

    let locale = crate::normalize_locale(body.locale.as_deref(), &headers);
    client
        .execute(
            "UPDATE users SET locale = $2 WHERE id = $1",
            &[&user_id, &locale],
        )
        .await
        .map_err(internal)?;

    client
        .execute(
            "DELETE FROM email_verifications WHERE user_id = $1 AND used_at IS NULL",
            &[&user_id],
        )
        .await
        .map_err(internal)?;

    let token = Uuid::new_v4();
    let code = confirmation_code();
    let expires = SystemTime::now() + CONFIRM_TTL;
    client
        .execute(
            "INSERT INTO email_verifications (token, code, user_id, expires_at)
             VALUES ($1, $2, $3, $4)",
            &[&token, &code, &user_id, &expires],
        )
        .await
        .map_err(internal)?;

    let app_url = public_app_url(&headers, &state.public_app_url);
    let confirm_url = format!("{app_url}/?verify={token}");
    tracing::info!("confirmation link for {username} ({email}): {confirm_url}");

    if let Err(err) = send_confirmation_email(&state, &email, &username, &confirm_url, &code).await {
        tracing::error!("could not send confirmation email to {email}: {err}");
    }

    Ok((
        StatusCode::CREATED,
        Json(RegisterResponse {
            message: "Check your email and click the confirmation link within 1 day to activate your account.".into(),
        }),
    ))
}

pub async fn verify_submit(
    State(state): State<AppState>,
    Json(body): Json<VerifyRequest>,
) -> Result<Json<VerifyResponse>, (StatusCode, Json<ErrorBody>)> {
    let code = body
        .code
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_uppercase());
    activate_account(&state, body.token, code.as_deref()).await?;
    Ok(Json(VerifyResponse {
        message: "Account confirmed. You can sign in.".into(),
    }))
}

pub async fn verify_click(
    State(state): State<AppState>,
    Path(token): Path<Uuid>,
) -> Redirect {
    let dest = match activate_account(&state, Some(token), None).await {
        Ok(()) => format!("{}/?verified=1", state.public_app_url.trim_end_matches('/')),
        Err((_, Json(body))) if body.error.contains("expired") => {
            format!(
                "{}/?verify_error=expired",
                state.public_app_url.trim_end_matches('/')
            )
        }
        Err(_) => format!(
            "{}/?verify_error=invalid",
            state.public_app_url.trim_end_matches('/')
        ),
    };
    Redirect::to(&dest)
}

pub async fn activate_account(
    state: &AppState,
    token: Option<Uuid>,
    code: Option<&str>,
) -> Result<(), (StatusCode, Json<ErrorBody>)> {
    if token.is_none() && code.is_none() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "confirmation token or code required",
        ));
    }

    let client = state.pool.get().await.map_err(internal)?;
    let row = if let Some(token) = token {
        client
            .query_opt(
                "SELECT v.token, v.user_id, v.expires_at, v.used_at, u.status
                 FROM email_verifications v
                 JOIN users u ON u.id = v.user_id
                 WHERE v.token = $1",
                &[&token],
            )
            .await
            .map_err(internal)?
    } else {
        let code = code.unwrap();
        client
            .query_opt(
                "SELECT v.token, v.user_id, v.expires_at, v.used_at, u.status
                 FROM email_verifications v
                 JOIN users u ON u.id = v.user_id
                 WHERE v.code = $1",
                &[&code],
            )
            .await
            .map_err(internal)?
    };

    let Some(row) = row else {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "invalid confirmation link or code",
        ));
    };

    let token: Uuid = row.get(0);
    let user_id: i32 = row.get(1);
    let expires_at: SystemTime = row.get(2);
    let used_at: Option<SystemTime> = row.get(3);
    let status: String = row.get(4);

    if status == "active" {
        return Ok(());
    }
    if used_at.is_some() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "this confirmation link was already used",
        ));
    }
    if expires_at < SystemTime::now() {
        let _ = client
            .execute(
                "UPDATE users SET status = 'expired' WHERE id = $1 AND status = 'pending'",
                &[&user_id],
            )
            .await;
        return Err(error(
            StatusCode::GONE,
            "this sign-up expired. Register a new account.",
        ));
    }

    client
        .execute(
            "UPDATE users SET status = 'active' WHERE id = $1",
            &[&user_id],
        )
        .await
        .map_err(internal)?;
    client
        .execute(
            "UPDATE email_verifications SET used_at = now() WHERE token = $1",
            &[&token],
        )
        .await
        .map_err(internal)?;
    Ok(())
}

async fn reuse_pending_user(
    client: &deadpool_postgres::Object,
    user_id: i32,
    status: String,
    username: &str,
    email: &str,
    hash: &str,
) -> Result<i32, (StatusCode, Json<ErrorBody>)> {
    if status == "active" {
        return Err(error(
            StatusCode::CONFLICT,
            "username or email already in use",
        ));
    }
    client
        .execute(
            "UPDATE users
             SET username = $2, email = $3, password_hash = $4, status = 'pending'
             WHERE id = $1",
            &[&user_id, &username, &email, &hash],
        )
        .await
        .map_err(internal)?;
    Ok(user_id)
}

async fn send_confirmation_email(
    state: &AppState,
    to_email: &str,
    username: &str,
    confirm_url: &str,
    code: &str,
) -> anyhow::Result<()> {
    let Some(host) = state.smtp.host.as_deref() else {
        tracing::warn!("SMTP_HOST is not set; confirmation email was not sent");
        return Ok(());
    };

    let from: Mailbox = state.smtp.from.parse()?;
    let to: Mailbox = to_email.parse()?;
    let body = format!(
        "Hello {username},\n\n\
         Confirm your Sosial Video account within 1 day by opening this link:\n\n\
         {confirm_url}\n\n\
         Confirmation code: {code}\n\n\
         If you did not create this account, you can ignore this email.\n\
         After 1 day the request expires and you will not be able to sign in.\n"
    );
    let message = Message::builder()
        .from(from)
        .to(to)
        .subject("Confirm your Sosial Video account")
        .body(body)?;

    let mut builder = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
        .port(state.smtp.port);
    if let (Some(user), Some(password)) = (&state.smtp.username, &state.smtp.password) {
        builder = builder.credentials(Credentials::new(user.clone(), password.clone()));
    }
    let mailer = builder.build();
    mailer.send(message).await?;
    tracing::info!("sent confirmation email to {to_email}");
    Ok(())
}

fn public_app_url(headers: &HeaderMap, fallback: &str) -> String {
    headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.trim_end_matches('/').to_string())
        .unwrap_or_else(|| fallback.trim_end_matches('/').to_string())
}

fn normalize_username(raw: &str) -> Result<String, (StatusCode, Json<ErrorBody>)> {
    let username = raw.trim().to_string();
    if username.len() < USERNAME_MIN || username.len() > USERNAME_MAX {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "username must be between 3 and 32 characters",
        ));
    }
    let valid = username
        .chars()
        .enumerate()
        .all(|(i, ch)| match ch {
            'A'..='Z' | 'a'..='z' => true,
            '0'..='9' | '_' | '-' if i > 0 => true,
            _ => false,
        });
    if !valid {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "username must start with a letter and may contain letters, numbers, _ and -",
        ));
    }
    Ok(username)
}

fn normalize_email(raw: &str) -> Result<String, (StatusCode, Json<ErrorBody>)> {
    let email = raw.trim().to_lowercase();
    let Some((local, domain)) = email.split_once('@') else {
        return Err(error(StatusCode::BAD_REQUEST, "a valid email is required"));
    };
    if local.is_empty() || !domain.contains('.') || domain.starts_with('.') || domain.ends_with('.')
    {
        return Err(error(StatusCode::BAD_REQUEST, "a valid email is required"));
    }
    if email.len() > 254 || email.chars().any(|ch| ch.is_whitespace()) {
        return Err(error(StatusCode::BAD_REQUEST, "a valid email is required"));
    }
    Ok(email)
}

fn confirmation_code() -> String {
    const CHARS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    Uuid::new_v4()
        .as_bytes()
        .iter()
        .take(8)
        .map(|byte| CHARS[(*byte as usize) % CHARS.len()] as char)
        .collect()
}
