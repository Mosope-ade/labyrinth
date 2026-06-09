-- Phase 12: user history — conversation turns linked to users and projects

-- Link turns to the owning user for per-user security scoping.
ALTER TABLE conversation_turns
  ADD COLUMN IF NOT EXISTS user_id UUID REFERENCES users(id) ON DELETE SET NULL;

-- Link a project back to the in-memory session that created it.
-- One session creates at most one project; one project has at most one originating session.
ALTER TABLE projects
  ADD COLUMN IF NOT EXISTS session_id UUID;

CREATE INDEX IF NOT EXISTS projects_session_id_idx ON projects (session_id);
CREATE INDEX IF NOT EXISTS turns_user_id_idx ON conversation_turns (user_id);
