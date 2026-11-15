-- Read access remains inside narrowly scoped, fixed-search-path functions.
CREATE FUNCTION labyrinth.require_project_access(p_actor uuid,p_tenant uuid,p_project uuid,p_write boolean)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
BEGIN
    PERFORM 1 FROM labyrinth.users u WHERE u.id=p_actor AND NOT u.suspended FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001', MESSAGE='admission_forbidden'; END IF;
    PERFORM 1 FROM labyrinth.tenants t WHERE t.id=p_tenant AND NOT t.suspended FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001', MESSAGE='admission_forbidden'; END IF;
    PERFORM 1 FROM labyrinth.memberships m WHERE m.tenant_id=p_tenant AND m.user_id=p_actor
        AND (NOT p_write OR m.role IN ('owner','editor')) FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001', MESSAGE='admission_forbidden'; END IF;
    PERFORM 1 FROM labyrinth.projects p WHERE p.tenant_id=p_tenant AND p.id=p_project FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001', MESSAGE='admission_not_found'; END IF;
END $$;
REVOKE ALL ON FUNCTION labyrinth.require_project_access(uuid,uuid,uuid,boolean) FROM PUBLIC,labyrinth_api;

CREATE FUNCTION labyrinth.admission_catalog(p_actor uuid,p_tenant uuid,p_project uuid,p_catalog text)
RETURNS TABLE(yaml text) LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
BEGIN
    PERFORM labyrinth.require_project_access(p_actor,p_tenant,p_project,true);
    -- Archived snapshots remain readable for identical idempotency replay. The
    -- admission transaction still refuses NEW jobs using a disabled catalog.
    RETURN QUERY SELECT c.yaml FROM labyrinth.catalogs c WHERE c.sha256=p_catalog;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001', MESSAGE='admission_catalog_unavailable'; END IF;
END $$;
REVOKE ALL ON FUNCTION labyrinth.admission_catalog(uuid,uuid,uuid,text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION labyrinth.admission_catalog(uuid,uuid,uuid,text) TO labyrinth_api;

CREATE FUNCTION labyrinth.job_status(p_actor uuid,p_tenant uuid,p_project uuid,p_job uuid)
RETURNS TABLE(job_id uuid,state text,created_at text,revision_id uuid,manifest_sha256 text)
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
BEGIN
    PERFORM labyrinth.require_project_access(p_actor,p_tenant,p_project,false);
    RETURN QUERY SELECT j.id,j.state,
        to_char(j.created_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"'),
        CASE WHEN j.state='succeeded' THEN r.id END,
        CASE WHEN j.state='succeeded' THEN r.manifest_sha256::text END
    FROM labyrinth.jobs j LEFT JOIN labyrinth.revisions r
        ON r.tenant_id=j.tenant_id AND r.project_id=j.project_id AND r.job_id=j.id
    WHERE j.tenant_id=p_tenant AND j.project_id=p_project AND j.id=p_job;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001', MESSAGE='admission_not_found'; END IF;
END $$;
REVOKE ALL ON FUNCTION labyrinth.job_status(uuid,uuid,uuid,uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION labyrinth.job_status(uuid,uuid,uuid,uuid) TO labyrinth_api;
