-- Run as a dedicated migration owner, never the runtime API login.
CREATE SCHEMA labyrinth;
REVOKE ALL ON SCHEMA labyrinth FROM PUBLIC;
DO $$ BEGIN
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'labyrinth_api') THEN
        CREATE ROLE labyrinth_api NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOBYPASSRLS;
    END IF;
END $$;

CREATE DOMAIN labyrinth.sha256 AS text CHECK (VALUE ~ '^[0-9a-f]{64}$');
CREATE TABLE labyrinth.users (
    id uuid PRIMARY KEY, -- Supabase Auth subject; no passwords or email duplicated here.
    suspended boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE labyrinth.tenants (
    id uuid PRIMARY KEY,
    suspended boolean NOT NULL DEFAULT false,
    max_active_jobs integer NOT NULL CHECK (max_active_jobs BETWEEN 1 AND 1000),
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE labyrinth.memberships (
    tenant_id uuid NOT NULL REFERENCES labyrinth.tenants(id),
    user_id uuid NOT NULL REFERENCES labyrinth.users(id),
    role text NOT NULL CHECK (role IN ('owner', 'editor', 'viewer')),
    PRIMARY KEY (tenant_id, user_id)
);
CREATE INDEX memberships_user ON labyrinth.memberships(user_id, tenant_id);
CREATE TABLE labyrinth.catalogs (
    sha256 labyrinth.sha256 PRIMARY KEY,
    semantic_version text NOT NULL CHECK (length(semantic_version) BETWEEN 1 AND 64),
    yaml text NOT NULL CHECK (octet_length(yaml) BETWEEN 1 AND 1048576),
    admitted boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE labyrinth.projects (
    tenant_id uuid NOT NULL REFERENCES labyrinth.tenants(id),
    id uuid NOT NULL,
    name text NOT NULL CHECK (length(name) BETWEEN 1 AND 200),
    head_revision_id uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id)
);
CREATE TABLE labyrinth.jobs (
    tenant_id uuid NOT NULL,
    project_id uuid NOT NULL,
    id uuid NOT NULL,
    actor_id uuid NOT NULL REFERENCES labyrinth.users(id),
    operation text NOT NULL DEFAULT 'generate_v1' CHECK (operation = 'generate_v1'),
    idempotency_key text NOT NULL CHECK (idempotency_key ~ '^[A-Za-z0-9._:-]{1,128}$'),
    request_sha256 labyrinth.sha256 NOT NULL,
    requirements jsonb NOT NULL CHECK (jsonb_typeof(requirements) = 'object' AND octet_length(requirements::text) <= 1048576),
    provenance jsonb NOT NULL CHECK (jsonb_typeof(provenance) = 'object' AND octet_length(provenance::text) <= 1048576),
    options jsonb NOT NULL CHECK (jsonb_typeof(options) = 'object' AND octet_length(options::text) <= 65536),
    catalog_sha256 labyrinth.sha256 NOT NULL REFERENCES labyrinth.catalogs(sha256),
    expected_head_revision_id uuid,
    base_revision_id uuid,
    base_manifest_sha256 labyrinth.sha256,
    state text NOT NULL DEFAULT 'queued' CHECK (state IN ('queued','running','retry_wait','cancel_requested','cancelled','failed','succeeded')),
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, project_id, id),
    UNIQUE (tenant_id, id),
    UNIQUE (tenant_id, actor_id, operation, idempotency_key),
    FOREIGN KEY (tenant_id, project_id) REFERENCES labyrinth.projects(tenant_id, id),
    CHECK ((base_revision_id IS NULL) = (base_manifest_sha256 IS NULL))
);
CREATE INDEX jobs_admission_quota ON labyrinth.jobs(tenant_id) WHERE state IN ('queued','running','retry_wait','cancel_requested');
CREATE INDEX jobs_project_history ON labyrinth.jobs(tenant_id, project_id, created_at DESC, id);
CREATE TABLE labyrinth.attempts (
    tenant_id uuid NOT NULL,
    project_id uuid NOT NULL,
    job_id uuid NOT NULL,
    id uuid NOT NULL,
    attempt_number integer NOT NULL CHECK (attempt_number > 0),
    fence uuid NOT NULL UNIQUE,
    worker_id text NOT NULL CHECK (length(worker_id) BETWEEN 1 AND 200),
    lease_expires_at timestamptz NOT NULL,
    heartbeat_at timestamptz NOT NULL,
    staged_prefix text NOT NULL UNIQUE CHECK (staged_prefix ~ '^attempts/[a-zA-Z0-9/-]+/$'),
    exit_classification text,
    PRIMARY KEY (tenant_id, project_id, job_id, id),
    UNIQUE (tenant_id, project_id, job_id, attempt_number),
    FOREIGN KEY (tenant_id, project_id, job_id) REFERENCES labyrinth.jobs(tenant_id, project_id, id)
);
CREATE INDEX attempts_expiry ON labyrinth.attempts(lease_expires_at) WHERE exit_classification IS NULL;
CREATE TABLE labyrinth.revisions (
    tenant_id uuid NOT NULL,
    project_id uuid NOT NULL,
    id uuid NOT NULL,
    job_id uuid NOT NULL,
    manifest_sha256 labyrinth.sha256 NOT NULL,
    catalog_sha256 labyrinth.sha256 NOT NULL REFERENCES labyrinth.catalogs(sha256),
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, project_id, id),
    UNIQUE (tenant_id, project_id, job_id),
    FOREIGN KEY (tenant_id, project_id, job_id) REFERENCES labyrinth.jobs(tenant_id, project_id, id)
);
ALTER TABLE labyrinth.projects ADD FOREIGN KEY (tenant_id, id, head_revision_id) REFERENCES labyrinth.revisions(tenant_id, project_id, id);
ALTER TABLE labyrinth.jobs ADD FOREIGN KEY (tenant_id, project_id, expected_head_revision_id) REFERENCES labyrinth.revisions(tenant_id, project_id, id);
ALTER TABLE labyrinth.jobs ADD FOREIGN KEY (tenant_id, project_id, base_revision_id) REFERENCES labyrinth.revisions(tenant_id, project_id, id);
CREATE TABLE labyrinth.artifacts (
    tenant_id uuid NOT NULL,
    project_id uuid NOT NULL,
    revision_id uuid NOT NULL,
    name text NOT NULL CHECK (name IN ('requirements.json','provenance.json','options.json','topology.json','catalog.json','ipam.json','routing.json','firewall.json','verification.json','selection.json','decisions.json','bom.json','manifest.json')),
    object_key text NOT NULL UNIQUE CHECK (length(object_key) BETWEEN 1 AND 512 AND object_key !~ '(^/|\.\.|\\)'),
    sha256 labyrinth.sha256 NOT NULL,
    size_bytes bigint NOT NULL CHECK (size_bytes BETWEEN 1 AND 33554432),
    PRIMARY KEY (tenant_id, project_id, revision_id, name),
    FOREIGN KEY (tenant_id, project_id, revision_id) REFERENCES labyrinth.revisions(tenant_id, project_id, id)
);
CREATE TABLE labyrinth.admin_audit_logs (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    actor_id uuid NOT NULL REFERENCES labyrinth.users(id),
    action text NOT NULL CHECK (length(action) BETWEEN 1 AND 100),
    target_id uuid NOT NULL,
    outcome text NOT NULL CHECK (outcome IN ('allowed','denied','failed')),
    occurred_at timestamptz NOT NULL DEFAULT now()
);
-- No client policies: private application tables are accessed through restricted
-- server functions only. Supabase anon/authenticated get no table privileges.
DO $$ DECLARE t text; BEGIN
    FOR t IN SELECT tablename FROM pg_tables WHERE schemaname = 'labyrinth' LOOP
        EXECUTE format('ALTER TABLE labyrinth.%I ENABLE ROW LEVEL SECURITY', t);
    END LOOP;
END $$;
REVOKE ALL ON ALL TABLES IN SCHEMA labyrinth FROM PUBLIC, labyrinth_api;
REVOKE ALL ON ALL SEQUENCES IN SCHEMA labyrinth FROM PUBLIC, labyrinth_api;
ALTER DEFAULT PRIVILEGES IN SCHEMA labyrinth REVOKE EXECUTE ON FUNCTIONS FROM PUBLIC;

CREATE FUNCTION labyrinth.reject_mutation() RETURNS trigger
LANGUAGE plpgsql SET search_path = pg_catalog AS $$
BEGIN RAISE EXCEPTION USING ERRCODE = '55000', MESSAGE = 'immutable_record'; END $$;
CREATE TRIGGER revisions_immutable BEFORE UPDATE OR DELETE ON labyrinth.revisions FOR EACH ROW EXECUTE FUNCTION labyrinth.reject_mutation();
CREATE TRIGGER artifacts_immutable BEFORE UPDATE OR DELETE ON labyrinth.artifacts FOR EACH ROW EXECUTE FUNCTION labyrinth.reject_mutation();
CREATE TRIGGER audit_immutable BEFORE UPDATE OR DELETE ON labyrinth.admin_audit_logs FOR EACH ROW EXECUTE FUNCTION labyrinth.reject_mutation();
CREATE FUNCTION labyrinth.guard_job_update() RETURNS trigger
LANGUAGE plpgsql SET search_path = pg_catalog AS $$
BEGIN
    IF (to_jsonb(NEW) - 'state') IS DISTINCT FROM (to_jsonb(OLD) - 'state') OR
       OLD.state IN ('succeeded','failed','cancelled') THEN
        RAISE EXCEPTION USING ERRCODE = '55000', MESSAGE = 'immutable_job';
    END IF;
    IF NOT ((OLD.state = 'queued' AND NEW.state IN ('running','cancelled')) OR
        (OLD.state = 'running' AND NEW.state IN ('succeeded','failed','retry_wait','cancel_requested')) OR
        (OLD.state = 'retry_wait' AND NEW.state IN ('queued','cancelled','failed')) OR
        (OLD.state = 'cancel_requested' AND NEW.state = 'cancelled')) THEN
        RAISE EXCEPTION USING ERRCODE = '55000', MESSAGE = 'invalid_job_transition';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER jobs_guard BEFORE UPDATE ON labyrinth.jobs FOR EACH ROW EXECUTE FUNCTION labyrinth.guard_job_update();
-- Historical engineering identities cannot drift behind their stored hashes.
CREATE FUNCTION labyrinth.guard_catalog_update() RETURNS trigger
LANGUAGE plpgsql SET search_path = pg_catalog AS $$
BEGIN
    IF (to_jsonb(NEW) - 'admitted') IS DISTINCT FROM (to_jsonb(OLD) - 'admitted') THEN
        RAISE EXCEPTION USING ERRCODE='55000', MESSAGE='immutable_catalog';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER catalogs_guard BEFORE UPDATE ON labyrinth.catalogs FOR EACH ROW EXECUTE FUNCTION labyrinth.guard_catalog_update();
CREATE TRIGGER catalogs_no_delete BEFORE DELETE ON labyrinth.catalogs FOR EACH ROW EXECUTE FUNCTION labyrinth.reject_mutation();
ALTER TABLE labyrinth.jobs ADD UNIQUE (tenant_id, project_id, id, catalog_sha256);
ALTER TABLE labyrinth.revisions ADD FOREIGN KEY (tenant_id, project_id, job_id, catalog_sha256)
    REFERENCES labyrinth.jobs(tenant_id, project_id, id, catalog_sha256);
ALTER TABLE labyrinth.revisions ADD UNIQUE (tenant_id, project_id, id, manifest_sha256, catalog_sha256);
ALTER TABLE labyrinth.jobs ADD FOREIGN KEY (tenant_id, project_id, base_revision_id, base_manifest_sha256, catalog_sha256)
    REFERENCES labyrinth.revisions(tenant_id, project_id, id, manifest_sha256, catalog_sha256);
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA labyrinth FROM PUBLIC, labyrinth_api;
-- An existing reserved role must not turn the runtime boundary into a superuser.
DO $$ BEGIN
    IF EXISTS (SELECT FROM pg_roles WHERE rolname='labyrinth_api' AND
        (rolsuper OR rolcreatedb OR rolcreaterole OR rolbypassrls OR rolcanlogin)) THEN
        RAISE EXCEPTION 'labyrinth_api must be an unprivileged NOLOGIN group role';
    END IF;
END $$;
