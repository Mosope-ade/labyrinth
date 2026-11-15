CREATE EXTENSION IF NOT EXISTS pgmq;
SELECT pgmq.create('labyrinth_generation_v1');
REVOKE ALL ON SCHEMA pgmq FROM PUBLIC, labyrinth_api;
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA pgmq FROM PUBLIC, labyrinth_api;
-- This application uses a dedicated Supabase project: no queue is client-exposed.
DO $$ BEGIN
    IF EXISTS (SELECT FROM pg_namespace WHERE nspname='pgmq_public') THEN
        REVOKE ALL ON SCHEMA pgmq_public FROM PUBLIC, labyrinth_api;
    END IF;
END $$;
-- Never expose this queue through pgmq_public/PostgREST grants.
DO $$ DECLARE r text; BEGIN
    FOREACH r IN ARRAY ARRAY['anon','authenticated'] LOOP
        IF EXISTS (SELECT FROM pg_roles WHERE rolname = r) THEN
            EXECUTE format('REVOKE ALL ON SCHEMA labyrinth, pgmq FROM %I', r);
            EXECUTE format('REVOKE ALL ON ALL TABLES IN SCHEMA labyrinth FROM %I', r);
            IF EXISTS (SELECT FROM pg_namespace WHERE nspname='pgmq_public') THEN
                EXECUTE format('REVOKE ALL ON SCHEMA pgmq_public FROM %I', r);
            END IF;
        END IF;
    END LOOP;
END $$;

CREATE FUNCTION labyrinth.admit_job(
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
    PERFORM 1 FROM labyrinth.projects p WHERE p.tenant_id = p_tenant AND p.id = p_project;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001', MESSAGE='admission_not_found'; END IF;

    SELECT j.* INTO prior FROM labyrinth.jobs j WHERE j.tenant_id = p_tenant
        AND j.actor_id = p_actor AND j.operation = 'generate_v1' AND j.idempotency_key = p_key;
    IF FOUND THEN
        IF prior.request_sha256 <> p_digest OR prior.project_id <> p_project THEN
            RAISE EXCEPTION USING ERRCODE='P0001', MESSAGE='admission_idempotency_conflict';
        END IF;
        RETURN QUERY SELECT prior.id, prior.state, true;
        RETURN;
    END IF;
    SELECT p.head_revision_id INTO current_head FROM labyrinth.projects p
        WHERE p.tenant_id = p_tenant AND p.id = p_project FOR UPDATE;
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
REVOKE ALL ON FUNCTION labyrinth.admit_job(uuid,uuid,uuid,uuid,text,text,jsonb,jsonb,jsonb,text,uuid,uuid) FROM PUBLIC;
GRANT USAGE ON SCHEMA labyrinth TO labyrinth_api;
GRANT EXECUTE ON FUNCTION labyrinth.admit_job(uuid,uuid,uuid,uuid,text,text,jsonb,jsonb,jsonb,text,uuid,uuid) TO labyrinth_api;
