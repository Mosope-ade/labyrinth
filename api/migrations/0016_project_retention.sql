-- Policy: explicit project deletion only; 7-day recovery; failed payloads 30 days.
DO $$ BEGIN
    IF NOT EXISTS(SELECT FROM pg_roles WHERE rolname='labyrinth_recovery') THEN
        CREATE ROLE labyrinth_recovery NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOBYPASSRLS;
    END IF;
    IF EXISTS(SELECT FROM pg_roles WHERE rolname='labyrinth_recovery' AND (rolsuper OR rolcreatedb OR rolcreaterole OR rolbypassrls OR rolcanlogin)) THEN RAISE EXCEPTION 'labyrinth_recovery must be an unprivileged NOLOGIN group role'; END IF;
END $$;
GRANT USAGE ON SCHEMA labyrinth TO labyrinth_recovery;
ALTER TABLE labyrinth.admin_audit_logs DROP CONSTRAINT admin_audit_logs_target_kind_check;
ALTER TABLE labyrinth.admin_audit_logs ADD CONSTRAINT admin_audit_logs_target_kind_check CHECK(target_kind IN ('legacy','system','user','tenant','catalog','job','project'));
ALTER TABLE labyrinth.projects ADD COLUMN deleted_at timestamptz;
ALTER TABLE labyrinth.projects ADD COLUMN lifecycle_epoch bigint NOT NULL DEFAULT 0;
ALTER TABLE labyrinth.projects ADD COLUMN purged_at timestamptz;
ALTER TABLE labyrinth.jobs ADD COLUMN idempotency_key_sha256 bytea;
ALTER TABLE labyrinth.project_creations ADD COLUMN idempotency_key_sha256 bytea;
ALTER TABLE labyrinth.jobs ADD COLUMN project_epoch bigint NOT NULL DEFAULT 0;
ALTER TABLE labyrinth.jobs ADD COLUMN ended_at timestamptz;
ALTER TABLE labyrinth.jobs ADD COLUMN payload_erased_at timestamptz;
-- Historical terminal jobs begin a conservative fresh retention window.
ALTER TABLE labyrinth.attempts ADD COLUMN project_epoch bigint NOT NULL DEFAULT 0;
ALTER TABLE labyrinth.attempts ADD COLUMN purge_token uuid;
ALTER TABLE labyrinth.attempts ADD COLUMN purge_lease_until timestamptz;
ALTER TABLE labyrinth.attempts ADD COLUMN project_erased_at timestamptz;
ALTER TABLE labyrinth.attempts ADD COLUMN project_erase_due timestamptz;
CREATE INDEX projects_deleted_due ON labyrinth.projects(deleted_at) WHERE deleted_at IS NOT NULL;
CREATE INDEX jobs_retention_due ON labyrinth.jobs(ended_at) WHERE payload_erased_at IS NULL AND state IN ('failed','cancelled');
CREATE INDEX jobs_project_payloads ON labyrinth.jobs(tenant_id,project_id) WHERE payload_erased_at IS NULL;
CREATE INDEX attempts_project_erasure ON labyrinth.attempts(tenant_id,project_id,project_erased_at);
CREATE FUNCTION labyrinth.capture_project_epoch() RETURNS trigger LANGUAGE plpgsql SET search_path=pg_catalog AS $$
BEGIN
    IF TG_TABLE_NAME='jobs' THEN
        SELECT lifecycle_epoch INTO NEW.project_epoch FROM labyrinth.projects WHERE tenant_id=NEW.tenant_id AND id=NEW.project_id;
    ELSE
        SELECT project_epoch INTO NEW.project_epoch FROM labyrinth.jobs WHERE tenant_id=NEW.tenant_id AND id=NEW.job_id;
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER jobs_epoch BEFORE INSERT ON labyrinth.jobs FOR EACH ROW EXECUTE FUNCTION labyrinth.capture_project_epoch();
CREATE TRIGGER attempts_epoch BEFORE INSERT ON labyrinth.attempts FOR EACH ROW EXECUTE FUNCTION labyrinth.capture_project_epoch();
CREATE FUNCTION labyrinth.project_erasure_due(p_tenant uuid,p_project uuid) RETURNS boolean
LANGUAGE sql SECURITY DEFINER SET search_path=pg_catalog AS $$
    SELECT EXISTS(SELECT FROM labyrinth.projects WHERE tenant_id=p_tenant AND id=p_project AND deleted_at<=clock_timestamp()-interval '168 hours');
$$;

CREATE OR REPLACE FUNCTION labyrinth.guard_job_update() RETURNS trigger
LANGUAGE plpgsql SET search_path = pg_catalog AS $$
BEGIN
    IF OLD.idempotency_key_sha256 IS NULL AND NEW.idempotency_key_sha256=sha256(convert_to(OLD.idempotency_key,'UTF8')) AND current_user=(SELECT pg_get_userbyid(relowner) FROM pg_class WHERE oid=TG_RELID) AND (to_jsonb(NEW)-'idempotency_key_sha256')=(to_jsonb(OLD)-'idempotency_key_sha256') THEN RETURN NEW; END IF;
    IF OLD.ended_at IS NULL AND NEW.ended_at IS NOT NULL AND OLD.state IN ('failed','cancelled','succeeded') AND current_user=(SELECT pg_get_userbyid(relowner) FROM pg_class WHERE oid=TG_RELID) AND (to_jsonb(NEW)-'ended_at')=(to_jsonb(OLD)-'ended_at') THEN RETURN NEW; END IF;
    IF current_user=(SELECT pg_get_userbyid(relowner) FROM pg_class WHERE oid=TG_RELID)
        AND OLD.payload_erased_at IS NULL AND NEW.payload_erased_at IS NOT NULL
        AND NEW.idempotency_key IS NULL AND NEW.requirements='{}'::jsonb AND NEW.provenance='{}'::jsonb AND NEW.options='{}'::jsonb
        AND (to_jsonb(NEW)-ARRAY['requirements','provenance','options','payload_erased_at','idempotency_key'])=(to_jsonb(OLD)-ARRAY['requirements','provenance','options','payload_erased_at','idempotency_key'])
        AND ((OLD.state IN ('failed','cancelled') AND OLD.ended_at<=clock_timestamp()-interval '720 hours') OR labyrinth.project_erasure_due(OLD.tenant_id,OLD.project_id)) THEN RETURN NEW; END IF;
    IF (to_jsonb(NEW) - ARRAY['state','ended_at']) IS DISTINCT FROM (to_jsonb(OLD) - ARRAY['state','ended_at']) OR NEW.ended_at IS DISTINCT FROM OLD.ended_at OR
       OLD.state IN ('succeeded','failed','cancelled') THEN
        RAISE EXCEPTION USING ERRCODE = '55000', MESSAGE = 'immutable_job';
    END IF;
    IF NOT ((OLD.state = 'queued' AND NEW.state IN ('running','cancelled')) OR
        (OLD.state = 'running' AND NEW.state IN ('succeeded','failed','retry_wait','cancel_requested')) OR
        (OLD.state = 'retry_wait' AND NEW.state IN ('queued','cancelled','failed')) OR
        (OLD.state = 'cancel_requested' AND NEW.state = 'cancelled')) THEN
        RAISE EXCEPTION USING ERRCODE = '55000', MESSAGE = 'invalid_job_transition';
    END IF;
    IF NEW.state IN ('failed','cancelled','succeeded') THEN NEW.ended_at:=clock_timestamp(); END IF;
    RETURN NEW;
END $$;

UPDATE labyrinth.jobs SET ended_at=clock_timestamp() WHERE state IN ('failed','cancelled','succeeded') AND ended_at IS NULL;

CREATE OR REPLACE FUNCTION labyrinth.require_project_access(p_actor uuid,p_tenant uuid,p_project uuid,p_write boolean)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
BEGIN
    PERFORM 1 FROM labyrinth.users u WHERE u.id=p_actor AND NOT u.suspended FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001', MESSAGE='admission_forbidden'; END IF;
    PERFORM 1 FROM labyrinth.tenants t WHERE t.id=p_tenant AND NOT t.suspended FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001', MESSAGE='admission_forbidden'; END IF;
    PERFORM 1 FROM labyrinth.memberships m WHERE m.tenant_id=p_tenant AND m.user_id=p_actor
        AND (NOT p_write OR m.role IN ('owner','editor')) FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001', MESSAGE='admission_forbidden'; END IF;
    PERFORM 1 FROM labyrinth.projects p WHERE p.tenant_id=p_tenant AND p.id=p_project AND p.deleted_at IS NULL FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001', MESSAGE='admission_not_found'; END IF;
END $$;

CREATE OR REPLACE FUNCTION labyrinth.admit_job(
    p_actor uuid, p_tenant uuid, p_project uuid, p_job uuid,
    p_key text, p_digest text, p_requirements jsonb, p_provenance jsonb,
    p_options jsonb, p_catalog text, p_expected_head uuid, p_base uuid
) RETURNS TABLE(job_id uuid, state text, replayed boolean)
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog AS $$
DECLARE
    max_jobs integer;
    current_head uuid;
    prior labyrinth.jobs%ROWTYPE;
    base_digest text;
BEGIN
    -- Shared authorization locks prevent concurrent suspension/revocation from
    -- being acknowledged before an admission using the old permission commits.
    PERFORM 1 FROM labyrinth.users u WHERE u.id = p_actor AND NOT u.suspended FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001', MESSAGE='admission_forbidden'; END IF;
    SELECT t.max_active_jobs INTO max_jobs FROM labyrinth.tenants t
        WHERE t.id = p_tenant AND NOT t.suspended FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001', MESSAGE='admission_forbidden'; END IF;
    PERFORM 1 FROM labyrinth.memberships m WHERE m.tenant_id = p_tenant AND m.user_id = p_actor
        AND m.role IN ('owner','editor') FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001', MESSAGE='admission_forbidden'; END IF;
    PERFORM 1 FROM labyrinth.projects p WHERE p.tenant_id = p_tenant AND p.id = p_project AND p.deleted_at IS NULL FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001', MESSAGE='admission_not_found'; END IF;

    SELECT j.* INTO prior FROM labyrinth.jobs j WHERE j.tenant_id = p_tenant
        AND j.actor_id = p_actor AND j.operation = 'generate_v1' AND j.idempotency_key_sha256 = sha256(convert_to(p_key,'UTF8'));
    IF FOUND THEN
        IF prior.request_sha256 <> p_digest OR prior.project_id <> p_project THEN
            RAISE EXCEPTION USING ERRCODE='P0001', MESSAGE='admission_idempotency_conflict';
        END IF;
        RETURN QUERY SELECT prior.id, prior.state, true;
        RETURN;
    END IF;
    SELECT p.head_revision_id INTO current_head FROM labyrinth.projects p
        WHERE p.tenant_id = p_tenant AND p.id = p_project AND p.deleted_at IS NULL FOR UPDATE;
    IF current_head IS DISTINCT FROM p_expected_head THEN
        RAISE EXCEPTION USING ERRCODE='P0001', MESSAGE='admission_stale_head';
    END IF;
    PERFORM 1 FROM labyrinth.catalogs c WHERE c.sha256 = p_catalog AND c.admitted FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001', MESSAGE='admission_catalog_unavailable'; END IF;
    IF p_base IS NOT NULL THEN
        IF p_base IS DISTINCT FROM current_head THEN
            RAISE EXCEPTION USING ERRCODE='P0001', MESSAGE='admission_stale_head';
        END IF;
        SELECT r.manifest_sha256 INTO base_digest FROM labyrinth.revisions r
            WHERE r.tenant_id = p_tenant AND r.project_id = p_project AND r.id = p_base
                AND r.catalog_sha256 = p_catalog;
        IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001', MESSAGE='admission_invalid_base'; END IF;
    END IF;
    IF (SELECT count(*) FROM labyrinth.jobs j WHERE j.tenant_id = p_tenant
        AND j.state IN ('queued','running','retry_wait','cancel_requested')) >= max_jobs THEN
        RAISE EXCEPTION USING ERRCODE='P0001', MESSAGE='admission_quota_exceeded';
    END IF;
    INSERT INTO labyrinth.jobs(tenant_id,project_id,id,actor_id,idempotency_key,request_sha256,
        requirements,provenance,options,catalog_sha256,expected_head_revision_id,base_revision_id,base_manifest_sha256)
    VALUES(p_tenant,p_project,p_job,p_actor,p_key,p_digest,p_requirements,p_provenance,p_options,
        p_catalog,p_expected_head,p_base,base_digest);
    PERFORM pgmq.send('labyrinth_generation_v1', jsonb_build_object(
        'schema_version',1,'tenant_id',p_tenant,'job_id',p_job));
    RETURN QUERY SELECT p_job, 'queued'::text, false;
END $$;

CREATE OR REPLACE FUNCTION labyrinth.list_projects(p_actor uuid,p_tenant uuid,p_after uuid,p_limit integer)
RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE result jsonb;
BEGIN
    IF p_limit IS NULL OR p_limit NOT BETWEEN 1 AND 100 THEN RAISE EXCEPTION 'invalid_page_limit'; END IF;
    PERFORM labyrinth.require_tenant_access(p_actor,p_tenant);
    SELECT coalesce(jsonb_agg(v ORDER BY id),'[]'::jsonb) INTO result FROM (
        SELECT p.id,jsonb_build_object('project_id',p.id,'name',p.name,'head_revision_id',p.head_revision_id,
            'created_at',to_char(p.created_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"')) v
        FROM labyrinth.projects p WHERE p.tenant_id=p_tenant AND p.deleted_at IS NULL AND (p_after IS NULL OR p.id>p_after)
        ORDER BY p.id LIMIT p_limit+1) page;
    RETURN result;
END $$;

CREATE OR REPLACE FUNCTION labyrinth.project_detail(p_actor uuid,p_tenant uuid,p_project uuid)
RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE result jsonb;
BEGIN
    PERFORM labyrinth.require_project_access(p_actor,p_tenant,p_project,false);
    SELECT jsonb_build_object('project_id',id,'name',name,'head_revision_id',head_revision_id,
        'created_at',to_char(created_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"'))
        INTO result FROM labyrinth.projects WHERE tenant_id=p_tenant AND id=p_project AND deleted_at IS NULL;
    RETURN result;
END $$;

CREATE OR REPLACE FUNCTION labyrinth.create_project(p_actor uuid,p_tenant uuid,p_key text,p_name text,p_limit integer)
RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE prior labyrinth.project_creations%ROWTYPE; pid uuid; result jsonb; replay boolean:=false; cap integer;
BEGIN
    IF p_limit IS NULL OR p_limit NOT BETWEEN 1 AND 10000 OR p_name IS NULL OR length(p_name) NOT BETWEEN 1 AND 200
        OR p_name<>btrim(p_name) OR p_name ~ '[[:cntrl:]]' OR p_key IS NULL OR p_key !~ '^[A-Za-z0-9._:-]{1,128}$' THEN RAISE EXCEPTION 'invalid_project_request'; END IF;
    PERFORM 1 FROM labyrinth.users WHERE id=p_actor AND NOT suspended FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_forbidden'; END IF;
    SELECT coalesce(max_projects,p_limit) INTO cap FROM labyrinth.tenants WHERE id=p_tenant AND NOT suspended FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_forbidden'; END IF;
    PERFORM 1 FROM labyrinth.memberships WHERE tenant_id=p_tenant AND user_id=p_actor AND role IN ('owner','editor') FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_forbidden'; END IF;
    SELECT * INTO prior FROM labyrinth.project_creations WHERE tenant_id=p_tenant AND actor_id=p_actor AND idempotency_key_sha256=sha256(convert_to(p_key,'UTF8'));
    IF FOUND THEN
        IF EXISTS(SELECT FROM labyrinth.projects WHERE tenant_id=p_tenant AND id=prior.project_id AND deleted_at IS NOT NULL) THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_not_found'; END IF;
        IF prior.name<>p_name THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_idempotency_conflict'; END IF;
        pid:=prior.project_id; replay:=true;
    ELSE
        IF (SELECT count(*) FROM labyrinth.projects WHERE tenant_id=p_tenant AND purged_at IS NULL)>=cap THEN
            RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_quota_exceeded';
        END IF;
        pid:=gen_random_uuid();
        INSERT INTO labyrinth.projects(tenant_id,id,name) VALUES(p_tenant,pid,p_name);
        INSERT INTO labyrinth.project_creations VALUES(p_tenant,p_actor,p_key,pid,p_name);
    END IF;
    SELECT jsonb_build_object('project_id',id,'name',name,'head_revision_id',head_revision_id,
        'created_at',to_char(created_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"'),'replayed',replay)
        INTO result FROM labyrinth.projects WHERE tenant_id=p_tenant AND id=pid;
    RETURN result;
END $$;

CREATE OR REPLACE FUNCTION labyrinth.admin_overview(p_actor uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
BEGIN
    IF NOT labyrinth.admin_access(p_actor) THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_forbidden'; END IF;
    RETURN jsonb_build_object('enabled_users',(SELECT count(*) FROM labyrinth.users WHERE NOT suspended),
        'tenants',(SELECT count(*) FROM labyrinth.tenants),'projects',(SELECT count(*) FROM labyrinth.projects WHERE purged_at IS NULL),
        'job_counts',(SELECT coalesce(jsonb_object_agg(state,n),'{}'::jsonb) FROM (SELECT state,count(*) n FROM labyrinth.jobs GROUP BY state) q),
        'attempt_counts',(SELECT coalesce(jsonb_object_agg(code,n),'{}'::jsonb) FROM (SELECT coalesce(diagnostic_code,CASE WHEN exit_classification IN ('succeeded','cancelled','stale_head','lease_expired','transient_failure','deterministic_failure') THEN exit_classification WHEN exit_classification IS NULL THEN 'running' ELSE 'unknown' END) code,count(*) n FROM labyrinth.attempts GROUP BY 1) q),
        'average_completed_attempt_seconds',(SELECT avg(extract(epoch FROM ended_at-started_at)) FROM labyrinth.attempts WHERE ended_at>=started_at),
        'artifact_bytes',(SELECT coalesce(sum(f.size_bytes),0) FROM labyrinth.artifacts f JOIN labyrinth.projects p ON (p.tenant_id,p.id)=(f.tenant_id,f.project_id) WHERE p.purged_at IS NULL),
        'ai_telemetry',jsonb_build_object('available',false));
END $$;

CREATE OR REPLACE FUNCTION labyrinth.admin_entities(p_actor uuid,p_kind text,p_after uuid,p_limit integer) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE result jsonb;
BEGIN
    IF NOT labyrinth.admin_access(p_actor) THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_forbidden'; END IF;
    IF p_limit IS NULL OR p_limit NOT BETWEEN 1 AND 100 THEN RAISE EXCEPTION 'invalid_page_limit'; END IF;
    IF p_kind='user' THEN
        SELECT coalesce(jsonb_agg(v ORDER BY id),'[]'::jsonb) INTO result FROM (
            SELECT id,jsonb_build_object('id',id,'suspended',suspended,'created_at',to_char(created_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"')) v
            FROM labyrinth.users WHERE p_after IS NULL OR id>p_after ORDER BY id LIMIT p_limit+1) q;
    ELSIF p_kind='tenant' THEN
        SELECT coalesce(jsonb_agg(v ORDER BY id),'[]'::jsonb) INTO result FROM (
            SELECT id,jsonb_build_object('id',id,'suspended',suspended,'max_active_jobs',max_active_jobs,'max_projects',max_projects,
                'project_count',(SELECT count(*) FROM labyrinth.projects p WHERE p.tenant_id=t.id AND p.purged_at IS NULL),
                'artifact_bytes',(SELECT coalesce(sum(size_bytes),0) FROM labyrinth.artifacts f JOIN labyrinth.projects p ON (p.tenant_id,p.id)=(f.tenant_id,f.project_id) WHERE f.tenant_id=t.id AND p.purged_at IS NULL)) v
            FROM labyrinth.tenants t WHERE p_after IS NULL OR id>p_after ORDER BY id LIMIT p_limit+1) q;
    ELSE RAISE EXCEPTION 'invalid_entity_kind'; END IF;
    RETURN result;
END $$;

-- New wrappers invalidate all pre-deletion work, including after a restore.
ALTER FUNCTION labyrinth.publish_attempt(uuid,uuid,uuid,uuid,text,jsonb) RENAME TO publish_attempt_before_retention;
REVOKE ALL ON FUNCTION labyrinth.publish_attempt_before_retention(uuid,uuid,uuid,uuid,text,jsonb) FROM PUBLIC,labyrinth_api,labyrinth_worker;
CREATE FUNCTION labyrinth.publish_attempt(p_tenant uuid,p_job uuid,p_fence uuid,p_revision uuid,p_manifest text,p_files jsonb) RETURNS boolean
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE epoch bigint; pid uuid;
BEGIN
    SELECT project_id INTO pid FROM labyrinth.jobs WHERE tenant_id=p_tenant AND id=p_job;
    SELECT lifecycle_epoch INTO epoch FROM labyrinth.projects WHERE tenant_id=p_tenant AND id=pid AND deleted_at IS NULL FOR UPDATE;
    IF NOT FOUND OR NOT EXISTS(SELECT FROM labyrinth.attempts WHERE tenant_id=p_tenant AND job_id=p_job AND fence=p_fence AND project_epoch=epoch) THEN RETURN false; END IF;
    RETURN labyrinth.publish_attempt_before_retention(p_tenant,p_job,p_fence,p_revision,p_manifest,p_files);
END $$;
ALTER FUNCTION labyrinth.heartbeat_attempt(uuid,uuid,uuid,integer) RENAME TO heartbeat_before_retention;
REVOKE ALL ON FUNCTION labyrinth.heartbeat_before_retention(uuid,uuid,uuid,integer) FROM PUBLIC,labyrinth_api,labyrinth_worker;
CREATE FUNCTION labyrinth.heartbeat_attempt(p_tenant uuid,p_job uuid,p_fence uuid,p_lease integer) RETURNS boolean
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE epoch bigint; pid uuid;
BEGIN
    SELECT project_id INTO pid FROM labyrinth.jobs WHERE tenant_id=p_tenant AND id=p_job;
    SELECT lifecycle_epoch INTO epoch FROM labyrinth.projects WHERE tenant_id=p_tenant AND id=pid AND deleted_at IS NULL FOR SHARE;
    IF NOT FOUND OR NOT EXISTS(SELECT FROM labyrinth.attempts WHERE tenant_id=p_tenant AND job_id=p_job AND fence=p_fence AND project_epoch=epoch) THEN RETURN false; END IF;
    RETURN labyrinth.heartbeat_before_retention(p_tenant,p_job,p_fence,p_lease);
END $$;
ALTER FUNCTION labyrinth.attempt_context(uuid,uuid,uuid) RENAME TO context_before_retention;
REVOKE ALL ON FUNCTION labyrinth.context_before_retention(uuid,uuid,uuid) FROM PUBLIC,labyrinth_api,labyrinth_worker;
CREATE FUNCTION labyrinth.attempt_context(p_tenant uuid,p_job uuid,p_fence uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
BEGIN
    IF NOT EXISTS(SELECT FROM labyrinth.jobs j JOIN labyrinth.projects p ON (p.tenant_id,p.id)=(j.tenant_id,j.project_id) JOIN labyrinth.attempts a ON (a.tenant_id,a.job_id)=(j.tenant_id,j.id) WHERE j.tenant_id=p_tenant AND j.id=p_job AND a.fence=p_fence AND p.deleted_at IS NULL AND a.project_epoch=p.lifecycle_epoch) THEN RETURN NULL; END IF;
    RETURN labyrinth.context_before_retention(p_tenant,p_job,p_fence);
END $$;
CREATE FUNCTION labyrinth.retire_project_jobs(p_tenant uuid,p_project uuid,p_limit integer) RETURNS integer
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE item record; n integer:=0;
BEGIN
    IF p_limit IS NULL OR p_limit NOT BETWEEN 1 AND 100 THEN RAISE EXCEPTION 'invalid_retention_batch'; END IF;
    FOR item IN SELECT j.tenant_id,j.id,j.state FROM labyrinth.jobs j JOIN labyrinth.projects p ON (p.tenant_id,p.id)=(j.tenant_id,j.project_id)
        WHERE (p_tenant IS NULL OR j.tenant_id=p_tenant) AND (p_project IS NULL OR j.project_id=p_project)
        AND (p.deleted_at IS NOT NULL OR p.lifecycle_epoch<>j.project_epoch)
        AND j.state IN ('queued','running','retry_wait','cancel_requested') ORDER BY j.id LIMIT p_limit FOR UPDATE OF j SKIP LOCKED
    LOOP
        IF item.state='running' THEN UPDATE labyrinth.jobs SET state='cancel_requested' WHERE tenant_id=item.tenant_id AND id=item.id; END IF;
        UPDATE labyrinth.jobs SET state='cancelled' WHERE tenant_id=item.tenant_id AND id=item.id;
        UPDATE labyrinth.attempts SET exit_classification='cancelled' WHERE tenant_id=item.tenant_id AND job_id=item.id AND exit_classification IS NULL;
        n:=n+1;
    END LOOP;
    RETURN n;
END $$;
ALTER FUNCTION labyrinth.claim_job(text,integer,integer,integer,integer) RENAME TO claim_before_retention;
REVOKE ALL ON FUNCTION labyrinth.claim_before_retention(text,integer,integer,integer,integer) FROM PUBLIC,labyrinth_api,labyrinth_worker;
CREATE FUNCTION labyrinth.claim_job(p_worker text,p_lease integer,p_budget integer,p_first integer,p_second integer) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE claim jsonb; tid uuid; jid uuid; token uuid;
BEGIN
    claim:=labyrinth.claim_before_retention(p_worker,p_lease,p_budget,p_first,p_second);
    IF claim IS NULL THEN RETURN NULL; END IF;
    tid:=(claim->>'tenant_id')::uuid; jid:=(claim->>'job_id')::uuid; token:=(claim->>'fence')::uuid;
    IF NOT EXISTS(SELECT FROM labyrinth.jobs j JOIN labyrinth.projects p ON (p.tenant_id,p.id)=(j.tenant_id,j.project_id) WHERE j.tenant_id=tid AND j.id=jid AND p.deleted_at IS NULL AND p.lifecycle_epoch=j.project_epoch) THEN
        UPDATE labyrinth.jobs SET state='cancel_requested' WHERE tenant_id=tid AND id=jid AND state='running';
        PERFORM labyrinth.finish_attempt(tid,jid,token,false,p_budget,p_first,p_second);
        RETURN NULL;
    END IF;
    RETURN claim;
END $$;
CREATE FUNCTION labyrinth.project_lifecycle(p_actor uuid,p_tenant uuid,p_project uuid,p_confirm uuid,p_restore boolean) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE p labyrinth.projects%ROWTYPE;
BEGIN
    IF p_confirm IS DISTINCT FROM p_project OR p_restore IS NULL THEN RAISE EXCEPTION 'invalid_project_confirmation'; END IF;
    PERFORM labyrinth.require_tenant_access(p_actor,p_tenant);
    PERFORM 1 FROM labyrinth.memberships WHERE tenant_id=p_tenant AND user_id=p_actor AND role='owner' FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_forbidden'; END IF;
    SELECT * INTO p FROM labyrinth.projects WHERE tenant_id=p_tenant AND id=p_project FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_not_found'; END IF;
    IF p_restore AND p.deleted_at IS NOT NULL THEN
        IF p.deleted_at<=clock_timestamp()-interval '168 hours' OR p.purged_at IS NOT NULL THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_recovery_expired'; END IF;
        UPDATE labyrinth.projects SET deleted_at=NULL,lifecycle_epoch=lifecycle_epoch+1 WHERE tenant_id=p_tenant AND id=p_project RETURNING * INTO p;
        PERFORM labyrinth.admin_log(p_actor,'project_restore','project',p_project::text,p_project,'allowed');
    ELSIF NOT p_restore AND p.deleted_at IS NULL THEN
        UPDATE labyrinth.projects SET deleted_at=clock_timestamp(),lifecycle_epoch=lifecycle_epoch+1 WHERE tenant_id=p_tenant AND id=p_project RETURNING * INTO p;
        PERFORM labyrinth.admin_log(p_actor,'project_delete','project',p_project::text,p_project,'allowed');
    END IF;
    PERFORM labyrinth.retire_project_jobs(p_tenant,p_project,100);
    RETURN jsonb_build_object('project_id',p.id,'state',CASE WHEN p.purged_at IS NOT NULL THEN 'purged' WHEN p.deleted_at IS NOT NULL THEN 'deleted' ELSE 'active' END,
        'deleted_at',to_char(p.deleted_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"'),
        'purge_after',to_char((p.deleted_at+interval '168 hours') AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"'));
END $$;
CREATE FUNCTION labyrinth.deleted_projects(p_actor uuid,p_tenant uuid,p_after uuid,p_limit integer) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE result jsonb;
BEGIN
    PERFORM labyrinth.require_tenant_access(p_actor,p_tenant);
    IF NOT EXISTS(SELECT FROM labyrinth.memberships WHERE tenant_id=p_tenant AND user_id=p_actor AND role='owner') THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_forbidden'; END IF;
    IF p_limit IS NULL OR p_limit NOT BETWEEN 1 AND 100 THEN RAISE EXCEPTION 'invalid_page_limit'; END IF;
    SELECT coalesce(jsonb_agg(v ORDER BY id),'[]'::jsonb) INTO result FROM (
        SELECT id,jsonb_build_object('project_id',id,'name',name,'deleted_at',to_char(deleted_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"'),
            'purge_after',to_char((deleted_at+interval '168 hours') AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"')) v
        FROM labyrinth.projects WHERE tenant_id=p_tenant AND deleted_at IS NOT NULL AND purged_at IS NULL AND (p_after IS NULL OR id>p_after) ORDER BY id LIMIT p_limit+1) q;
    RETURN result;
END $$;

-- Owner-only mutation exceptions are limited to approved, due payload erasure.
CREATE OR REPLACE FUNCTION labyrinth.guard_engine_evidence() RETURNS trigger LANGUAGE plpgsql SET search_path=pg_catalog AS $$
BEGIN
    IF OLD.engine_evidence IS NOT NULL AND NEW.engine_evidence IS DISTINCT FROM OLD.engine_evidence THEN
        IF NEW.engine_evidence IS NULL AND current_user=(SELECT pg_get_userbyid(relowner) FROM pg_class WHERE oid=TG_RELID)
            AND EXISTS(SELECT FROM labyrinth.jobs j WHERE j.tenant_id=OLD.tenant_id AND j.id=OLD.job_id AND ((j.state IN ('failed','cancelled') AND j.ended_at<=clock_timestamp()-interval '720 hours') OR labyrinth.project_erasure_due(j.tenant_id,j.project_id))) THEN RETURN NEW; END IF;
        RAISE EXCEPTION 'immutable_engine_evidence';
    END IF;
    RETURN NEW;
END $$;
CREATE FUNCTION labyrinth.prune_job_payloads(p_limit integer) RETURNS integer LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE item record; n integer:=0;
BEGIN
    IF p_limit IS NULL OR p_limit NOT BETWEEN 1 AND 100 THEN RAISE EXCEPTION 'invalid_retention_batch'; END IF;
    FOR item IN WITH candidates AS (
        (SELECT tenant_id,id FROM labyrinth.jobs WHERE payload_erased_at IS NULL AND state IN ('failed','cancelled') AND ended_at<=clock_timestamp()-interval '720 hours' ORDER BY ended_at,id LIMIT p_limit)
        UNION
        (SELECT j.tenant_id,j.id FROM labyrinth.projects p JOIN labyrinth.jobs j ON (j.tenant_id,j.project_id)=(p.tenant_id,p.id)
         WHERE p.deleted_at<=clock_timestamp()-interval '168 hours' AND j.payload_erased_at IS NULL AND j.state IN ('failed','cancelled','succeeded') ORDER BY j.ended_at,j.id LIMIT p_limit))
        SELECT j.tenant_id,j.id FROM labyrinth.jobs j JOIN candidates c ON (c.tenant_id,c.id)=(j.tenant_id,j.id)
        ORDER BY j.ended_at,j.id LIMIT p_limit FOR UPDATE OF j SKIP LOCKED
    LOOP
        UPDATE labyrinth.attempts SET engine_evidence=NULL WHERE tenant_id=item.tenant_id AND job_id=item.id AND engine_evidence IS NOT NULL;
        UPDATE labyrinth.jobs SET idempotency_key=NULL,requirements='{}',provenance='{}',options='{}',payload_erased_at=clock_timestamp() WHERE tenant_id=item.tenant_id AND id=item.id;
        n:=n+1;
    END LOOP;
    RETURN n;
END $$;
CREATE FUNCTION labyrinth.claim_project_erasure() RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE chosen labyrinth.attempts%ROWTYPE; token uuid:=gen_random_uuid();
BEGIN
    SELECT a.* INTO chosen FROM labyrinth.attempts a JOIN labyrinth.projects p ON (p.tenant_id,p.id)=(a.tenant_id,a.project_id)
        WHERE p.deleted_at<=clock_timestamp()-interval '168 hours' AND a.exit_classification IS NOT NULL
        AND (a.project_erase_due IS NULL OR a.project_erase_due<=clock_timestamp()) AND (a.purge_lease_until IS NULL OR a.purge_lease_until<=clock_timestamp())
        ORDER BY a.project_erase_due NULLS FIRST,a.id LIMIT 1 FOR UPDATE OF a SKIP LOCKED;
    IF NOT FOUND THEN RETURN NULL; END IF;
    UPDATE labyrinth.attempts SET purge_token=token,purge_lease_until=clock_timestamp()+interval '60 seconds' WHERE fence=chosen.fence;
    RETURN jsonb_build_object('fence',chosen.fence,'token',token,'prefix',chosen.staged_prefix);
END $$;
CREATE FUNCTION labyrinth.check_project_erasure(p_fence uuid,p_token uuid) RETURNS text LANGUAGE sql SECURITY DEFINER SET search_path=pg_catalog AS $$
    SELECT a.staged_prefix FROM labyrinth.attempts a JOIN labyrinth.projects p ON (p.tenant_id,p.id)=(a.tenant_id,a.project_id)
    WHERE a.fence=p_fence AND a.purge_token=p_token AND a.purge_lease_until>clock_timestamp() AND p.deleted_at<=clock_timestamp()-interval '168 hours';
$$;
CREATE FUNCTION labyrinth.finish_project_erasure(p_fence uuid,p_token uuid,p_ok boolean) RETURNS boolean LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
BEGIN
    UPDATE labyrinth.attempts SET project_erased_at=CASE WHEN p_ok THEN clock_timestamp() ELSE project_erased_at END,
        project_erase_due=clock_timestamp()+CASE WHEN p_ok THEN interval '24 hours' ELSE interval '60 seconds' END,purge_token=NULL,purge_lease_until=NULL
        WHERE fence=p_fence AND purge_token=p_token AND purge_lease_until>clock_timestamp();
    RETURN FOUND;
END $$;
CREATE OR REPLACE FUNCTION labyrinth.reject_mutation() RETURNS trigger LANGUAGE plpgsql SET search_path=pg_catalog AS $$
BEGIN
    IF TG_TABLE_NAME='project_creations' AND TG_OP='UPDATE' THEN
        IF OLD.idempotency_key_sha256 IS NULL AND NEW.idempotency_key_sha256=sha256(convert_to(OLD.idempotency_key,'UTF8')) AND current_user=(SELECT pg_get_userbyid(relowner) FROM pg_class WHERE oid=TG_RELID) AND (to_jsonb(NEW)-'idempotency_key_sha256')=(to_jsonb(OLD)-'idempotency_key_sha256') THEN RETURN NEW; END IF;
        IF current_user=(SELECT pg_get_userbyid(relowner) FROM pg_class WHERE oid=TG_RELID)
            AND NEW.name='[deleted]' AND NEW.idempotency_key IS NULL AND (to_jsonb(NEW)-ARRAY['name','idempotency_key'])=(to_jsonb(OLD)-ARRAY['name','idempotency_key']) AND labyrinth.project_erasure_due(OLD.tenant_id,OLD.project_id) THEN RETURN NEW; END IF;
    END IF;
    RAISE EXCEPTION USING ERRCODE='55000',MESSAGE='immutable_record';
END $$;
CREATE FUNCTION labyrinth.finish_project_tombstone() RETURNS boolean LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE chosen record;
BEGIN
    SELECT tenant_id,id INTO chosen FROM labyrinth.projects p WHERE deleted_at<=clock_timestamp()-interval '168 hours' AND purged_at IS NULL
        AND NOT EXISTS(SELECT FROM labyrinth.jobs j WHERE (j.tenant_id,j.project_id)=(p.tenant_id,p.id) AND j.payload_erased_at IS NULL)
        AND NOT EXISTS(SELECT FROM labyrinth.attempts a WHERE (a.tenant_id,a.project_id)=(p.tenant_id,p.id) AND a.project_erased_at IS NULL)
        ORDER BY deleted_at,id LIMIT 1 FOR UPDATE SKIP LOCKED;
    IF NOT FOUND THEN RETURN false; END IF;
    UPDATE labyrinth.project_creations SET name='[deleted]',idempotency_key=NULL WHERE tenant_id=chosen.tenant_id AND project_id=chosen.id;
    UPDATE labyrinth.projects SET name='[deleted]',purged_at=clock_timestamp() WHERE tenant_id=chosen.tenant_id AND id=chosen.id;
    RETURN true;
END $$;

-- Offline recovery-role ledger export/reconciliation for offline disaster-recovery gating.
CREATE FUNCTION labyrinth.export_project_lifecycle(p_after_tenant uuid,p_after_project uuid,p_limit integer) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE result jsonb;
BEGIN
    IF p_limit IS NULL OR p_limit NOT BETWEEN 1 AND 100 OR (p_after_tenant IS NULL)<>(p_after_project IS NULL) THEN RAISE EXCEPTION 'invalid_retention_batch'; END IF;
    SELECT coalesce(jsonb_agg(v ORDER BY tenant_id,id),'[]'::jsonb) INTO result FROM (
        SELECT tenant_id,id,jsonb_build_object('tenant_id',tenant_id,'project_id',id,'epoch',lifecycle_epoch::text,
            'deleted_at',to_char(deleted_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"'),'purged',purged_at IS NOT NULL) v
        FROM labyrinth.projects WHERE lifecycle_epoch>0 AND (p_after_tenant IS NULL OR (tenant_id,id)>(p_after_tenant,p_after_project)) ORDER BY tenant_id,id LIMIT p_limit) q;
    RETURN result;
END $$;
CREATE FUNCTION labyrinth.reconcile_project_lifecycle(p_tenant uuid,p_project uuid,p_epoch bigint,p_deleted timestamptz,p_purged boolean) RETURNS boolean
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE current_epoch bigint;
BEGIN
    IF p_epoch IS NULL OR p_epoch<1 OR p_epoch>=9223372036854775807 OR p_purged IS NULL OR (p_purged AND p_deleted IS NULL) OR p_deleted>clock_timestamp() THEN RAISE EXCEPTION 'invalid_lifecycle_ledger'; END IF;
    SELECT lifecycle_epoch INTO current_epoch FROM labyrinth.projects WHERE tenant_id=p_tenant AND id=p_project FOR UPDATE;
    IF NOT FOUND OR current_epoch>p_epoch THEN RETURN false; END IF;
    UPDATE labyrinth.projects SET lifecycle_epoch=p_epoch+1,deleted_at=p_deleted,purged_at=NULL WHERE tenant_id=p_tenant AND id=p_project;
    UPDATE labyrinth.attempts SET project_erased_at=NULL,project_erase_due=NULL,purge_token=NULL,purge_lease_until=NULL WHERE tenant_id=p_tenant AND project_id=p_project;
    PERFORM labyrinth.retire_project_jobs(p_tenant,p_project,100);
    INSERT INTO labyrinth.admin_audit_logs(actor_id,action,target_id,target_kind,target_reference,outcome,operator_principal)
        VALUES(NULL,'restore_reconcile',p_project,'project',p_project::text,'allowed',session_user);
    RETURN true;
END $$;
REVOKE ALL ON FUNCTION labyrinth.export_project_lifecycle(uuid,uuid,integer),labyrinth.reconcile_project_lifecycle(uuid,uuid,bigint,timestamptz,boolean) FROM PUBLIC,labyrinth_api,labyrinth_worker,labyrinth_maintenance;
GRANT EXECUTE ON FUNCTION labyrinth.export_project_lifecycle(uuid,uuid,integer),labyrinth.reconcile_project_lifecycle(uuid,uuid,bigint,timestamptz,boolean) TO labyrinth_recovery;

CREATE FUNCTION labyrinth.ack_terminal_messages(p_limit integer) RETURNS integer
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE item record; n integer:=0;
BEGIN
    IF p_limit IS NULL OR p_limit NOT BETWEEN 1 AND 100 THEN RAISE EXCEPTION 'invalid_retention_batch'; END IF;
    FOR item IN SELECT q.msg_id FROM pgmq.q_labyrinth_generation_v1 q JOIN labyrinth.jobs j
        ON j.tenant_id=(q.message->>'tenant_id')::uuid AND j.id=(q.message->>'job_id')::uuid
        WHERE j.state IN ('failed','cancelled','succeeded') ORDER BY q.msg_id LIMIT p_limit FOR UPDATE OF q SKIP LOCKED
    LOOP PERFORM pgmq.delete('labyrinth_generation_v1',item.msg_id); n:=n+1; END LOOP;
    RETURN n;
END $$;

UPDATE labyrinth.jobs SET idempotency_key_sha256=sha256(convert_to(idempotency_key,'UTF8')) WHERE idempotency_key_sha256 IS NULL;
UPDATE labyrinth.project_creations SET idempotency_key_sha256=sha256(convert_to(idempotency_key,'UTF8')) WHERE idempotency_key_sha256 IS NULL;
ALTER TABLE labyrinth.jobs ALTER COLUMN idempotency_key_sha256 SET NOT NULL;
ALTER TABLE labyrinth.jobs ALTER COLUMN idempotency_key DROP NOT NULL;
CREATE UNIQUE INDEX jobs_idempotency_hash ON labyrinth.jobs(tenant_id,actor_id,operation,idempotency_key_sha256);
ALTER TABLE labyrinth.project_creations DROP CONSTRAINT project_creations_pkey;
ALTER TABLE labyrinth.project_creations ALTER COLUMN idempotency_key DROP NOT NULL;
ALTER TABLE labyrinth.project_creations ALTER COLUMN idempotency_key_sha256 SET NOT NULL;
ALTER TABLE labyrinth.project_creations ADD PRIMARY KEY(tenant_id,actor_id,idempotency_key_sha256);
CREATE FUNCTION labyrinth.capture_idempotency_hash() RETURNS trigger LANGUAGE plpgsql SET search_path=pg_catalog AS $$
BEGIN NEW.idempotency_key_sha256:=sha256(convert_to(NEW.idempotency_key,'UTF8')); RETURN NEW; END $$;
REVOKE ALL ON FUNCTION labyrinth.capture_idempotency_hash() FROM PUBLIC,labyrinth_api,labyrinth_worker;
CREATE TRIGGER jobs_key_hash BEFORE INSERT ON labyrinth.jobs FOR EACH ROW EXECUTE FUNCTION labyrinth.capture_idempotency_hash();
CREATE TRIGGER project_creations_key_hash BEFORE INSERT ON labyrinth.project_creations FOR EACH ROW EXECUTE FUNCTION labyrinth.capture_idempotency_hash();

REVOKE ALL ON FUNCTION labyrinth.capture_project_epoch() FROM PUBLIC,labyrinth_api,labyrinth_worker;

REVOKE ALL ON FUNCTION labyrinth.project_erasure_due(uuid,uuid) FROM PUBLIC,labyrinth_api,labyrinth_worker;

REVOKE ALL ON FUNCTION labyrinth.ack_terminal_messages(integer) FROM PUBLIC,labyrinth_api,labyrinth_worker;

REVOKE ALL ON FUNCTION labyrinth.publish_attempt(uuid,uuid,uuid,uuid,text,jsonb) FROM PUBLIC,labyrinth_api,labyrinth_worker;

REVOKE ALL ON FUNCTION labyrinth.heartbeat_attempt(uuid,uuid,uuid,integer) FROM PUBLIC,labyrinth_api,labyrinth_worker;

REVOKE ALL ON FUNCTION labyrinth.attempt_context(uuid,uuid,uuid) FROM PUBLIC,labyrinth_api,labyrinth_worker;

REVOKE ALL ON FUNCTION labyrinth.claim_job(text,integer,integer,integer,integer) FROM PUBLIC,labyrinth_api,labyrinth_worker;

REVOKE ALL ON FUNCTION labyrinth.retire_project_jobs(uuid,uuid,integer) FROM PUBLIC,labyrinth_api,labyrinth_worker;

REVOKE ALL ON FUNCTION labyrinth.prune_job_payloads(integer) FROM PUBLIC,labyrinth_api,labyrinth_worker;

REVOKE ALL ON FUNCTION labyrinth.claim_project_erasure() FROM PUBLIC,labyrinth_api,labyrinth_worker;

REVOKE ALL ON FUNCTION labyrinth.check_project_erasure(uuid,uuid) FROM PUBLIC,labyrinth_api,labyrinth_worker;

REVOKE ALL ON FUNCTION labyrinth.finish_project_erasure(uuid,uuid,boolean) FROM PUBLIC,labyrinth_api,labyrinth_worker;

REVOKE ALL ON FUNCTION labyrinth.finish_project_tombstone() FROM PUBLIC,labyrinth_api,labyrinth_worker;

REVOKE ALL ON FUNCTION labyrinth.project_lifecycle(uuid,uuid,uuid,uuid,boolean) FROM PUBLIC,labyrinth_api,labyrinth_worker;

REVOKE ALL ON FUNCTION labyrinth.deleted_projects(uuid,uuid,uuid,integer) FROM PUBLIC,labyrinth_api,labyrinth_worker;

GRANT EXECUTE ON FUNCTION labyrinth.ack_terminal_messages(integer) TO labyrinth_worker;

GRANT EXECUTE ON FUNCTION labyrinth.publish_attempt(uuid,uuid,uuid,uuid,text,jsonb) TO labyrinth_worker;

GRANT EXECUTE ON FUNCTION labyrinth.heartbeat_attempt(uuid,uuid,uuid,integer) TO labyrinth_worker;

GRANT EXECUTE ON FUNCTION labyrinth.attempt_context(uuid,uuid,uuid) TO labyrinth_worker;

GRANT EXECUTE ON FUNCTION labyrinth.claim_job(text,integer,integer,integer,integer) TO labyrinth_worker;

GRANT EXECUTE ON FUNCTION labyrinth.retire_project_jobs(uuid,uuid,integer) TO labyrinth_worker;

GRANT EXECUTE ON FUNCTION labyrinth.prune_job_payloads(integer) TO labyrinth_worker;

GRANT EXECUTE ON FUNCTION labyrinth.claim_project_erasure() TO labyrinth_worker;

GRANT EXECUTE ON FUNCTION labyrinth.check_project_erasure(uuid,uuid) TO labyrinth_worker;

GRANT EXECUTE ON FUNCTION labyrinth.finish_project_erasure(uuid,uuid,boolean) TO labyrinth_worker;

GRANT EXECUTE ON FUNCTION labyrinth.finish_project_tombstone() TO labyrinth_worker;

GRANT EXECUTE ON FUNCTION labyrinth.project_lifecycle(uuid,uuid,uuid,uuid,boolean) TO labyrinth_api;

GRANT EXECUTE ON FUNCTION labyrinth.deleted_projects(uuid,uuid,uuid,integer) TO labyrinth_api;
