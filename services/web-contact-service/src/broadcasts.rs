use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{current_user, error, internal, AppState, UserView};

#[derive(Clone, Serialize)]
pub struct BroadcastView {
    id: i32,
    room_id: String,
    host: UserView,
    member_count: i64,
    title: String,
    tags: Vec<String>,
    is_public: bool,
}

#[derive(Serialize)]
pub struct MembershipView {
    broadcast_id: i32,
    room_id: String,
    role: String,
    host: UserView,
    title: String,
}

#[derive(Serialize)]
pub struct JoinRequestView {
    id: i32,
    broadcast_id: i32,
    from: UserView,
    status: String,
    granted_role: Option<String>,
}

#[derive(Serialize)]
pub struct OutgoingRequestView {
    id: i32,
    broadcast_id: i32,
    host: UserView,
    status: String,
    granted_role: Option<String>,
}

#[derive(Serialize)]
pub struct ParticipantView {
    user: UserView,
    role: String,
    speaking: bool,
}

#[derive(Serialize)]
pub struct ReactionView {
    pub id: i32,
    pub emoji: String,
    pub from: UserView,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct StudioView {
    broadcasts: Vec<BroadcastView>,
    public_broadcasts: Vec<BroadcastView>,
    membership: Option<MembershipView>,
    incoming_requests: Vec<JoinRequestView>,
    outgoing_requests: Vec<OutgoingRequestView>,
    participants: Vec<ParticipantView>,
    recent_reactions: Vec<ReactionView>,
}

#[derive(Deserialize)]
pub struct ReactionRequest {
    emoji: String,
}

const ALLOWED_REACTIONS: &[&str] = &["❤️", "❤", "👍", "👎", "😂", "🔥", "👏", "😮", "😢", "🎉"];

#[derive(Deserialize)]
pub struct StartBroadcastRequest {
    #[serde(default)]
    title: String,
    #[serde(default = "default_public")]
    is_public: bool,
}

fn default_public() -> bool {
    true
}

#[derive(Deserialize)]
pub struct PublicQuery {
    #[serde(default)]
    q: String,
}

#[derive(Deserialize)]
pub struct AcceptRequest {
    role: String,
}

#[derive(Deserialize)]
pub struct SpeakingRequest {
    speaking: bool,
}

pub async fn ensure_schema(pool: &deadpool_postgres::Pool) -> anyhow::Result<()> {
    let client = pool.get().await?;
    client
        .batch_execute(
            "
            CREATE TABLE IF NOT EXISTS broadcasts (
                id           SERIAL PRIMARY KEY,
                host_user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
                room_id      TEXT UNIQUE NOT NULL,
                started_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
                ended_at     TIMESTAMPTZ
            );
            CREATE INDEX IF NOT EXISTS broadcasts_live_idx ON broadcasts (ended_at);
            CREATE TABLE IF NOT EXISTS broadcast_members (
                broadcast_id INTEGER NOT NULL REFERENCES broadcasts (id) ON DELETE CASCADE,
                user_id      INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
                role         TEXT NOT NULL,
                joined_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (broadcast_id, user_id)
            );
            CREATE TABLE IF NOT EXISTS join_requests (
                id           SERIAL PRIMARY KEY,
                broadcast_id INTEGER NOT NULL REFERENCES broadcasts (id) ON DELETE CASCADE,
                from_user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
                status       TEXT NOT NULL DEFAULT 'pending',
                granted_role TEXT,
                created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
                UNIQUE (broadcast_id, from_user_id)
            );
            CREATE TABLE IF NOT EXISTS broadcast_speaking (
                broadcast_id INTEGER NOT NULL REFERENCES broadcasts (id) ON DELETE CASCADE,
                user_id      INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
                speaking     BOOLEAN NOT NULL DEFAULT false,
                updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (broadcast_id, user_id)
            );
            CREATE TABLE IF NOT EXISTS camera_comments (
                id               SERIAL PRIMARY KEY,
                target_user_id   INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
                from_user_id     INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
                body             TEXT NOT NULL,
                parent_id        INTEGER REFERENCES camera_comments (id) ON DELETE CASCADE,
                is_private       BOOLEAN NOT NULL DEFAULT false,
                reply_to_user_id INTEGER REFERENCES users (id) ON DELETE SET NULL,
                created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
            );
            CREATE INDEX IF NOT EXISTS camera_comments_target_idx
                ON camera_comments (target_user_id, created_at DESC);
            ALTER TABLE camera_comments ADD COLUMN IF NOT EXISTS parent_id INTEGER REFERENCES camera_comments (id) ON DELETE CASCADE;
            ALTER TABLE camera_comments ADD COLUMN IF NOT EXISTS is_private BOOLEAN NOT NULL DEFAULT false;
            ALTER TABLE camera_comments ADD COLUMN IF NOT EXISTS reply_to_user_id INTEGER REFERENCES users (id) ON DELETE SET NULL;
            CREATE TABLE IF NOT EXISTS broadcast_reactions (
                id           SERIAL PRIMARY KEY,
                broadcast_id INTEGER NOT NULL REFERENCES broadcasts (id) ON DELETE CASCADE,
                from_user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
                emoji        TEXT NOT NULL,
                created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
            );
            CREATE INDEX IF NOT EXISTS broadcast_reactions_live_idx
                ON broadcast_reactions (broadcast_id, created_at DESC);
            ALTER TABLE broadcasts ADD COLUMN IF NOT EXISTS title TEXT NOT NULL DEFAULT '';
            ALTER TABLE broadcasts ADD COLUMN IF NOT EXISTS tags TEXT[] NOT NULL DEFAULT '{}';
            ALTER TABLE broadcasts ADD COLUMN IF NOT EXISTS is_public BOOLEAN NOT NULL DEFAULT true;
            ALTER TABLE users ADD COLUMN IF NOT EXISTS locale TEXT NOT NULL DEFAULT 'en';
            ",
        )
        .await?;
    Ok(())
}

pub async fn studio(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<StudioView>, (StatusCode, Json<crate::ErrorBody>)> {
    let (user, _) = current_user(&state, &headers).await?;
    let client = state.pool.get().await.map_err(internal)?;
    let locale = user_locale(&client, user.id).await?;

    let broadcasts = load_live_broadcasts(&client, 50).await?;
    let public_broadcasts = load_public_broadcasts(&client, "", &locale, 10).await?;

    let membership_row = client
        .query_opt(
            "SELECT b.id, b.room_id, m.role, h.id, h.username, COALESCE(b.title, '')
             FROM broadcast_members m
             JOIN broadcasts b ON b.id = m.broadcast_id
             JOIN users h ON h.id = b.host_user_id
             WHERE m.user_id = $1 AND b.ended_at IS NULL",
            &[&user.id],
        )
        .await
        .map_err(internal)?;
    let membership = membership_row.map(|row| MembershipView {
        broadcast_id: row.get(0),
        room_id: row.get(1),
        role: row.get(2),
        host: UserView {
            id: row.get(3),
            username: row.get(4),
        },
        title: row.get(5),
    });

    let incoming_rows = client
        .query(
            "SELECT r.id, r.broadcast_id, u.id, u.username, r.status, r.granted_role
             FROM join_requests r
             JOIN broadcasts b ON b.id = r.broadcast_id
             JOIN users u ON u.id = r.from_user_id
             WHERE b.host_user_id = $1 AND b.ended_at IS NULL AND r.status = 'pending'
             ORDER BY r.created_at",
            &[&user.id],
        )
        .await
        .map_err(internal)?;
    let incoming_requests = incoming_rows
        .into_iter()
        .map(|row| JoinRequestView {
            id: row.get(0),
            broadcast_id: row.get(1),
            from: UserView {
                id: row.get(2),
                username: row.get(3),
            },
            status: row.get(4),
            granted_role: row.get(5),
        })
        .collect();

    let outgoing_rows = client
        .query(
            "SELECT r.id, r.broadcast_id, h.id, h.username, r.status, r.granted_role
             FROM join_requests r
             JOIN broadcasts b ON b.id = r.broadcast_id
             JOIN users h ON h.id = b.host_user_id
             WHERE r.from_user_id = $1 AND b.ended_at IS NULL
             ORDER BY r.created_at DESC",
            &[&user.id],
        )
        .await
        .map_err(internal)?;
    let outgoing_requests = outgoing_rows
        .into_iter()
        .map(|row| OutgoingRequestView {
            id: row.get(0),
            broadcast_id: row.get(1),
            host: UserView {
                id: row.get(2),
                username: row.get(3),
            },
            status: row.get(4),
            granted_role: row.get(5),
        })
        .collect();

    let mut participants = Vec::new();
    let mut recent_reactions = Vec::new();
    if let Some(member) = &membership {
        let rows = client
            .query(
                "SELECT u.id, u.username, m.role, COALESCE(s.speaking, false)
                 FROM broadcast_members m
                 JOIN users u ON u.id = m.user_id
                 LEFT JOIN broadcast_speaking s
                   ON s.broadcast_id = m.broadcast_id AND s.user_id = m.user_id
                 WHERE m.broadcast_id = $1
                 ORDER BY m.joined_at",
                &[&member.broadcast_id],
            )
            .await
            .map_err(internal)?;
        participants = rows
            .into_iter()
            .map(|row| ParticipantView {
                user: UserView {
                    id: row.get(0),
                    username: row.get(1),
                },
                role: row.get(2),
                speaking: row.get(3),
            })
            .collect();
        recent_reactions = load_recent_reactions(&client, member.broadcast_id).await?;
    }

    Ok(Json(StudioView {
        broadcasts,
        public_broadcasts,
        membership,
        incoming_requests,
        outgoing_requests,
        participants,
        recent_reactions,
    }))
}

pub async fn public_broadcasts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PublicQuery>,
) -> Result<Json<Vec<BroadcastView>>, (StatusCode, Json<crate::ErrorBody>)> {
    let (user, _) = current_user(&state, &headers).await?;
    let client = state.pool.get().await.map_err(internal)?;
    let locale = user_locale(&client, user.id).await?;
    let list = load_public_broadcasts(&client, query.q.trim(), &locale, 40).await?;
    Ok(Json(list))
}

async fn user_locale(
    client: &deadpool_postgres::Object,
    user_id: i32,
) -> Result<String, (StatusCode, Json<crate::ErrorBody>)> {
    let row = client
        .query_one(
            "SELECT COALESCE(NULLIF(locale, ''), 'en') FROM users WHERE id = $1",
            &[&user_id],
        )
        .await
        .map_err(internal)?;
    Ok(primary_locale(row.get::<_, String>(0)))
}

async fn load_live_broadcasts(
    client: &deadpool_postgres::Object,
    limit: i64,
) -> Result<Vec<BroadcastView>, (StatusCode, Json<crate::ErrorBody>)> {
    map_broadcast_rows(
        client
            .query(
                "SELECT b.id, b.room_id, COALESCE(b.title, ''), COALESCE(b.tags, ARRAY[]::text[]),
                        COALESCE(b.is_public, true), u.id, u.username,
                        (SELECT count(*)::bigint FROM broadcast_members m WHERE m.broadcast_id = b.id)
                 FROM broadcasts b
                 JOIN users u ON u.id = b.host_user_id
                 WHERE b.ended_at IS NULL
                 ORDER BY b.started_at DESC
                 LIMIT $1",
                &[&limit],
            )
            .await
            .map_err(internal)?,
    )
}

async fn load_public_broadcasts(
    client: &deadpool_postgres::Object,
    raw_query: &str,
    locale: &str,
    limit: i64,
) -> Result<Vec<BroadcastView>, (StatusCode, Json<crate::ErrorBody>)> {
    let tag = normalize_search_tag(raw_query);
    let searching = !tag.is_empty();
    let like = format!("%{tag}%");
    let rows = client
        .query(
            "SELECT b.id, b.room_id, COALESCE(b.title, ''), COALESCE(b.tags, ARRAY[]::text[]),
                    COALESCE(b.is_public, true), u.id, u.username,
                    (SELECT count(*)::bigint FROM broadcast_members m WHERE m.broadcast_id = b.id)
             FROM broadcasts b
             JOIN users u ON u.id = b.host_user_id
             WHERE b.ended_at IS NULL
               AND COALESCE(b.is_public, true) = true
               AND (
                    $1 = false
                    OR b.title ILIKE $2
                    OR EXISTS (
                        SELECT 1 FROM unnest(COALESCE(b.tags, ARRAY[]::text[])) AS t(tag)
                        WHERE t.tag ILIKE $2
                    )
               )
               AND (
                    $1 = true
                    OR lower(split_part(COALESCE(u.locale, 'en'), '-', 1)) = $3
               )
             ORDER BY (SELECT count(*) FROM broadcast_members m WHERE m.broadcast_id = b.id) DESC,
                      b.started_at DESC
             LIMIT $4",
            &[&searching, &like, &locale, &limit],
        )
        .await
        .map_err(internal)?;
    let mut list = map_broadcast_rows(rows)?;
    if !searching && list.is_empty() {
        list = load_public_broadcasts_worldwide(client, limit).await?;
    }
    Ok(list)
}

async fn load_public_broadcasts_worldwide(
    client: &deadpool_postgres::Object,
    limit: i64,
) -> Result<Vec<BroadcastView>, (StatusCode, Json<crate::ErrorBody>)> {
    map_broadcast_rows(
        client
            .query(
                "SELECT b.id, b.room_id, COALESCE(b.title, ''), COALESCE(b.tags, ARRAY[]::text[]),
                        COALESCE(b.is_public, true), u.id, u.username,
                        (SELECT count(*)::bigint FROM broadcast_members m WHERE m.broadcast_id = b.id)
                 FROM broadcasts b
                 JOIN users u ON u.id = b.host_user_id
                 WHERE b.ended_at IS NULL AND COALESCE(b.is_public, true) = true
                 ORDER BY (SELECT count(*) FROM broadcast_members m WHERE m.broadcast_id = b.id) DESC,
                          b.started_at DESC
                 LIMIT $1",
                &[&limit],
            )
            .await
            .map_err(internal)?,
    )
}

fn map_broadcast_rows(
    rows: Vec<tokio_postgres::Row>,
) -> Result<Vec<BroadcastView>, (StatusCode, Json<crate::ErrorBody>)> {
    Ok(rows
        .into_iter()
        .map(|row| {
            let tags: Vec<String> = row.get(3);
            BroadcastView {
                id: row.get(0),
                room_id: row.get(1),
                title: row.get(2),
                tags,
                is_public: row.get(4),
                host: UserView {
                    id: row.get(5),
                    username: row.get(6),
                },
                member_count: row.get(7),
            }
        })
        .collect())
}

async fn load_recent_reactions(
    client: &deadpool_postgres::Object,
    broadcast_id: i32,
) -> Result<Vec<ReactionView>, (StatusCode, Json<crate::ErrorBody>)> {
    let rows = client
        .query(
            "SELECT r.id, r.emoji, u.id, u.username, r.created_at
             FROM broadcast_reactions r
             JOIN users u ON u.id = r.from_user_id
             WHERE r.broadcast_id = $1 AND r.created_at > now() - interval '12 seconds'
             ORDER BY r.created_at",
            &[&broadcast_id],
        )
        .await
        .map_err(internal)?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let created_at: std::time::SystemTime = row.get(4);
            ReactionView {
                id: row.get(0),
                emoji: row.get(1),
                from: UserView {
                    id: row.get(2),
                    username: row.get(3),
                },
                created_at: crate::rfc3339(created_at),
            }
        })
        .collect())
}

pub async fn start_broadcast(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<StartBroadcastRequest>,
) -> Result<Json<MembershipView>, (StatusCode, Json<crate::ErrorBody>)> {
    let (user, _) = current_user(&state, &headers).await?;
    let client = state.pool.get().await.map_err(internal)?;

    let already = client
        .query_opt(
            "SELECT 1
             FROM broadcast_members m
             JOIN broadcasts b ON b.id = m.broadcast_id
             WHERE m.user_id = $1 AND b.ended_at IS NULL",
            &[&user.id],
        )
        .await
        .map_err(internal)?;
    if already.is_some() {
        return Err(error(
            StatusCode::CONFLICT,
            "already in a live broadcast",
        ));
    }

    let title = body.title.trim();
    if title.len() > 120 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "title must be 120 characters or less",
        ));
    }
    let tags = parse_tags(title);
    let room_id = format!("{}-{}", user.username, Uuid::new_v4().simple());
    let row = client
        .query_one(
            "INSERT INTO broadcasts (host_user_id, room_id, title, tags, is_public)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING id",
            &[&user.id, &room_id, &title, &tags, &body.is_public],
        )
        .await
        .map_err(internal)?;
    let broadcast_id: i32 = row.get(0);
    client
        .execute(
            "INSERT INTO broadcast_members (broadcast_id, user_id, role) VALUES ($1, $2, 'host')",
            &[&broadcast_id, &user.id],
        )
        .await
        .map_err(internal)?;

    Ok(Json(MembershipView {
        broadcast_id,
        room_id,
        role: "host".into(),
        host: user,
        title: title.to_string(),
    }))
}

pub async fn leave_broadcast(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, (StatusCode, Json<crate::ErrorBody>)> {
    let (user, _) = current_user(&state, &headers).await?;
    let client = state.pool.get().await.map_err(internal)?;
    leave_live_membership(&client, user.id).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn leave_live_membership(
    client: &deadpool_postgres::Object,
    user_id: i32,
) -> anyhow::Result<()> {
    let host = client
        .query_opt(
            "SELECT id FROM broadcasts WHERE host_user_id = $1 AND ended_at IS NULL",
            &[&user_id],
        )
        .await?;
    if let Some(row) = host {
        let broadcast_id: i32 = row.get(0);
        client
            .execute(
                "UPDATE broadcasts SET ended_at = now() WHERE id = $1",
                &[&broadcast_id],
            )
            .await?;
        return Ok(());
    }
    client
        .execute(
            "DELETE FROM broadcast_members m
             USING broadcasts b
             WHERE m.broadcast_id = b.id AND m.user_id = $1 AND b.ended_at IS NULL",
            &[&user_id],
        )
        .await?;
    Ok(())
}

pub async fn end_broadcast(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, (StatusCode, Json<crate::ErrorBody>)> {
    let (user, _) = current_user(&state, &headers).await?;
    let client = state.pool.get().await.map_err(internal)?;
    let updated = client
        .execute(
            "UPDATE broadcasts SET ended_at = now()
             WHERE host_user_id = $1 AND ended_at IS NULL",
            &[&user.id],
        )
        .await
        .map_err(internal)?;
    if updated == 0 {
        return Err(error(StatusCode::NOT_FOUND, "no live broadcast to end"));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn request_join(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(broadcast_id): Path<i32>,
) -> Result<StatusCode, (StatusCode, Json<crate::ErrorBody>)> {
    let (user, _) = current_user(&state, &headers).await?;
    let client = state.pool.get().await.map_err(internal)?;
    let live = client
        .query_opt(
            "SELECT host_user_id FROM broadcasts WHERE id = $1 AND ended_at IS NULL",
            &[&broadcast_id],
        )
        .await
        .map_err(internal)?;
    let Some(row) = live else {
        return Err(error(StatusCode::NOT_FOUND, "broadcast is not live"));
    };
    let host_id: i32 = row.get(0);
    if host_id == user.id {
        return Err(error(StatusCode::BAD_REQUEST, "host is already in the broadcast"));
    }

    let in_other = client
        .query_opt(
            "SELECT 1
             FROM broadcast_members m
             JOIN broadcasts b ON b.id = m.broadcast_id
             WHERE m.user_id = $1 AND b.ended_at IS NULL",
            &[&user.id],
        )
        .await
        .map_err(internal)?;
    if in_other.is_some() {
        return Err(error(StatusCode::CONFLICT, "already in a live broadcast"));
    }

    client
        .execute(
            "INSERT INTO join_requests (broadcast_id, from_user_id, status)
             VALUES ($1, $2, 'pending')
             ON CONFLICT (broadcast_id, from_user_id) DO UPDATE
             SET status = 'pending', granted_role = NULL, created_at = now()",
            &[&broadcast_id, &user.id],
        )
        .await
        .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn accept_join(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((broadcast_id, request_id)): Path<(i32, i32)>,
    Json(body): Json<AcceptRequest>,
) -> Result<StatusCode, (StatusCode, Json<crate::ErrorBody>)> {
    let role = body.role.trim().to_lowercase();
    if role != "listener" && role != "speaker" {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "role must be listener or speaker",
        ));
    }
    let (user, _) = current_user(&state, &headers).await?;
    let client = state.pool.get().await.map_err(internal)?;
    let host = client
        .query_opt(
            "SELECT 1 FROM broadcasts WHERE id = $1 AND host_user_id = $2 AND ended_at IS NULL",
            &[&broadcast_id, &user.id],
        )
        .await
        .map_err(internal)?;
    if host.is_none() {
        return Err(error(StatusCode::FORBIDDEN, "only the host can accept"));
    }
    let request = client
        .query_opt(
            "SELECT from_user_id FROM join_requests
             WHERE id = $1 AND broadcast_id = $2 AND status = 'pending'",
            &[&request_id, &broadcast_id],
        )
        .await
        .map_err(internal)?;
    let Some(request) = request else {
        return Err(error(StatusCode::NOT_FOUND, "join request not found"));
    };
    let from_user_id: i32 = request.get(0);
    client
        .execute(
            "UPDATE join_requests SET status = 'accepted', granted_role = $3
             WHERE id = $1 AND broadcast_id = $2",
            &[&request_id, &broadcast_id, &role],
        )
        .await
        .map_err(internal)?;
    client
        .execute(
            "INSERT INTO broadcast_members (broadcast_id, user_id, role)
             VALUES ($1, $2, $3)
             ON CONFLICT (broadcast_id, user_id) DO UPDATE SET role = EXCLUDED.role",
            &[&broadcast_id, &from_user_id, &role],
        )
        .await
        .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn reject_join(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((broadcast_id, request_id)): Path<(i32, i32)>,
) -> Result<StatusCode, (StatusCode, Json<crate::ErrorBody>)> {
    let (user, _) = current_user(&state, &headers).await?;
    let client = state.pool.get().await.map_err(internal)?;
    let updated = client
        .execute(
            "UPDATE join_requests r SET status = 'rejected'
             FROM broadcasts b
             WHERE r.id = $1 AND r.broadcast_id = $2 AND r.status = 'pending'
               AND b.id = r.broadcast_id AND b.host_user_id = $3 AND b.ended_at IS NULL",
            &[&request_id, &broadcast_id, &user.id],
        )
        .await
        .map_err(internal)?;
    if updated == 0 {
        return Err(error(StatusCode::NOT_FOUND, "join request not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn set_speaking(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(broadcast_id): Path<i32>,
    Json(body): Json<SpeakingRequest>,
) -> Result<StatusCode, (StatusCode, Json<crate::ErrorBody>)> {
    let (user, _) = current_user(&state, &headers).await?;
    let client = state.pool.get().await.map_err(internal)?;
    let member = client
        .query_opt(
            "SELECT m.role FROM broadcast_members m
             JOIN broadcasts b ON b.id = m.broadcast_id
             WHERE m.broadcast_id = $1 AND m.user_id = $2 AND b.ended_at IS NULL",
            &[&broadcast_id, &user.id],
        )
        .await
        .map_err(internal)?;
    let Some(member) = member else {
        return Err(error(StatusCode::FORBIDDEN, "not in this broadcast"));
    };
    let role: String = member.get(0);
    if role == "listener" {
        return Ok(StatusCode::NO_CONTENT);
    }
    client
        .execute(
            "INSERT INTO broadcast_speaking (broadcast_id, user_id, speaking, updated_at)
             VALUES ($1, $2, $3, now())
             ON CONFLICT (broadcast_id, user_id) DO UPDATE
             SET speaking = EXCLUDED.speaking, updated_at = now()",
            &[&broadcast_id, &user.id, &body.speaking],
        )
        .await
        .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn add_reaction(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(broadcast_id): Path<i32>,
    Json(body): Json<ReactionRequest>,
) -> Result<Json<ReactionView>, (StatusCode, Json<crate::ErrorBody>)> {
    let emoji = normalize_reaction(&body.emoji);
    if !ALLOWED_REACTIONS.contains(&emoji.as_str()) {
        return Err(error(StatusCode::BAD_REQUEST, "unsupported reaction"));
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
            "SELECT count(*)::bigint FROM broadcast_reactions
             WHERE broadcast_id = $1 AND from_user_id = $2
               AND created_at > now() - interval '2 seconds'",
            &[&broadcast_id, &user.id],
        )
        .await
        .map_err(internal)?
        .get(0);
    if recent >= 6 {
        return Err(error(StatusCode::TOO_MANY_REQUESTS, "slow down a little"));
    }
    let row = client
        .query_one(
            "INSERT INTO broadcast_reactions (broadcast_id, from_user_id, emoji)
             VALUES ($1, $2, $3)
             RETURNING id, created_at",
            &[&broadcast_id, &user.id, &emoji],
        )
        .await
        .map_err(internal)?;
    let created_at: std::time::SystemTime = row.get(1);
    Ok(Json(ReactionView {
        id: row.get(0),
        emoji,
        from: user,
        created_at: crate::rfc3339(created_at),
    }))
}

fn normalize_reaction(emoji: &str) -> String {
    let trimmed = emoji.trim();
    if trimmed == "❤" || trimmed == "♥" {
        "❤️".into()
    } else {
        trimmed.to_string()
    }
}

pub fn parse_tags(title: &str) -> Vec<String> {
    let mut tags = Vec::new();
    let mut chars = title.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '#' {
            continue;
        }
        let mut tag = String::new();
        while let Some(&next) = chars.peek() {
            if next.is_alphanumeric() || next == '_' {
                for lower in next.to_lowercase() {
                    tag.push(lower);
                }
                chars.next();
            } else {
                break;
            }
        }
        if !tag.is_empty() && !tags.iter().any(|existing| existing == &tag) {
            tags.push(tag);
        }
        if tags.len() == 8 {
            break;
        }
    }
    tags
}

fn normalize_search_tag(raw: &str) -> String {
    raw.trim()
        .trim_start_matches('#')
        .chars()
        .take(32)
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn primary_locale(raw: String) -> String {
    let primary = raw
        .split([',', ';', '-', '_'])
        .next()
        .unwrap_or("en")
        .trim()
        .to_lowercase();
    if primary.is_empty() || primary.len() > 8 {
        "en".into()
    } else {
        primary
    }
}
