CREATE FUNCTION labyrinth.list_catalogs(p_actor uuid,p_tenant uuid,p_after text,p_limit integer)
RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE result jsonb;
BEGIN
    IF p_limit IS NULL OR p_limit NOT BETWEEN 1 AND 100 OR (p_after IS NOT NULL AND p_after !~ '^[0-9a-f]{64}$') THEN RAISE EXCEPTION 'invalid_catalog_page'; END IF;
    PERFORM labyrinth.require_tenant_access(p_actor,p_tenant);
    SELECT coalesce(jsonb_agg(v ORDER BY sha256),'[]'::jsonb) INTO result FROM (
        SELECT sha256,jsonb_build_object('sha256',sha256,'semantic_version',semantic_version,
            'created_at',to_char(created_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"')) v
        FROM labyrinth.catalogs WHERE admitted AND (p_after IS NULL OR sha256>p_after) ORDER BY sha256 LIMIT p_limit+1) rows;
    RETURN result;
END $$;
CREATE FUNCTION labyrinth.catalog_payload(p_actor uuid,p_tenant uuid,p_hash text)
RETURNS text LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE result text;
BEGIN
    PERFORM labyrinth.require_tenant_access(p_actor,p_tenant);
    SELECT yaml INTO result FROM labyrinth.catalogs WHERE sha256=p_hash AND admitted;
    IF result IS NULL THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_not_found'; END IF;
    RETURN result;
END $$;
CREATE FUNCTION labyrinth.list_jobs(p_actor uuid,p_tenant uuid,p_project uuid,p_after uuid,p_limit integer)
RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE result jsonb;
BEGIN
    IF p_limit IS NULL OR p_limit NOT BETWEEN 1 AND 100 THEN RAISE EXCEPTION 'invalid_page_limit'; END IF;
    PERFORM labyrinth.require_project_access(p_actor,p_tenant,p_project,false);
    SELECT coalesce(jsonb_agg(v ORDER BY id),'[]'::jsonb) INTO result FROM (
        SELECT j.id,jsonb_build_object('job_id',j.id,'state',j.state,
            'created_at',to_char(j.created_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"'),
            'revision_id',CASE WHEN j.state='succeeded' THEN r.id END) v
        FROM labyrinth.jobs j LEFT JOIN labyrinth.revisions r ON (r.tenant_id,r.project_id,r.job_id)=(j.tenant_id,j.project_id,j.id)
        WHERE j.tenant_id=p_tenant AND j.project_id=p_project AND (p_after IS NULL OR j.id>p_after)
        ORDER BY j.id LIMIT p_limit+1) rows;
    RETURN result;
END $$;
REVOKE ALL ON FUNCTION labyrinth.list_catalogs(uuid,uuid,text,integer),labyrinth.catalog_payload(uuid,uuid,text),labyrinth.list_jobs(uuid,uuid,uuid,uuid,integer) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION labyrinth.list_catalogs(uuid,uuid,text,integer),labyrinth.catalog_payload(uuid,uuid,text),labyrinth.list_jobs(uuid,uuid,uuid,uuid,integer) TO labyrinth_api;
CREATE FUNCTION labyrinth.validation_access(p_actor uuid,p_tenant uuid) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
BEGIN PERFORM labyrinth.require_tenant_access(p_actor,p_tenant); END $$;
REVOKE ALL ON FUNCTION labyrinth.validation_access(uuid,uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION labyrinth.validation_access(uuid,uuid) TO labyrinth_api;
