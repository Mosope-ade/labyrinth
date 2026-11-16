CREATE FUNCTION labyrinth.require_tenant_access(p_actor uuid,p_tenant uuid)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
BEGIN
    PERFORM 1 FROM labyrinth.users WHERE id=p_actor AND NOT suspended FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_forbidden'; END IF;
    PERFORM 1 FROM labyrinth.tenants WHERE id=p_tenant AND NOT suspended FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_forbidden'; END IF;
    PERFORM 1 FROM labyrinth.memberships WHERE tenant_id=p_tenant AND user_id=p_actor FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_forbidden'; END IF;
END $$;
CREATE FUNCTION labyrinth.list_projects(p_actor uuid,p_tenant uuid,p_after uuid,p_limit integer)
RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE result jsonb;
BEGIN
    IF p_limit IS NULL OR p_limit NOT BETWEEN 1 AND 100 THEN RAISE EXCEPTION 'invalid_page_limit'; END IF;
    PERFORM labyrinth.require_tenant_access(p_actor,p_tenant);
    SELECT coalesce(jsonb_agg(v ORDER BY id),'[]'::jsonb) INTO result FROM (
        SELECT p.id,jsonb_build_object('project_id',p.id,'name',p.name,'head_revision_id',p.head_revision_id,
            'created_at',to_char(p.created_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"')) v
        FROM labyrinth.projects p WHERE p.tenant_id=p_tenant AND (p_after IS NULL OR p.id>p_after)
        ORDER BY p.id LIMIT p_limit+1) page;
    RETURN result;
END $$;
CREATE FUNCTION labyrinth.list_revisions(p_actor uuid,p_tenant uuid,p_project uuid,p_after uuid,p_limit integer)
RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE result jsonb;
BEGIN
    IF p_limit IS NULL OR p_limit NOT BETWEEN 1 AND 100 THEN RAISE EXCEPTION 'invalid_page_limit'; END IF;
    PERFORM labyrinth.require_project_access(p_actor,p_tenant,p_project,false);
    SELECT coalesce(jsonb_agg(v ORDER BY id),'[]'::jsonb) INTO result FROM (
        SELECT r.id,jsonb_build_object('revision_id',r.id,'job_id',r.job_id,'manifest_sha256',r.manifest_sha256,
            'catalog_sha256',r.catalog_sha256,'created_at',to_char(r.created_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"')) v
        FROM labyrinth.revisions r JOIN labyrinth.jobs j ON (j.tenant_id,j.project_id,j.id)=(r.tenant_id,r.project_id,r.job_id)
        WHERE r.tenant_id=p_tenant AND r.project_id=p_project AND j.state='succeeded' AND (p_after IS NULL OR r.id>p_after)
        ORDER BY r.id LIMIT p_limit+1) page;
    RETURN result;
END $$;
CREATE FUNCTION labyrinth.revision_detail(p_actor uuid,p_tenant uuid,p_project uuid,p_revision uuid)
RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE result jsonb;
BEGIN
    PERFORM labyrinth.require_project_access(p_actor,p_tenant,p_project,false);
    SELECT jsonb_build_object('revision_id',r.id,'job_id',r.job_id,'manifest_sha256',r.manifest_sha256,'catalog_sha256',r.catalog_sha256,
        'created_at',to_char(r.created_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"'),
        'artifacts',coalesce((SELECT jsonb_agg(jsonb_build_object('name',f.name,'sha256',f.sha256,'size_bytes',f.size_bytes) ORDER BY f.name)
            FROM labyrinth.artifacts f WHERE (f.tenant_id,f.project_id,f.revision_id)=(r.tenant_id,r.project_id,r.id)),'[]'::jsonb))
        INTO result FROM labyrinth.revisions r JOIN labyrinth.jobs j ON (j.tenant_id,j.project_id,j.id)=(r.tenant_id,r.project_id,r.job_id)
        WHERE r.tenant_id=p_tenant AND r.project_id=p_project AND r.id=p_revision AND j.state='succeeded';
    IF result IS NULL THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_not_found'; END IF;
    RETURN result;
END $$;
REVOKE ALL ON FUNCTION labyrinth.require_tenant_access(uuid,uuid),labyrinth.list_projects(uuid,uuid,uuid,integer),labyrinth.list_revisions(uuid,uuid,uuid,uuid,integer),labyrinth.revision_detail(uuid,uuid,uuid,uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION labyrinth.list_projects(uuid,uuid,uuid,integer),labyrinth.list_revisions(uuid,uuid,uuid,uuid,integer),labyrinth.revision_detail(uuid,uuid,uuid,uuid) TO labyrinth_api;
