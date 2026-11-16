CREATE TABLE labyrinth.admin_roles(user_id uuid PRIMARY KEY REFERENCES labyrinth.users(id),granted_at timestamptz NOT NULL DEFAULT clock_timestamp());
ALTER TABLE labyrinth.admin_roles ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON labyrinth.admin_roles FROM PUBLIC,labyrinth_api,labyrinth_worker;
ALTER TABLE labyrinth.admin_audit_logs ALTER COLUMN target_id DROP NOT NULL;
ALTER TABLE labyrinth.admin_audit_logs ADD COLUMN target_kind text NOT NULL DEFAULT 'legacy' CHECK(target_kind IN ('legacy','system','user','tenant','catalog','job'));
ALTER TABLE labyrinth.admin_audit_logs ADD COLUMN target_reference text CHECK(length(target_reference) BETWEEN 1 AND 128);
CREATE UNIQUE INDEX catalogs_one_hash_per_version ON labyrinth.catalogs(semantic_version);
CREATE FUNCTION labyrinth.admin_access(p_actor uuid) RETURNS boolean
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
BEGIN
    PERFORM 1 FROM labyrinth.users u JOIN labyrinth.admin_roles a ON a.user_id=u.id WHERE u.id=p_actor AND NOT u.suspended FOR SHARE OF u,a;
    RETURN FOUND;
END $$;
CREATE FUNCTION labyrinth.admin_log(p_actor uuid,p_action text,p_kind text,p_reference text,p_target uuid,p_outcome text) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
BEGIN
    INSERT INTO labyrinth.admin_audit_logs(actor_id,action,target_id,target_kind,target_reference,outcome)
        SELECT p_actor,p_action,p_target,p_kind,p_reference,p_outcome WHERE EXISTS(SELECT FROM labyrinth.users WHERE id=p_actor);
END $$;
CREATE FUNCTION labyrinth.admin_denied(p_actor uuid) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
BEGIN PERFORM labyrinth.admin_log(p_actor,'admin_access','system','admin',NULL,'denied'); END $$;
CREATE FUNCTION labyrinth.admin_overview(p_actor uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
BEGIN
    IF NOT labyrinth.admin_access(p_actor) THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_forbidden'; END IF;
    RETURN jsonb_build_object('enabled_users',(SELECT count(*) FROM labyrinth.users WHERE NOT suspended),
        'tenants',(SELECT count(*) FROM labyrinth.tenants),'projects',(SELECT count(*) FROM labyrinth.projects),
        'job_counts',(SELECT coalesce(jsonb_object_agg(state,n),'{}'::jsonb) FROM (SELECT state,count(*) n FROM labyrinth.jobs GROUP BY state) q),
        'attempt_counts',(SELECT coalesce(jsonb_object_agg(code,n),'{}'::jsonb) FROM (SELECT coalesce(diagnostic_code,CASE WHEN exit_classification IN ('succeeded','cancelled','stale_head','lease_expired','transient_failure','deterministic_failure') THEN exit_classification WHEN exit_classification IS NULL THEN 'running' ELSE 'unknown' END) code,count(*) n FROM labyrinth.attempts GROUP BY 1) q),
        'average_completed_attempt_seconds',(SELECT avg(extract(epoch FROM ended_at-started_at)) FROM labyrinth.attempts WHERE ended_at>=started_at),
        'artifact_bytes',(SELECT coalesce(sum(size_bytes),0) FROM labyrinth.artifacts),
        'ai_telemetry',jsonb_build_object('available',false));
END $$;
CREATE FUNCTION labyrinth.admin_entities(p_actor uuid,p_kind text,p_after uuid,p_limit integer) RETURNS jsonb
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
                'project_count',(SELECT count(*) FROM labyrinth.projects p WHERE p.tenant_id=t.id),
                'artifact_bytes',(SELECT coalesce(sum(size_bytes),0) FROM labyrinth.artifacts f WHERE f.tenant_id=t.id)) v
            FROM labyrinth.tenants t WHERE p_after IS NULL OR id>p_after ORDER BY id LIMIT p_limit+1) q;
    ELSE RAISE EXCEPTION 'invalid_entity_kind'; END IF;
    RETURN result;
END $$;
CREATE FUNCTION labyrinth.admin_audit(p_actor uuid,p_before bigint,p_limit integer) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE result jsonb;
BEGIN
    IF NOT labyrinth.admin_access(p_actor) THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_forbidden'; END IF;
    IF p_limit IS NULL OR p_limit NOT BETWEEN 1 AND 100 OR p_before<=0 THEN RAISE EXCEPTION 'invalid_page_limit'; END IF;
    SELECT coalesce(jsonb_agg(v ORDER BY id DESC),'[]'::jsonb) INTO result FROM (
        SELECT id,jsonb_build_object('id',id::text,'actor_id',actor_id,'action',action,'target_kind',target_kind,
            'target_reference',coalesce(target_reference,target_id::text),'outcome',outcome,
            'occurred_at',to_char(occurred_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"')) v
        FROM labyrinth.admin_audit_logs WHERE p_before IS NULL OR id<p_before ORDER BY id DESC LIMIT p_limit+1) q;
    RETURN result;
END $$;
CREATE FUNCTION labyrinth.admin_catalog_note(p_actor uuid,p_action text,p_hash text,p_valid boolean) RETURNS boolean
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE allowed boolean;
BEGIN
    IF p_action NOT IN ('catalog_validate','catalog_promote') OR (p_hash IS NOT NULL AND p_hash !~ '^[0-9a-f]{64}$') THEN RAISE EXCEPTION 'invalid_catalog_note'; END IF;
    allowed:=labyrinth.admin_access(p_actor);
    PERFORM labyrinth.admin_log(p_actor,p_action,'catalog',p_hash,NULL,CASE WHEN NOT allowed THEN 'denied' WHEN p_valid THEN 'allowed' ELSE 'failed' END);
    RETURN allowed;
END $$;
CREATE FUNCTION labyrinth.admin_promote_catalog(p_actor uuid,p_hash text,p_version text,p_yaml text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE prior text; existed boolean; active boolean;
BEGIN
    IF NOT labyrinth.admin_access(p_actor) THEN
        PERFORM labyrinth.admin_log(p_actor,'catalog_promote','catalog',p_hash,NULL,'denied'); RETURN jsonb_build_object('error','forbidden');
    END IF;
    PERFORM pg_advisory_xact_lock(hashtextextended('labyrinth/catalog/'||p_version,0));
    SELECT sha256,admitted INTO prior,active FROM labyrinth.catalogs WHERE semantic_version=p_version FOR UPDATE;
    IF FOUND AND prior<>p_hash THEN
        PERFORM labyrinth.admin_log(p_actor,'catalog_promote','catalog',p_hash,NULL,'failed'); RETURN jsonb_build_object('error','catalog_version_conflict');
    END IF;
    existed:=FOUND;
    IF NOT existed THEN INSERT INTO labyrinth.catalogs(sha256,semantic_version,yaml,admitted) VALUES(p_hash,p_version,p_yaml,true); active:=true; END IF;
    PERFORM labyrinth.admin_log(p_actor,'catalog_promote','catalog',p_hash,NULL,'allowed');
    RETURN jsonb_build_object('sha256',p_hash,'semantic_version',p_version,'admitted',active,'replayed',existed);
END $$;
CREATE FUNCTION labyrinth.admin_catalog_state(p_actor uuid,p_hash text,p_expected boolean,p_admitted boolean) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE current_state boolean;
BEGIN
    IF NOT labyrinth.admin_access(p_actor) THEN
        PERFORM labyrinth.admin_log(p_actor,'catalog_admission','catalog',p_hash,NULL,'denied'); RETURN jsonb_build_object('error','forbidden'); END IF;
    SELECT admitted INTO current_state FROM labyrinth.catalogs WHERE sha256=p_hash FOR UPDATE;
    IF NOT FOUND THEN PERFORM labyrinth.admin_log(p_actor,'catalog_admission','catalog',p_hash,NULL,'failed'); RETURN jsonb_build_object('error','not_found'); END IF;
    IF current_state IS DISTINCT FROM p_expected THEN
        PERFORM labyrinth.admin_log(p_actor,'catalog_admission','catalog',p_hash,NULL,'failed'); RETURN jsonb_build_object('error','stale_state'); END IF;
    UPDATE labyrinth.catalogs SET admitted=p_admitted WHERE sha256=p_hash;
    PERFORM labyrinth.admin_log(p_actor,'catalog_admission','catalog',p_hash,NULL,'allowed');
    RETURN jsonb_build_object('sha256',p_hash,'admitted',p_admitted);
END $$;
CREATE FUNCTION labyrinth.admin_suspend(p_actor uuid,p_kind text,p_target uuid,p_expected boolean,p_suspended boolean) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE current_state boolean;
BEGIN
    IF p_kind NOT IN ('user','tenant') THEN RAISE EXCEPTION 'invalid_entity_kind'; END IF;
    IF NOT labyrinth.admin_access(p_actor) THEN
        PERFORM labyrinth.admin_log(p_actor,'suspension',p_kind,p_target::text,p_target,'denied'); RETURN jsonb_build_object('error','forbidden'); END IF;
    IF p_kind='user' THEN
        IF EXISTS(SELECT FROM labyrinth.admin_roles WHERE user_id=p_target) THEN
            PERFORM labyrinth.admin_log(p_actor,'suspension',p_kind,p_target::text,p_target,'denied'); RETURN jsonb_build_object('error','operator_required'); END IF;
        SELECT suspended INTO current_state FROM labyrinth.users WHERE id=p_target FOR UPDATE;
        IF EXISTS(SELECT FROM labyrinth.admin_roles WHERE user_id=p_target) THEN PERFORM labyrinth.admin_log(p_actor,'suspension',p_kind,p_target::text,p_target,'denied'); RETURN jsonb_build_object('error','operator_required'); END IF;
    ELSE SELECT suspended INTO current_state FROM labyrinth.tenants WHERE id=p_target FOR UPDATE; END IF;
    IF current_state IS NULL THEN PERFORM labyrinth.admin_log(p_actor,'suspension',p_kind,p_target::text,p_target,'failed'); RETURN jsonb_build_object('error','not_found'); END IF;
    IF current_state IS DISTINCT FROM p_expected THEN
        PERFORM labyrinth.admin_log(p_actor,'suspension',p_kind,p_target::text,p_target,'failed'); RETURN jsonb_build_object('error','stale_state'); END IF;
    IF p_kind='user' THEN UPDATE labyrinth.users SET suspended=p_suspended WHERE id=p_target;
    ELSE UPDATE labyrinth.tenants SET suspended=p_suspended WHERE id=p_target; END IF;
    PERFORM labyrinth.admin_log(p_actor,'suspension',p_kind,p_target::text,p_target,'allowed');
    RETURN jsonb_build_object('id',p_target,'suspended',p_suspended);
END $$;
REVOKE ALL ON FUNCTION labyrinth.admin_access(uuid),labyrinth.admin_log(uuid,text,text,text,uuid,text),labyrinth.admin_denied(uuid),labyrinth.admin_overview(uuid),labyrinth.admin_entities(uuid,text,uuid,integer),labyrinth.admin_audit(uuid,bigint,integer),labyrinth.admin_catalog_note(uuid,text,text,boolean),labyrinth.admin_promote_catalog(uuid,text,text,text),labyrinth.admin_catalog_state(uuid,text,boolean,boolean),labyrinth.admin_suspend(uuid,text,uuid,boolean,boolean) FROM PUBLIC,labyrinth_api,labyrinth_worker;
GRANT EXECUTE ON FUNCTION labyrinth.admin_access(uuid),labyrinth.admin_denied(uuid),labyrinth.admin_overview(uuid),labyrinth.admin_entities(uuid,text,uuid,integer),labyrinth.admin_audit(uuid,bigint,integer),labyrinth.admin_catalog_note(uuid,text,text,boolean),labyrinth.admin_promote_catalog(uuid,text,text,text),labyrinth.admin_catalog_state(uuid,text,boolean,boolean),labyrinth.admin_suspend(uuid,text,uuid,boolean,boolean) TO labyrinth_api;

CREATE FUNCTION labyrinth.admin_job(p_actor uuid,p_tenant uuid,p_project uuid,p_job uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE result jsonb;
BEGIN
    IF NOT labyrinth.admin_access(p_actor) THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_forbidden'; END IF;
    SELECT jsonb_build_object('tenant_id',tenant_id,'project_id',project_id,'job_id',id,'state',state,
        'created_at',to_char(created_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"'),
        'attempts',(SELECT count(*) FROM labyrinth.attempts a WHERE a.tenant_id=p_tenant AND a.project_id=p_project AND a.job_id=p_job))
        INTO result FROM labyrinth.jobs WHERE tenant_id=p_tenant AND project_id=p_project AND id=p_job;
    IF result IS NULL THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_not_found'; END IF;
    RETURN result;
END $$;
CREATE FUNCTION labyrinth.admin_cancel_job(p_actor uuid,p_tenant uuid,p_project uuid,p_job uuid,p_expected text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE current_state text;
BEGIN
    IF NOT labyrinth.admin_access(p_actor) THEN
        PERFORM labyrinth.admin_log(p_actor,'job_cancel','job',p_job::text,p_job,'denied');RETURN jsonb_build_object('error','forbidden'); END IF;
    PERFORM 1 FROM labyrinth.projects WHERE tenant_id=p_tenant AND id=p_project FOR SHARE;
    SELECT state INTO current_state FROM labyrinth.jobs WHERE tenant_id=p_tenant AND project_id=p_project AND id=p_job FOR UPDATE;
    IF NOT FOUND THEN PERFORM labyrinth.admin_log(p_actor,'job_cancel','job',p_job::text,p_job,'failed');RETURN jsonb_build_object('error','not_found'); END IF;
    IF current_state IS DISTINCT FROM p_expected THEN PERFORM labyrinth.admin_log(p_actor,'job_cancel','job',p_job::text,p_job,'failed');RETURN jsonb_build_object('error','stale_state'); END IF;
    IF current_state IN ('queued','retry_wait') THEN
        UPDATE labyrinth.jobs SET state='cancelled' WHERE tenant_id=p_tenant AND id=p_job;
        UPDATE labyrinth.attempts SET exit_classification='cancelled' WHERE tenant_id=p_tenant AND project_id=p_project AND job_id=p_job AND exit_classification IS NULL;
        current_state:='cancelled';
    ELSIF current_state='running' THEN UPDATE labyrinth.jobs SET state='cancel_requested' WHERE tenant_id=p_tenant AND id=p_job;current_state:='cancel_requested'; END IF;
    PERFORM labyrinth.admin_log(p_actor,'job_cancel','job',p_job::text,p_job,'allowed');
    RETURN jsonb_build_object('job_id',p_job,'state',current_state);
END $$;
REVOKE ALL ON FUNCTION labyrinth.admin_job(uuid,uuid,uuid,uuid),labyrinth.admin_cancel_job(uuid,uuid,uuid,uuid,text) FROM PUBLIC,labyrinth_api,labyrinth_worker;
GRANT EXECUTE ON FUNCTION labyrinth.admin_job(uuid,uuid,uuid,uuid),labyrinth.admin_cancel_job(uuid,uuid,uuid,uuid,text) TO labyrinth_api;
