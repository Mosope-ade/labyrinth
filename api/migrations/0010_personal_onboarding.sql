ALTER TABLE labyrinth.tenants ADD COLUMN max_projects integer CHECK(max_projects BETWEEN 1 AND 10000);
CREATE TABLE labyrinth.personal_workspaces (
    user_id uuid PRIMARY KEY REFERENCES labyrinth.users(id),
    tenant_id uuid NOT NULL UNIQUE REFERENCES labyrinth.tenants(id)
);
ALTER TABLE labyrinth.personal_workspaces ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON labyrinth.personal_workspaces FROM PUBLIC,labyrinth_api,labyrinth_worker;
CREATE TRIGGER personal_workspaces_immutable BEFORE UPDATE OR DELETE ON labyrinth.personal_workspaces FOR EACH ROW EXECUTE FUNCTION labyrinth.reject_mutation();
CREATE FUNCTION labyrinth.onboard_personal(p_actor uuid,p_jobs integer,p_projects integer)
RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE tid uuid; replay boolean:=false; member_role text; jobs integer; projects integer;
BEGIN
    IF p_actor IS NULL OR p_jobs IS NULL OR p_jobs NOT BETWEEN 1 AND 1000 OR p_projects IS NULL OR p_projects NOT BETWEEN 1 AND 10000 THEN RAISE EXCEPTION 'invalid_onboarding_policy'; END IF;
    INSERT INTO labyrinth.users(id) VALUES(p_actor) ON CONFLICT DO NOTHING;
    PERFORM 1 FROM labyrinth.users WHERE id=p_actor AND NOT suspended FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_forbidden'; END IF;
    SELECT tenant_id INTO tid FROM labyrinth.personal_workspaces WHERE user_id=p_actor;
    IF FOUND THEN
        replay:=true;
        PERFORM labyrinth.require_tenant_access(p_actor,tid);
    ELSE
        tid:=gen_random_uuid();
        INSERT INTO labyrinth.tenants(id,max_active_jobs,max_projects) VALUES(tid,p_jobs,p_projects);
        INSERT INTO labyrinth.memberships(tenant_id,user_id,role) VALUES(tid,p_actor,'owner');
        INSERT INTO labyrinth.personal_workspaces(user_id,tenant_id) VALUES(p_actor,tid);
    END IF;
    SELECT max_active_jobs,max_projects INTO jobs,projects FROM labyrinth.tenants WHERE id=tid;
    SELECT role INTO member_role FROM labyrinth.memberships WHERE tenant_id=tid AND user_id=p_actor;
    RETURN jsonb_build_object('tenant_id',tid,'role',member_role,'max_active_jobs',jobs,'max_projects',projects,'replayed',replay);
END $$;
CREATE FUNCTION labyrinth.current_identity(p_actor uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE active boolean; tid uuid;
BEGIN
    SELECT NOT suspended INTO active FROM labyrinth.users WHERE id=p_actor FOR SHARE;
    IF active=false THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_forbidden'; END IF;
    SELECT p.tenant_id INTO tid FROM labyrinth.personal_workspaces p JOIN labyrinth.tenants t ON t.id=p.tenant_id
        JOIN labyrinth.memberships m ON m.tenant_id=t.id AND m.user_id=p_actor WHERE p.user_id=p_actor AND NOT t.suspended;
    RETURN jsonb_build_object('user_id',p_actor,'provisioned',coalesce(active,false),'personal_tenant_id',tid);
END $$;
REVOKE ALL ON FUNCTION labyrinth.onboard_personal(uuid,integer,integer),labyrinth.current_identity(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION labyrinth.onboard_personal(uuid,integer,integer),labyrinth.current_identity(uuid) TO labyrinth_api;
-- Persisted caps take precedence for provisioned personal tenants. Legacy tenant
-- caps continue to use the explicit deployment policy rather than a silent backfill.
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
    SELECT * INTO prior FROM labyrinth.project_creations WHERE tenant_id=p_tenant AND actor_id=p_actor AND idempotency_key=p_key;
    IF FOUND THEN
        IF prior.name<>p_name THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_idempotency_conflict'; END IF;
        pid:=prior.project_id; replay:=true;
    ELSE
        IF (SELECT count(*) FROM labyrinth.projects WHERE tenant_id=p_tenant)>=cap THEN
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
