ALTER TABLE labyrinth.attempts ADD COLUMN started_at timestamptz;
ALTER TABLE labyrinth.attempts ALTER COLUMN started_at SET DEFAULT clock_timestamp();
ALTER TABLE labyrinth.attempts ADD COLUMN diagnostic_code text CHECK(diagnostic_code IN
    ('invalid_input','engine_rejected','resource_limit','storage_unavailable','database_unavailable','process_io'));
CREATE FUNCTION labyrinth.guard_diagnostic_update() RETURNS trigger
LANGUAGE plpgsql SET search_path=pg_catalog AS $$
BEGIN
    IF OLD.diagnostic_code IS NOT NULL AND NEW.diagnostic_code IS DISTINCT FROM OLD.diagnostic_code THEN RAISE EXCEPTION 'immutable_diagnostic'; END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER attempts_diagnostic_guard BEFORE UPDATE ON labyrinth.attempts FOR EACH ROW EXECUTE FUNCTION labyrinth.guard_diagnostic_update();
CREATE FUNCTION labyrinth.finish_attempt_report(p_tenant uuid,p_job uuid,p_fence uuid,p_code text,p_budget integer,p_first integer,p_second integer)
RETURNS boolean LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE done boolean;
BEGIN
    IF p_code IS NULL OR p_code NOT IN ('invalid_input','engine_rejected','resource_limit','storage_unavailable','database_unavailable','process_io') THEN RAISE EXCEPTION 'invalid_diagnostic'; END IF;
    done:=labyrinth.finish_attempt(p_tenant,p_job,p_fence,p_code IN ('storage_unavailable','database_unavailable','process_io'),p_budget,p_first,p_second);
    IF done THEN UPDATE labyrinth.attempts SET diagnostic_code=p_code WHERE tenant_id=p_tenant AND job_id=p_job AND fence=p_fence; END IF;
    RETURN done;
END $$;
CREATE FUNCTION labyrinth.job_attempts(p_actor uuid,p_tenant uuid,p_project uuid,p_job uuid)
RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE result jsonb;
BEGIN
    PERFORM labyrinth.require_project_access(p_actor,p_tenant,p_project,false);
    PERFORM 1 FROM labyrinth.jobs WHERE tenant_id=p_tenant AND project_id=p_project AND id=p_job;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_not_found'; END IF;
    SELECT coalesce(jsonb_agg(v ORDER BY n),'[]'::jsonb) INTO result FROM (
        SELECT a.attempt_number n,jsonb_build_object('attempt_id',a.id,'attempt_number',a.attempt_number,
            'started_at',to_char(a.started_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"'),
            'finished_at',to_char(a.ended_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"'),
            'retry_scheduled',j.state='retry_wait' AND a.attempt_number=(SELECT max(z.attempt_number) FROM labyrinth.attempts z WHERE z.tenant_id=p_tenant AND z.project_id=p_project AND z.job_id=p_job),
            'retry_at',CASE WHEN j.state='retry_wait' AND a.attempt_number=(SELECT max(z.attempt_number) FROM labyrinth.attempts z WHERE z.tenant_id=p_tenant AND z.project_id=p_project AND z.job_id=p_job) THEN to_char(a.retry_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"') END,
            'code',CASE WHEN a.exit_classification IN ('succeeded','cancelled','stale_head','lease_expired') THEN a.exit_classification
                WHEN a.exit_classification IS NULL THEN NULL ELSE coalesce(a.diagnostic_code,'attempt_failed') END) v
        FROM labyrinth.attempts a JOIN labyrinth.jobs j ON (j.tenant_id,j.project_id,j.id)=(a.tenant_id,a.project_id,a.job_id)
        WHERE a.tenant_id=p_tenant AND a.project_id=p_project AND a.job_id=p_job ORDER BY a.attempt_number LIMIT 3) rows;
    RETURN result;
END $$;
REVOKE ALL ON FUNCTION labyrinth.guard_diagnostic_update(),labyrinth.finish_attempt_report(uuid,uuid,uuid,text,integer,integer,integer),labyrinth.job_attempts(uuid,uuid,uuid,uuid) FROM PUBLIC,labyrinth_api,labyrinth_worker;
GRANT EXECUTE ON FUNCTION labyrinth.finish_attempt_report(uuid,uuid,uuid,text,integer,integer,integer) TO labyrinth_worker;
GRANT EXECUTE ON FUNCTION labyrinth.job_attempts(uuid,uuid,uuid,uuid) TO labyrinth_api;
