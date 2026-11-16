-- Private, bounded counterexamples; only fenced worker finalization can write them.
ALTER TABLE labyrinth.attempts ADD COLUMN engine_evidence jsonb
    CHECK(engine_evidence IS NULL OR octet_length(engine_evidence::text)<=65536);
CREATE FUNCTION labyrinth.guard_engine_evidence() RETURNS trigger
LANGUAGE plpgsql SET search_path=pg_catalog AS $$
BEGIN
    IF OLD.engine_evidence IS NOT NULL AND NEW.engine_evidence IS DISTINCT FROM OLD.engine_evidence THEN
        RAISE EXCEPTION 'immutable_engine_evidence';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER attempts_engine_evidence BEFORE UPDATE ON labyrinth.attempts FOR EACH ROW EXECUTE FUNCTION labyrinth.guard_engine_evidence();
CREATE FUNCTION labyrinth.finish_attempt_evidence(p_tenant uuid,p_job uuid,p_fence uuid,p_code text,
    p_budget integer,p_first integer,p_second integer,p_evidence jsonb)
RETURNS boolean LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE done boolean;
BEGIN
    IF p_evidence IS NOT NULL THEN
        IF NOT coalesce(p_code IN ('engine_rejected','resource_limit')
            AND jsonb_typeof(p_evidence)='object'
            AND octet_length(p_evidence::text)<=65536
            AND p_evidence ?& ARRAY['schema_version','outcome','truncated','error']
            AND (p_evidence-ARRAY['schema_version','outcome','truncated','error'])='{}'::jsonb
            AND p_evidence->>'schema_version'='engine_failure_v1'
            AND p_evidence->>'outcome'=CASE WHEN p_code='resource_limit' THEN 'inconclusive' ELSE 'rejected' END
            AND jsonb_typeof(p_evidence->'truncated')='boolean'
            AND jsonb_typeof(p_evidence->'error') IN ('object','null')
            AND (p_evidence->>'truncated'<>'true' OR p_evidence->'error'='null'::jsonb),false) THEN
            RAISE EXCEPTION 'invalid_engine_evidence';
        END IF;
    END IF;
    done:=labyrinth.finish_attempt_report(p_tenant,p_job,p_fence,p_code,p_budget,p_first,p_second);
    IF done AND p_evidence IS NOT NULL THEN
        UPDATE labyrinth.attempts SET engine_evidence=p_evidence
        WHERE tenant_id=p_tenant AND job_id=p_job AND fence=p_fence AND exit_classification='deterministic_failure';
        -- Cancellation may win finalization; never attach engine rejection evidence to it.
    END IF;
    RETURN done;
END $$;
CREATE FUNCTION labyrinth.attempt_evidence(p_actor uuid,p_tenant uuid,p_project uuid,p_job uuid,p_attempt uuid)
RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE result jsonb;
BEGIN
    PERFORM labyrinth.require_project_access(p_actor,p_tenant,p_project,false);
    SELECT jsonb_build_object('attempt_id',a.id,'evidence',a.engine_evidence) INTO result
    FROM labyrinth.attempts a WHERE a.tenant_id=p_tenant AND a.project_id=p_project AND a.job_id=p_job AND a.id=p_attempt;
    IF result IS NULL THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_not_found'; END IF;
    RETURN result;
END $$;
REVOKE ALL ON FUNCTION labyrinth.guard_engine_evidence(),labyrinth.finish_attempt_evidence(uuid,uuid,uuid,text,integer,integer,integer,jsonb),labyrinth.attempt_evidence(uuid,uuid,uuid,uuid,uuid) FROM PUBLIC,labyrinth_api,labyrinth_worker;
GRANT EXECUTE ON FUNCTION labyrinth.finish_attempt_evidence(uuid,uuid,uuid,text,integer,integer,integer,jsonb) TO labyrinth_worker;
GRANT EXECUTE ON FUNCTION labyrinth.attempt_evidence(uuid,uuid,uuid,uuid,uuid) TO labyrinth_api;
