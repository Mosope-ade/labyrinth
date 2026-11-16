CREATE TABLE labyrinth.project_creations (
    tenant_id uuid NOT NULL,
    actor_id uuid NOT NULL REFERENCES labyrinth.users(id),
    idempotency_key text NOT NULL CHECK(idempotency_key ~ '^[A-Za-z0-9._:-]{1,128}$'),
    project_id uuid NOT NULL,
    name text NOT NULL,
    PRIMARY KEY(tenant_id,actor_id,idempotency_key),
    FOREIGN KEY(tenant_id,project_id) REFERENCES labyrinth.projects(tenant_id,id)
);
ALTER TABLE labyrinth.project_creations ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON labyrinth.project_creations FROM PUBLIC,labyrinth_api,labyrinth_worker;
CREATE TRIGGER project_creations_immutable BEFORE UPDATE OR DELETE ON labyrinth.project_creations FOR EACH ROW EXECUTE FUNCTION labyrinth.reject_mutation();
CREATE FUNCTION labyrinth.create_project(p_actor uuid,p_tenant uuid,p_key text,p_name text,p_limit integer)
RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE prior labyrinth.project_creations%ROWTYPE; pid uuid; result jsonb; replay boolean:=false;
BEGIN
    IF p_limit IS NULL OR p_limit NOT BETWEEN 1 AND 10000 OR p_name IS NULL OR length(p_name) NOT BETWEEN 1 AND 200
        OR p_name<>btrim(p_name) OR p_name ~ '[[:cntrl:]]' OR p_key IS NULL OR p_key !~ '^[A-Za-z0-9._:-]{1,128}$' THEN RAISE EXCEPTION 'invalid_project_request'; END IF;
    PERFORM 1 FROM labyrinth.users WHERE id=p_actor AND NOT suspended FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_forbidden'; END IF;
    PERFORM 1 FROM labyrinth.tenants WHERE id=p_tenant AND NOT suspended FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_forbidden'; END IF;
    PERFORM 1 FROM labyrinth.memberships WHERE tenant_id=p_tenant AND user_id=p_actor AND role IN ('owner','editor') FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_forbidden'; END IF;
    SELECT * INTO prior FROM labyrinth.project_creations WHERE tenant_id=p_tenant AND actor_id=p_actor AND idempotency_key=p_key;
    IF FOUND THEN
        IF prior.name<>p_name THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_idempotency_conflict'; END IF;
        pid:=prior.project_id; replay:=true;
    ELSE
        IF (SELECT count(*) FROM labyrinth.projects WHERE tenant_id=p_tenant)>=p_limit THEN
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
CREATE FUNCTION labyrinth.project_detail(p_actor uuid,p_tenant uuid,p_project uuid)
RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE result jsonb;
BEGIN
    PERFORM labyrinth.require_project_access(p_actor,p_tenant,p_project,false);
    SELECT jsonb_build_object('project_id',id,'name',name,'head_revision_id',head_revision_id,
        'created_at',to_char(created_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"'))
        INTO result FROM labyrinth.projects WHERE tenant_id=p_tenant AND id=p_project;
    RETURN result;
END $$;
REVOKE ALL ON FUNCTION labyrinth.create_project(uuid,uuid,text,text,integer),labyrinth.project_detail(uuid,uuid,uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION labyrinth.create_project(uuid,uuid,text,text,integer),labyrinth.project_detail(uuid,uuid,uuid) TO labyrinth_api;
