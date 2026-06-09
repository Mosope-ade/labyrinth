-- Labyrinth initial schema
-- Run with: sqlx migrate run
-- gen_random_uuid() is built-in since PostgreSQL 13 — no extension needed.

-- ── users ────────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS users (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    email           TEXT        NOT NULL UNIQUE,
    password_hash   TEXT        NOT NULL,
    tier            TEXT        NOT NULL DEFAULT 'free' CHECK (tier IN ('free', 'pro', 'enterprise')),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS users_email_idx ON users (email);

-- ── projects ─────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS projects (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name            TEXT        NOT NULL,
    intent_json     JSONB,
    graph_json      JSONB,
    status          TEXT        NOT NULL DEFAULT 'draft'
                        CHECK (status IN ('draft', 'generating', 'ready', 'error')),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS projects_user_id_idx ON projects (user_id);
CREATE INDEX IF NOT EXISTS projects_updated_at_idx ON projects (updated_at DESC);

-- ── jobs ─────────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS jobs (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id      UUID        NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    job_type        TEXT        NOT NULL,
    status          TEXT        NOT NULL DEFAULT 'queued'
                        CHECK (status IN ('queued', 'running', 'complete', 'failed')),
    error_message   TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS jobs_project_id_idx ON jobs (project_id);
CREATE INDEX IF NOT EXISTS jobs_status_idx ON jobs (status);

-- ── conversation_turns ────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS conversation_turns (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id      UUID        NOT NULL,
    project_id      UUID        REFERENCES projects (id) ON DELETE SET NULL,
    role            TEXT        NOT NULL CHECK (role IN ('system', 'user', 'assistant')),
    content         TEXT        NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS turns_session_id_idx ON conversation_turns (session_id);
CREATE INDEX IF NOT EXISTS turns_project_id_idx ON conversation_turns (project_id);
