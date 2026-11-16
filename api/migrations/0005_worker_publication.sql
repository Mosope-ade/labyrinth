-- Metadata publication is the only visibility point. Uploads happen beforehand.
CREATE FUNCTION labyrinth.publish_attempt(p_tenant uuid,p_job uuid,p_fence uuid,p_revision uuid,
    p_manifest text,p_files jsonb) RETURNS boolean
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE j labyrinth.jobs%ROWTYPE; a labyrinth.attempts%ROWTYPE; head uuid; f jsonb;
    pid uuid; names text[]; required text[]:=ARRAY['bom.json','catalog.json','decisions.json','firewall.json','ipam.json','manifest.json','options.json','provenance.json','requirements.json','routing.json','selection.json','topology.json','verification.json'];
BEGIN
    SELECT project_id INTO pid FROM labyrinth.jobs WHERE tenant_id=p_tenant AND id=p_job;
    IF NOT FOUND THEN RETURN false; END IF;
    PERFORM 1 FROM pgmq.q_labyrinth_generation_v1 q WHERE q.msg_id=(SELECT queue_message_id FROM labyrinth.attempts WHERE fence=p_fence) FOR UPDATE;
    -- Match admission lock order: project before job. No network I/O here.
    SELECT head_revision_id INTO head FROM labyrinth.projects WHERE tenant_id=p_tenant AND id=pid FOR UPDATE;
    SELECT * INTO j FROM labyrinth.jobs WHERE tenant_id=p_tenant AND id=p_job FOR UPDATE;
    SELECT * INTO a FROM labyrinth.attempts WHERE tenant_id=p_tenant AND job_id=p_job AND fence=p_fence
        AND exit_classification IS NULL AND lease_expires_at>clock_timestamp() FOR UPDATE;
    IF NOT FOUND OR j.state<>'running' THEN RETURN false; END IF;
    IF head IS DISTINCT FROM j.expected_head_revision_id THEN
        UPDATE labyrinth.attempts SET exit_classification='stale_head' WHERE id=a.id;
        UPDATE labyrinth.jobs SET state='failed' WHERE tenant_id=p_tenant AND id=p_job;
        PERFORM pgmq.delete('labyrinth_generation_v1',a.queue_message_id); RETURN false;
    END IF;
    IF jsonb_typeof(p_files)<>'array' OR jsonb_array_length(p_files)<>13 THEN RAISE EXCEPTION 'invalid_inventory'; END IF;
    SELECT array_agg(v->>'name' ORDER BY v->>'name') INTO names FROM jsonb_array_elements(p_files) v;
    IF names IS DISTINCT FROM required OR p_manifest !~ '^[0-9a-f]{64}$' THEN RAISE EXCEPTION 'invalid_inventory'; END IF;
    IF (SELECT sum((v->>'size_bytes')::bigint) FROM jsonb_array_elements(p_files) v)>33554432 THEN RAISE EXCEPTION 'package_too_large'; END IF;
    FOR f IN SELECT * FROM jsonb_array_elements(p_files) LOOP
        IF f->>'object_key' IS DISTINCT FROM a.staged_prefix||(f->>'name') OR
            (f->>'name'='manifest.json' AND f->>'sha256' IS DISTINCT FROM p_manifest) THEN RAISE EXCEPTION 'invalid_inventory'; END IF;
    END LOOP;
    INSERT INTO labyrinth.revisions(tenant_id,project_id,id,job_id,manifest_sha256,catalog_sha256)
        VALUES(p_tenant,j.project_id,p_revision,p_job,p_manifest,j.catalog_sha256);
    INSERT INTO labyrinth.artifacts(tenant_id,project_id,revision_id,name,object_key,sha256,size_bytes)
        SELECT p_tenant,j.project_id,p_revision,v->>'name',v->>'object_key',v->>'sha256',(v->>'size_bytes')::bigint
        FROM jsonb_array_elements(p_files) v;
    UPDATE labyrinth.projects SET head_revision_id=p_revision WHERE tenant_id=p_tenant AND id=j.project_id;
    UPDATE labyrinth.attempts SET exit_classification='succeeded' WHERE id=a.id;
    UPDATE labyrinth.jobs SET state='succeeded' WHERE tenant_id=p_tenant AND id=p_job;
    PERFORM pgmq.delete('labyrinth_generation_v1',a.queue_message_id);
    RETURN true;
END $$;
-- Context access requires possession of the current fence, not just a job UUID.
CREATE FUNCTION labyrinth.attempt_context(p_tenant uuid,p_job uuid,p_fence uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE result jsonb;
BEGIN
    SELECT jsonb_build_object('requirements',j.requirements,'provenance',j.provenance,'options',j.options,
        'catalog_yaml',c.yaml,'catalog_sha256',j.catalog_sha256,'base_revision_id',j.base_revision_id,
        'base_manifest_sha256',j.base_manifest_sha256,'staged_prefix',a.staged_prefix,
        'base_files',coalesce((SELECT jsonb_agg(jsonb_build_object('name',f.name,'object_key',f.object_key,'sha256',f.sha256,'size_bytes',f.size_bytes))
        FROM labyrinth.artifacts f WHERE f.tenant_id=j.tenant_id AND f.project_id=j.project_id AND f.revision_id=j.base_revision_id),'[]'::jsonb))
    INTO result FROM labyrinth.jobs j JOIN labyrinth.catalogs c ON c.sha256=j.catalog_sha256
        JOIN labyrinth.attempts a ON a.tenant_id=j.tenant_id AND a.job_id=j.id
        WHERE j.tenant_id=p_tenant AND j.id=p_job AND j.state='running' AND a.fence=p_fence
        AND a.exit_classification IS NULL AND a.lease_expires_at>clock_timestamp();
    RETURN result;
END $$;
REVOKE ALL ON FUNCTION labyrinth.publish_attempt(uuid,uuid,uuid,uuid,text,jsonb),labyrinth.attempt_context(uuid,uuid,uuid) FROM PUBLIC,labyrinth_api;
GRANT EXECUTE ON FUNCTION labyrinth.publish_attempt(uuid,uuid,uuid,uuid,text,jsonb),labyrinth.attempt_context(uuid,uuid,uuid) TO labyrinth_worker;
