-- frontend-db schema. Test users are seeded by web-contact-service on startup.

CREATE TABLE IF NOT EXISTS users (
    id            SERIAL PRIMARY KEY,
    username      TEXT UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    email         TEXT UNIQUE,
    status        TEXT NOT NULL DEFAULT 'active',
    locale        TEXT NOT NULL DEFAULT 'en',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS sessions (
    token      UUID PRIMARY KEY,
    user_id    INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS sessions_user_id_idx ON sessions (user_id);

CREATE TABLE IF NOT EXISTS email_verifications (
    token      UUID PRIMARY KEY,
    code       TEXT UNIQUE NOT NULL,
    user_id    INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    expires_at TIMESTAMPTZ NOT NULL,
    used_at    TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS email_verifications_user_idx ON email_verifications (user_id);

CREATE TABLE IF NOT EXISTS contacts (
    user_id    INTEGER PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    ip_address TEXT,
    port       INTEGER,
    last_seen  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS broadcasts (
    id           SERIAL PRIMARY KEY,
    host_user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    room_id      TEXT UNIQUE NOT NULL,
    title        TEXT NOT NULL DEFAULT '',
    tags         TEXT[] NOT NULL DEFAULT '{}',
    is_public    BOOLEAN NOT NULL DEFAULT true,
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
    broadcast_id     INTEGER REFERENCES broadcasts (id) ON DELETE CASCADE,
    target_user_id   INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    from_user_id     INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    body             TEXT NOT NULL,
    parent_id        INTEGER REFERENCES camera_comments (id) ON DELETE CASCADE,
    is_private       BOOLEAN NOT NULL DEFAULT false,
    reply_to_user_id INTEGER REFERENCES users (id) ON DELETE SET NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS camera_comments_target_idx ON camera_comments (target_user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS camera_comments_broadcast_idx ON camera_comments (broadcast_id, created_at DESC);

CREATE TABLE IF NOT EXISTS broadcast_reactions (
    id           SERIAL PRIMARY KEY,
    broadcast_id INTEGER NOT NULL REFERENCES broadcasts (id) ON DELETE CASCADE,
    from_user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    emoji        TEXT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS broadcast_reactions_live_idx ON broadcast_reactions (broadcast_id, created_at DESC);

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
