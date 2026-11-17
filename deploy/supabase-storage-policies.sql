-- Supabase-specific operator bootstrap, AFTER application migration 0016.
-- Never expose the labyrinth schema via the Data API. Register UUIDs separately.
BEGIN;
DO $$ BEGIN
    IF NOT EXISTS(SELECT FROM storage.buckets WHERE id='engineering-packages' AND public=false) THEN RAISE EXCEPTION 'private engineering-packages bucket required'; END IF;
    IF EXISTS(SELECT FROM pg_policies WHERE schemaname='storage' AND tablename IN ('objects','buckets') AND (tablename,policyname) NOT IN (('buckets','labyrinth_service_bucket'),('objects','labyrinth_service_read'),('objects','labyrinth_service_insert'),('objects','labyrinth_service_delete'))) THEN RAISE EXCEPTION 'review existing Storage policies before bootstrap'; END IF;
END $$;
ALTER TABLE storage.objects ENABLE ROW LEVEL SECURITY;
ALTER TABLE storage.buckets ENABLE ROW LEVEL SECURITY;
GRANT USAGE ON SCHEMA storage TO authenticated;
GRANT SELECT ON storage.buckets TO authenticated;
GRANT SELECT,INSERT,DELETE ON storage.objects TO authenticated;
CREATE TABLE IF NOT EXISTS labyrinth.storage_principals (
    user_id uuid PRIMARY KEY,
    kind text NOT NULL CHECK(kind IN ('reader','worker')),
    enabled boolean NOT NULL DEFAULT true
);
ALTER TABLE labyrinth.storage_principals ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON labyrinth.storage_principals FROM PUBLIC,anon,authenticated,labyrinth_api,labyrinth_worker;
CREATE OR REPLACE FUNCTION labyrinth.audit_storage_principal_change() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE target uuid;
BEGIN
    IF TG_OP='DELETE' THEN target:=OLD.user_id; ELSE target:=NEW.user_id; END IF;
    INSERT INTO labyrinth.admin_audit_logs(actor_id,action,target_id,target_kind,target_reference,outcome,operator_principal)
        VALUES(NULL,'storage_principal_'||lower(TG_OP),target,'system',target::text,'allowed',session_user);
    IF TG_OP='DELETE' THEN RETURN OLD; END IF; RETURN NEW;
END $$;
REVOKE ALL ON FUNCTION labyrinth.audit_storage_principal_change() FROM PUBLIC,anon,authenticated,labyrinth_api,labyrinth_worker;
DROP TRIGGER IF EXISTS storage_principal_audit ON labyrinth.storage_principals;
CREATE TRIGGER storage_principal_audit AFTER INSERT OR UPDATE OR DELETE ON labyrinth.storage_principals FOR EACH ROW EXECUTE FUNCTION labyrinth.audit_storage_principal_change();
CREATE OR REPLACE FUNCTION labyrinth.storage_permitted(p_bucket text,p_name text,p_action text)
RETURNS boolean LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE principal text;
BEGIN
    IF p_bucket IS DISTINCT FROM 'engineering-packages' THEN RETURN false; END IF;
    SELECT kind INTO principal FROM labyrinth.storage_principals WHERE user_id=auth.uid() AND enabled;
    IF NOT FOUND THEN RETURN false; END IF;
    IF p_action='bucket' THEN RETURN true; END IF;
    IF p_action='read' AND EXISTS(
        SELECT FROM labyrinth.artifacts f JOIN labyrinth.projects p ON (p.tenant_id,p.id)=(f.tenant_id,f.project_id)
        JOIN labyrinth.revisions r ON (r.tenant_id,r.project_id,r.id)=(f.tenant_id,f.project_id,f.revision_id)
        JOIN labyrinth.jobs j ON (j.tenant_id,j.id)=(r.tenant_id,r.job_id)
        WHERE f.object_key=p_name AND p.deleted_at IS NULL AND j.state='succeeded') THEN RETURN true; END IF;
    IF principal<>'worker' THEN RETURN false; END IF;
    IF p_action IN ('read','write') AND EXISTS(
        SELECT FROM labyrinth.attempts a JOIN labyrinth.jobs j ON (j.tenant_id,j.id)=(a.tenant_id,a.job_id)
        JOIN labyrinth.projects p ON (p.tenant_id,p.id)=(a.tenant_id,a.project_id)
        WHERE p_name IN (SELECT a.staged_prefix||n FROM unnest(ARRAY['bom.json','catalog.json','decisions.json','firewall.json','ipam.json','manifest.json','options.json','provenance.json','requirements.json','routing.json','selection.json','topology.json','verification.json']) AS allowed(n))
        AND p.deleted_at IS NULL AND p.lifecycle_epoch=a.project_epoch AND j.project_epoch=p.lifecycle_epoch
        AND j.state='running' AND a.exit_classification IS NULL AND a.lease_expires_at>now()) THEN RETURN true; END IF;
    IF p_action IN ('read','delete') AND EXISTS(
        SELECT FROM labyrinth.attempts a JOIN labyrinth.projects p ON (p.tenant_id,p.id)=(a.tenant_id,a.project_id)
        WHERE p_name IN (SELECT a.staged_prefix||n FROM unnest(ARRAY['bom.json','catalog.json','decisions.json','firewall.json','ipam.json','manifest.json','options.json','provenance.json','requirements.json','routing.json','selection.json','topology.json','verification.json']) AS allowed(n))
        AND a.exit_classification IS NOT NULL AND (
            p.deleted_at<=now()-interval '168 hours' OR
            (a.exit_classification<>'succeeded' AND a.ended_at<=now()-interval '24 hours'
             AND NOT EXISTS(SELECT FROM labyrinth.artifacts f WHERE f.object_key LIKE a.staged_prefix||'%')))) THEN RETURN true; END IF;
    RETURN false;
END $$;
REVOKE ALL ON FUNCTION labyrinth.storage_permitted(text,text,text) FROM PUBLIC,anon,labyrinth_api,labyrinth_worker;
GRANT USAGE ON SCHEMA labyrinth TO authenticated;
GRANT EXECUTE ON FUNCTION labyrinth.storage_permitted(text,text,text) TO authenticated;
DROP POLICY IF EXISTS labyrinth_service_bucket ON storage.buckets;
CREATE POLICY labyrinth_service_bucket ON storage.buckets FOR SELECT TO authenticated USING(labyrinth.storage_permitted(id,'','bucket'));
DROP POLICY IF EXISTS labyrinth_service_read ON storage.objects;
CREATE POLICY labyrinth_service_read ON storage.objects FOR SELECT TO authenticated USING(labyrinth.storage_permitted(bucket_id,name,'read'));
DROP POLICY IF EXISTS labyrinth_service_insert ON storage.objects;
CREATE POLICY labyrinth_service_insert ON storage.objects FOR INSERT TO authenticated WITH CHECK(labyrinth.storage_permitted(bucket_id,name,'write'));
DROP POLICY IF EXISTS labyrinth_service_delete ON storage.objects;
CREATE POLICY labyrinth_service_delete ON storage.objects FOR DELETE TO authenticated USING(labyrinth.storage_permitted(bucket_id,name,'delete'));
-- Intentionally no UPDATE/upsert, public bucket or human-user policy.
COMMIT;
