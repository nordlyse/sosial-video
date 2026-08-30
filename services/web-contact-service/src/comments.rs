use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{current_user, error, internal, AppState, UserView};

const MAX_COMMENT_LEN: usize = 280;

#[derive(Serialize)]
pub struct CommentView {
    id: i32,
    body: String,
    from: UserView,
    created_at: String,
    parent_id: Option<i32>,
    is_private: bool,
    reply_to: Option<UserView>,
}

#[derive(Deserialize)]
pub struct CommentRequest {
    body: String,
    parent_id: Option<i32>,
    #[serde(default)]
    is_private: bool,
}

pub async fn list_comments(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<i32>,
) -> Result<Json<Vec<CommentView>>, (StatusCode, Json<crate::ErrorBody>)> {
    let (viewer, _) = current_user(&state, &headers).await?;
    let client = state.pool.get().await.map_err(internal)?;
    let exists = client
        .query_opt("SELECT 1 FROM users WHERE id = $1", &[&user_id])
        .await
        .map_err(internal)?;
    if exists.is_none() {
        return Err(error(StatusCode::NOT_FOUND, "user not found"));
    }
    let rows = client
        .query(
            "SELECT c.id, c.body, u.id, u.username, c.created_at,
                    c.parent_id, c.is_private, r.id, r.username
             FROM camera_comments c
             JOIN users u ON u.id = c.from_user_id
             LEFT JOIN users r ON r.id = c.reply_to_user_id
             WHERE c.target_user_id = $1
               AND (
                    c.is_private = false
                    OR c.from_user_id = $2
                    OR c.reply_to_user_id = $2
                    OR c.target_user_id = $2
               )
             ORDER BY c.created_at ASC
             LIMIT 200",
            &[&user_id, &viewer.id],
        )
        .await
        .map_err(internal)?;
    Ok(Json(
        rows.into_iter()
            .map(|row| {
                let created_at: std::time::SystemTime = row.get(4);
                CommentView {
                    id: row.get(0),
                    body: row.get(1),
                    from: UserView {
                        id: row.get(2),
                        username: row.get(3),
                    },
                    created_at: crate::rfc3339(created_at),
                    parent_id: row.get(5),
                    is_private: row.get(6),
                    reply_to: {
                        let reply_id: Option<i32> = row.get(7);
                        let reply_name: Option<String> = row.get(8);
                        match (reply_id, reply_name) {
                            (Some(id), Some(username)) => Some(UserView { id, username }),
                            _ => None,
                        }
                    },
                }
            })
            .collect(),
    ))
}

pub async fn add_comment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<i32>,
    Json(body): Json<CommentRequest>,
) -> Result<Json<CommentView>, (StatusCode, Json<crate::ErrorBody>)> {
    let text = body.body.trim().to_string();
    if text.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "comment cannot be empty"));
    }
    if text.chars().count() > MAX_COMMENT_LEN {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "comment must be 280 characters or fewer",
        ));
    }
    let (user, _) = current_user(&state, &headers).await?;
    let client = state.pool.get().await.map_err(internal)?;
    let exists = client
        .query_opt("SELECT 1 FROM users WHERE id = $1", &[&user_id])
        .await
        .map_err(internal)?;
    if exists.is_none() {
        return Err(error(StatusCode::NOT_FOUND, "user not found"));
    }

    let parent_id = body.parent_id;
    let mut reply_to_user_id: Option<i32> = None;
    let mut reply_to: Option<UserView> = None;
    if let Some(pid) = parent_id {
        let parent = client
            .query_opt(
                "SELECT from_user_id, target_user_id, u.username
                 FROM camera_comments c
                 JOIN users u ON u.id = c.from_user_id
                 WHERE c.id = $1",
                &[&pid],
            )
            .await
            .map_err(internal)?;
        let Some(parent) = parent else {
            return Err(error(StatusCode::NOT_FOUND, "comment not found"));
        };
        let parent_target: i32 = parent.get(1);
        if parent_target != user_id {
            return Err(error(StatusCode::BAD_REQUEST, "comment does not belong here"));
        }
        let from_id: i32 = parent.get(0);
        let from_name: String = parent.get(2);
        reply_to_user_id = Some(from_id);
        reply_to = Some(UserView {
            id: from_id,
            username: from_name,
        });
    } else if body.is_private {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "private replies must be on a comment",
        ));
    }

    let is_private = body.is_private;
    if is_private && user.id != user_id {
        return Err(error(
            StatusCode::FORBIDDEN,
            "only the camera owner can send a private reply",
        ));
    }

    let row = client
        .query_one(
            "INSERT INTO camera_comments
                (target_user_id, from_user_id, body, parent_id, is_private, reply_to_user_id)
             VALUES ($1, $2, $3, $4, $5, $6)
             RETURNING id, created_at",
            &[
                &user_id,
                &user.id,
                &text,
                &parent_id,
                &is_private,
                &reply_to_user_id,
            ],
        )
        .await
        .map_err(internal)?;
    let created_at: std::time::SystemTime = row.get(1);
    Ok(Json(CommentView {
        id: row.get(0),
        body: text,
        from: user,
        created_at: crate::rfc3339(created_at),
        parent_id,
        is_private,
        reply_to,
    }))
}
