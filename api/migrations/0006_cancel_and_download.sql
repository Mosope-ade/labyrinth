CREATE FUNCTION labyrinth.cancel_job(p_actor uuid,p_tenant uuid,p_project uuid,p_job uuid)
RETURNS text LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE s text;
BEGIN
    PERFORM labyrinth.require_project_access(p_actor,p_tenant,p_project,true);
    SELECT state INTO s FROM labyrinth.jobs WHERE tenant_id=p_tenant AND project_id=p_project AND id=p_job FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_not_found'; END IF;
    IF s IN ('queued','retry_wait') THEN
        UPDATE labyrinth.jobs SET state='cancelled' WHERE tenant_id=p_tenant AND id=p_job;
        UPDATE labyrinth.attempts SET exit_classification='cancelled' WHERE tenant_id=p_tenant AND job_id=p_job AND exit_classification IS NULL;
        -- Queue redelivery acknowledges this terminal job without execution.
        RETURN 'cancelled';
    ELSIF s='running' THEN
        UPDATE labyrinth.jobs SET state='cancel_requested' WHERE tenant_id=p_tenant AND id=p_job;
        RETURN 'cancel_requested';
    END IF;
    RETURN s; -- Idempotent; completed history is never rewritten.
END $$;
CREATE FUNCTION labyrinth.download_artifact(p_actor uuid,p_tenant uuid,p_project uuid,p_revision uuid,p_name text)
RETURNS TABLE(object_key text,sha256 text,size_bytes bigint)
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
BEGIN
    PERFORM labyrinth.require_project_access(p_actor,p_tenant,p_project,false);
    RETURN QUERY SELECT f.object_key,f.sha256::text,f.size_bytes FROM labyrinth.artifacts f
        JOIN labyrinth.revisions r ON (r.tenant_id,r.project_id,r.id)=(f.tenant_id,f.project_id,f.revision_id)
        JOIN labyrinth.jobs j ON (j.tenant_id,j.project_id,j.id)=(r.tenant_id,r.project_id,r.job_id)
        WHERE f.tenant_id=p_tenant AND f.project_id=p_project AND f.revision_id=p_revision AND f.name=p_name AND j.state='succeeded';
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_not_found'; END IF;
END $$;
REVOKE ALL ON FUNCTION labyrinth.cancel_job(uuid,uuid,uuid,uuid),labyrinth.download_artifact(uuid,uuid,uuid,uuid,text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION labyrinth.cancel_job(uuid,uuid,uuid,uuid),labyrinth.download_artifact(uuid,uuid,uuid,uuid,text) TO labyrinth_api;

REVOKE ALL ON FUNCTION labyrinth.cancel_job(uuid,uuid,uuid,uuid),labyrinth.download_artifact(uuid,uuid,uuid,uuid,text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION labyrinth.cancel_job(uuid,uuid,uuid,uuid),labyrinth.download_artifact(uuid,uuid,uuid,uuid,text) TO labyrinth_api;
