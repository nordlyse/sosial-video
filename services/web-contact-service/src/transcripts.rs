use std::path::{Path, PathBuf};

use axum::{
    extract::{Path as AxumPath, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use crate::{current_user, error, internal, AppState, UserView};

const MAX_TRANSCRIPT_LEN: usize = 2000;

#[derive(Deserialize)]
pub struct TranscriptRequest {
    body: String,
}

#[derive(Serialize)]
pub struct TranscriptView {
    id: i32,
    body: String,
    from: UserView,
    created_at: String,
}

pub async fn ensure_schema(pool: &deadpool_postgres::Pool) -> anyhow::Result<()> {
    let client = pool.get().await?;
    client
        .batch_execute(
            "
            ALTER TABLE broadcasts ADD COLUMN IF NOT EXISTS transcript_log_path TEXT;
            CREATE TABLE IF NOT EXISTS broadcast_transcripts (
                id           SERIAL PRIMARY KEY,
                broadcast_id INTEGER NOT NULL REFERENCES broadcasts (id) ON DELETE CASCADE,
                user_id      INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
                body         TEXT NOT NULL,
                created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
            );
            CREATE INDEX IF NOT EXISTS broadcast_transcripts_broadcast_idx
                ON broadcast_transcripts (broadcast_id, created_at);
            ",
        )
        .await?;
    Ok(())
}

pub async fn add_transcript(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(broadcast_id): AxumPath<i32>,
    Json(body): Json<TranscriptRequest>,
) -> Result<Json<TranscriptView>, (StatusCode, Json<crate::ErrorBody>)> {
    let text = normalize_transcript(&body.body);
    if text.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "transcript is empty"));
    }
    let (user, _) = current_user(&state, &headers).await?;
    let client = state.pool.get().await.map_err(internal)?;
    let member = client
        .query_opt(
            "SELECT 1 FROM broadcast_members m
             JOIN broadcasts b ON b.id = m.broadcast_id
             WHERE m.broadcast_id = $1 AND m.user_id = $2 AND b.ended_at IS NULL",
            &[&broadcast_id, &user.id],
        )
        .await
        .map_err(internal)?;
    if member.is_none() {
        return Err(error(StatusCode::FORBIDDEN, "not in this broadcast"));
    }
    let recent: i64 = client
        .query_one(
            "SELECT count(*)::bigint FROM broadcast_transcripts
             WHERE broadcast_id = $1 AND user_id = $2
               AND created_at > now() - interval '10 seconds'",
            &[&broadcast_id, &user.id],
        )
        .await
        .map_err(internal)?
        .get(0);
    if recent >= 20 {
        return Err(error(StatusCode::TOO_MANY_REQUESTS, "slow down a little"));
    }
    let row = client
        .query_one(
            "INSERT INTO broadcast_transcripts (broadcast_id, user_id, body)
             VALUES ($1, $2, $3)
             RETURNING id, created_at",
            &[&broadcast_id, &user.id, &text],
        )
        .await
        .map_err(internal)?;
    let created_at: std::time::SystemTime = row.get(1);
    let stamp = crate::rfc3339(created_at);
    append_utterance(&state, &client, broadcast_id, &user.username, &text, &stamp)
        .await
        .map_err(internal)?;
    Ok(Json(TranscriptView {
        id: row.get(0),
        body: text,
        from: user,
        created_at: stamp,
    }))
}

pub async fn create_broadcast_log(
    state: &AppState,
    client: &deadpool_postgres::Object,
    broadcast_id: i32,
    host: &str,
    title: &str,
) -> anyhow::Result<String> {
    let date = Utc::now().format("%Y-%m-%d").to_string();
    let rel = format!("{date}/broadcast-{broadcast_id}.txt");
    let abs = abs_log_path(&state.transcript_log_dir, &rel);
    if let Some(parent) = abs.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let started = Utc::now().to_rfc3339();
    let title_line = if title.trim().is_empty() {
        "(untitled)".to_string()
    } else {
        title.replace('\n', " ")
    };
    let header = format!(
        "# Sosial Video speech log\n# broadcast_id: {broadcast_id}\n# started_at: {started}\n# host: {host}\n# title: {title_line}\n#\n"
    );
    let _guard = state.transcript_lock.lock().await;
    tokio::fs::write(&abs, header).await?;
    drop(_guard);
    client
        .execute(
            "UPDATE broadcasts SET transcript_log_path = $2 WHERE id = $1",
            &[&broadcast_id, &rel],
        )
        .await?;
    tracing::info!("speech log {rel} opened for broadcast {broadcast_id}");
    Ok(rel)
}

pub async fn mark_broadcast_ended(
    state: &AppState,
    client: &deadpool_postgres::Object,
    broadcast_id: i32,
) -> anyhow::Result<()> {
    let rel = match load_log_path(client, broadcast_id).await? {
        Some(path) => path,
        None => return Ok(()),
    };
    let abs = abs_log_path(&state.transcript_log_dir, &rel);
    let line = format!("# ended_at: {}\n", Utc::now().to_rfc3339());
    append_file(&state, &abs, &line).await
}

async fn append_utterance(
    state: &AppState,
    client: &deadpool_postgres::Object,
    broadcast_id: i32,
    username: &str,
    body: &str,
    stamp: &str,
) -> anyhow::Result<()> {
    let rel = match load_log_path(client, broadcast_id).await? {
        Some(path) => path,
        None => {
            create_broadcast_log(state, client, broadcast_id, username, "").await?
        }
    };
    let abs = abs_log_path(&state.transcript_log_dir, &rel);
    if let Some(parent) = abs.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let line = format!("[{stamp}] {username}: {body}\n");
    append_file(state, &abs, &line).await
}

async fn load_log_path(
    client: &deadpool_postgres::Object,
    broadcast_id: i32,
) -> anyhow::Result<Option<String>> {
    let row = client
        .query_opt(
            "SELECT transcript_log_path FROM broadcasts WHERE id = $1",
            &[&broadcast_id],
        )
        .await?;
    Ok(row.and_then(|row| row.get::<_, Option<String>>(0)))
}

async fn append_file(state: &AppState, path: &Path, line: &str) -> anyhow::Result<()> {
    let _guard = state.transcript_lock.lock().await;
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    file.write_all(line.as_bytes()).await?;
    Ok(())
}

fn abs_log_path(root: &Path, rel: &str) -> PathBuf {
    root.join(rel.replace('\\', "/"))
}

fn normalize_transcript(raw: &str) -> String {
    let collapsed: String = raw
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    collapsed.chars().take(MAX_TRANSCRIPT_LEN).collect()
}
