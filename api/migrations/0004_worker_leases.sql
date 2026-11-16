-- Separate trusted worker boundary; never grant this role to API/client logins.
DO $$ BEGIN
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname='labyrinth_worker') THEN
        CREATE ROLE labyrinth_worker NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOBYPASSRLS;
    END IF;
    IF EXISTS (SELECT FROM pg_roles WHERE rolname='labyrinth_worker' AND
        (rolsuper OR rolcreatedb OR rolcreaterole OR rolbypassrls OR rolcanlogin)) THEN
        RAISE EXCEPTION 'labyrinth_worker must be an unprivileged NOLOGIN group role';
    END IF;
END $$;
GRANT USAGE ON SCHEMA labyrinth TO labyrinth_worker;
ALTER TABLE labyrinth.attempts ADD COLUMN queue_message_id bigint;
ALTER TABLE labyrinth.attempts ADD COLUMN retry_at timestamptz;

CREATE FUNCTION labyrinth.claim_job(p_worker text, p_lease integer, p_budget integer,
    p_delay_first integer, p_delay_second integer) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE m record; j labyrinth.jobs%ROWTYPE; a labyrinth.attempts%ROWTYPE;
    n integer; aid uuid; token uuid; prefix text; delay integer;
BEGIN
    IF length(p_worker) NOT BETWEEN 1 AND 200 OR p_lease NOT BETWEEN 10 AND 300
        OR p_budget NOT BETWEEN 1 AND 3 OR p_delay_first NOT BETWEEN 1 AND 3600
        OR p_delay_second NOT BETWEEN p_delay_first AND 3600 THEN
        RAISE EXCEPTION 'invalid_worker_policy';
    END IF;
    SELECT * INTO m FROM pgmq.read('labyrinth_generation_v1',p_lease,1);
    IF NOT FOUND THEN RETURN NULL; END IF;
    SELECT * INTO j FROM labyrinth.jobs WHERE tenant_id=(m.message->>'tenant_id')::uuid
        AND id=(m.message->>'job_id')::uuid FOR UPDATE;
    IF NOT FOUND OR j.state IN ('succeeded','failed','cancelled') THEN
        PERFORM pgmq.delete('labyrinth_generation_v1',m.msg_id); RETURN NULL;
    END IF;
    SELECT * INTO a FROM labyrinth.attempts WHERE tenant_id=j.tenant_id AND job_id=j.id
        ORDER BY attempt_number DESC LIMIT 1;
    IF j.state='cancel_requested' THEN
        UPDATE labyrinth.attempts SET exit_classification='cancelled' WHERE id=a.id AND exit_classification IS NULL;
        UPDATE labyrinth.jobs SET state='cancelled' WHERE tenant_id=j.tenant_id AND id=j.id;
        PERFORM pgmq.delete('labyrinth_generation_v1',m.msg_id); RETURN NULL;
    END IF;
    IF j.state='running' THEN
        IF a.lease_expires_at > clock_timestamp() THEN
            PERFORM pgmq.set_vt('labyrinth_generation_v1',m.msg_id,p_lease); RETURN NULL;
        END IF;
        UPDATE labyrinth.attempts SET exit_classification='lease_expired',
            retry_at=clock_timestamp()+make_interval(secs=>CASE WHEN a.attempt_number=1 THEN p_delay_first ELSE p_delay_second END)
            WHERE id=a.id;
        UPDATE labyrinth.jobs SET state=CASE WHEN a.attempt_number>=p_budget THEN 'failed' ELSE 'retry_wait' END
            WHERE tenant_id=j.tenant_id AND id=j.id;
        IF a.attempt_number>=p_budget THEN PERFORM pgmq.delete('labyrinth_generation_v1',m.msg_id);
        ELSE PERFORM pgmq.set_vt('labyrinth_generation_v1',m.msg_id,CASE WHEN a.attempt_number=1 THEN p_delay_first ELSE p_delay_second END);
        END IF;
        RETURN NULL;
    END IF;
    IF j.state='retry_wait' THEN
        IF a.retry_at > clock_timestamp() THEN
            delay:=greatest(1,ceil(extract(epoch FROM a.retry_at-clock_timestamp()))::integer);
            PERFORM pgmq.set_vt('labyrinth_generation_v1',m.msg_id,delay); RETURN NULL;
        END IF;
        UPDATE labyrinth.jobs SET state='queued' WHERE tenant_id=j.tenant_id AND id=j.id;
    END IF;
    n:=coalesce(a.attempt_number,0)+1;
    IF n>p_budget THEN
        -- A reduced deployment budget cannot silently grant more attempts.
        UPDATE labyrinth.jobs SET state='running' WHERE tenant_id=j.tenant_id AND id=j.id;
        UPDATE labyrinth.jobs SET state='failed' WHERE tenant_id=j.tenant_id AND id=j.id;
        PERFORM pgmq.delete('labyrinth_generation_v1',m.msg_id); RETURN NULL;
    END IF;
    aid:=gen_random_uuid(); token:=gen_random_uuid();
    prefix:='attempts/'||j.tenant_id||'/'||j.id||'/'||aid||'/';
    UPDATE labyrinth.jobs SET state='running' WHERE tenant_id=j.tenant_id AND id=j.id;
    INSERT INTO labyrinth.attempts(tenant_id,project_id,job_id,id,attempt_number,fence,worker_id,lease_expires_at,heartbeat_at,staged_prefix,queue_message_id)
        VALUES(j.tenant_id,j.project_id,j.id,aid,n,token,p_worker,clock_timestamp()+make_interval(secs=>p_lease),clock_timestamp(),prefix,m.msg_id);
    RETURN jsonb_build_object('tenant_id',j.tenant_id,'job_id',j.id,'fence',token);

END $$;

CREATE FUNCTION labyrinth.heartbeat_attempt(p_tenant uuid,p_job uuid,p_fence uuid,p_lease integer) RETURNS boolean
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE a labyrinth.attempts%ROWTYPE;
BEGIN
    IF p_lease NOT BETWEEN 10 AND 300 THEN RAISE EXCEPTION 'invalid_worker_policy'; END IF;
    PERFORM 1 FROM pgmq.q_labyrinth_generation_v1 q WHERE q.msg_id=(SELECT queue_message_id FROM labyrinth.attempts WHERE fence=p_fence) FOR UPDATE;
    PERFORM 1 FROM labyrinth.jobs WHERE tenant_id=p_tenant AND id=p_job AND state='running' FOR UPDATE;
    IF NOT FOUND THEN RETURN false; END IF;
    SELECT * INTO a FROM labyrinth.attempts WHERE tenant_id=p_tenant AND job_id=p_job AND fence=p_fence
        AND exit_classification IS NULL AND lease_expires_at>clock_timestamp() FOR UPDATE;
    IF NOT FOUND THEN RETURN false; END IF;
    UPDATE labyrinth.attempts SET heartbeat_at=clock_timestamp(),lease_expires_at=clock_timestamp()+make_interval(secs=>p_lease) WHERE id=a.id;
    PERFORM pgmq.set_vt('labyrinth_generation_v1',a.queue_message_id,p_lease);
    RETURN true;
END $$;

CREATE FUNCTION labyrinth.finish_attempt(p_tenant uuid,p_job uuid,p_fence uuid,p_transient boolean,
    p_budget integer,p_delay_first integer,p_delay_second integer) RETURNS boolean
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE a labyrinth.attempts%ROWTYPE; s text; delay integer;
BEGIN
    IF p_budget NOT BETWEEN 1 AND 3 OR p_delay_first NOT BETWEEN 1 AND 3600 OR p_delay_second NOT BETWEEN p_delay_first AND 3600 THEN RAISE EXCEPTION 'invalid_worker_policy'; END IF;
    PERFORM 1 FROM pgmq.q_labyrinth_generation_v1 q WHERE q.msg_id=(SELECT queue_message_id FROM labyrinth.attempts WHERE fence=p_fence) FOR UPDATE;
    SELECT state INTO s FROM labyrinth.jobs WHERE tenant_id=p_tenant AND id=p_job FOR UPDATE;
    IF s NOT IN ('running','cancel_requested') THEN RETURN false; END IF;
    SELECT * INTO a FROM labyrinth.attempts WHERE tenant_id=p_tenant AND job_id=p_job AND fence=p_fence
        AND exit_classification IS NULL AND lease_expires_at>clock_timestamp() FOR UPDATE;
    IF NOT FOUND THEN RETURN false; END IF;
    delay:=CASE WHEN a.attempt_number=1 THEN p_delay_first ELSE p_delay_second END;
    UPDATE labyrinth.attempts SET exit_classification=CASE WHEN s='cancel_requested' THEN 'cancelled' WHEN p_transient THEN 'transient_failure' ELSE 'deterministic_failure' END,
        retry_at=CASE WHEN p_transient THEN clock_timestamp()+make_interval(secs=>delay) END WHERE id=a.id;
    UPDATE labyrinth.jobs SET state=CASE WHEN s='cancel_requested' THEN 'cancelled'
        WHEN p_transient AND a.attempt_number<p_budget THEN 'retry_wait' ELSE 'failed' END WHERE tenant_id=p_tenant AND id=p_job;
    IF s<>'cancel_requested' AND p_transient AND a.attempt_number<p_budget THEN
        PERFORM pgmq.set_vt('labyrinth_generation_v1',a.queue_message_id,delay);
    ELSE PERFORM pgmq.delete('labyrinth_generation_v1',a.queue_message_id); END IF;
    RETURN true;
END $$;
REVOKE ALL ON FUNCTION labyrinth.claim_job(text,integer,integer,integer,integer),
    labyrinth.heartbeat_attempt(uuid,uuid,uuid,integer),labyrinth.finish_attempt(uuid,uuid,uuid,boolean,integer,integer,integer) FROM PUBLIC,labyrinth_api;
GRANT EXECUTE ON FUNCTION labyrinth.claim_job(text,integer,integer,integer,integer),
    labyrinth.heartbeat_attempt(uuid,uuid,uuid,integer),labyrinth.finish_attempt(uuid,uuid,uuid,boolean,integer,integer,integer) TO labyrinth_worker;
