DO $$ BEGIN
    IF NOT EXISTS(SELECT FROM pg_roles WHERE rolname='labyrinth_maintenance') THEN
        CREATE ROLE labyrinth_maintenance NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOBYPASSRLS;
    END IF;
    IF EXISTS(SELECT FROM pg_roles WHERE rolname='labyrinth_maintenance' AND (rolsuper OR rolcreatedb OR rolcreaterole OR rolbypassrls OR rolcanlogin)) THEN RAISE EXCEPTION 'labyrinth_maintenance must be an unprivileged NOLOGIN group role'; END IF;
END $$;
GRANT USAGE ON SCHEMA labyrinth TO labyrinth_maintenance;
ALTER TABLE labyrinth.admin_audit_logs ALTER COLUMN actor_id DROP NOT NULL;
ALTER TABLE labyrinth.admin_audit_logs ADD COLUMN operator_principal text CHECK(length(operator_principal) BETWEEN 1 AND 128);
CREATE FUNCTION labyrinth.audit_operator_role_change() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
BEGIN
    INSERT INTO labyrinth.admin_audit_logs(actor_id,action,target_id,target_kind,target_reference,outcome,operator_principal)
        VALUES(NULL,CASE WHEN TG_OP='INSERT' THEN 'admin_role_grant' ELSE 'admin_role_revoke' END,
            CASE WHEN TG_OP='INSERT' THEN NEW.user_id ELSE OLD.user_id END,'user',
            CASE WHEN TG_OP='INSERT' THEN NEW.user_id::text ELSE OLD.user_id::text END,'allowed',session_user);
    RETURN NULL;
END $$;
CREATE TRIGGER admin_role_audit AFTER INSERT OR DELETE ON labyrinth.admin_roles FOR EACH ROW EXECUTE FUNCTION labyrinth.audit_operator_role_change();
CREATE TRIGGER admin_role_no_update BEFORE UPDATE ON labyrinth.admin_roles FOR EACH ROW EXECUTE FUNCTION labyrinth.reject_mutation();
CREATE FUNCTION labyrinth.guard_admin_audit() RETURNS trigger
LANGUAGE plpgsql SET search_path=pg_catalog AS $$
BEGIN
    IF TG_OP='DELETE' AND OLD.occurred_at<clock_timestamp()-interval '365 days'
        AND current_user=(SELECT pg_get_userbyid(relowner) FROM pg_class WHERE oid=TG_RELID) THEN RETURN OLD; END IF;
    RAISE EXCEPTION USING ERRCODE='55000',MESSAGE='immutable_audit_record';
END $$;
DROP TRIGGER audit_immutable ON labyrinth.admin_audit_logs;
CREATE TRIGGER audit_immutable BEFORE UPDATE OR DELETE ON labyrinth.admin_audit_logs FOR EACH ROW EXECUTE FUNCTION labyrinth.guard_admin_audit();
CREATE FUNCTION labyrinth.prune_admin_audit(p_limit integer) RETURNS bigint
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE removed bigint;
BEGIN
    IF p_limit IS NULL OR p_limit NOT BETWEEN 1 AND 1000 THEN RAISE EXCEPTION 'invalid_retention_batch'; END IF;
    WITH candidates AS (SELECT id FROM labyrinth.admin_audit_logs WHERE occurred_at<clock_timestamp()-interval '365 days' ORDER BY id LIMIT p_limit FOR UPDATE SKIP LOCKED),
        deleted AS (DELETE FROM labyrinth.admin_audit_logs a USING candidates c WHERE a.id=c.id RETURNING a.id)
    SELECT count(*) INTO removed FROM deleted;
    INSERT INTO labyrinth.admin_audit_logs(actor_id,action,target_id,target_kind,target_reference,outcome,operator_principal)
        VALUES(NULL,'audit_retention_prune',NULL,'system','audit','allowed',session_user);
    RETURN removed;
END $$;
REVOKE ALL ON FUNCTION labyrinth.audit_operator_role_change(),labyrinth.guard_admin_audit(),labyrinth.prune_admin_audit(integer) FROM PUBLIC,labyrinth_api,labyrinth_worker;
GRANT EXECUTE ON FUNCTION labyrinth.prune_admin_audit(integer) TO labyrinth_maintenance;
CREATE OR REPLACE FUNCTION labyrinth.admin_audit(p_actor uuid,p_before bigint,p_limit integer) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE result jsonb;
BEGIN
    IF NOT labyrinth.admin_access(p_actor) THEN RAISE EXCEPTION USING ERRCODE='P0001',MESSAGE='admission_forbidden'; END IF;
    IF p_limit IS NULL OR p_limit NOT BETWEEN 1 AND 100 OR p_before<=0 THEN RAISE EXCEPTION 'invalid_page_limit'; END IF;
    SELECT coalesce(jsonb_agg(v ORDER BY id DESC),'[]'::jsonb) INTO result FROM (
        SELECT id,jsonb_build_object('id',id::text,'actor_id',actor_id,'operator_principal',operator_principal,'action',action,'target_kind',target_kind,
            'target_reference',coalesce(target_reference,target_id::text),'outcome',outcome,
            'occurred_at',to_char(occurred_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"')) v
        FROM labyrinth.admin_audit_logs WHERE p_before IS NULL OR id<p_before ORDER BY id DESC LIMIT p_limit+1) q;
    RETURN result;
END $$;

CREATE FUNCTION labyrinth.guard_admin_role_grant() RETURNS trigger
LANGUAGE plpgsql SET search_path=pg_catalog AS $$
BEGIN
    PERFORM 1 FROM labyrinth.users WHERE id=NEW.user_id AND NOT suspended FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION 'admin_grant_requires_active_user'; END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER admin_role_active_user BEFORE INSERT ON labyrinth.admin_roles FOR EACH ROW EXECUTE FUNCTION labyrinth.guard_admin_role_grant();
REVOKE ALL ON FUNCTION labyrinth.guard_admin_role_grant() FROM PUBLIC,labyrinth_api,labyrinth_worker,labyrinth_maintenance;
