-- Terminal timestamps start the safety window; pre-migration history waits afresh.
ALTER TABLE labyrinth.attempts ADD COLUMN ended_at timestamptz;
ALTER TABLE labyrinth.attempts ADD COLUMN cleanup_due_at timestamptz;
ALTER TABLE labyrinth.attempts ADD COLUMN cleanup_token uuid;
ALTER TABLE labyrinth.attempts ADD COLUMN cleanup_lease_until timestamptz;
ALTER TABLE labyrinth.attempts ADD COLUMN last_cleaned_at timestamptz;
UPDATE labyrinth.attempts SET ended_at=clock_timestamp(),cleanup_due_at=clock_timestamp()
    WHERE exit_classification IS NOT NULL;
CREATE FUNCTION labyrinth.guard_attempt_terminal() RETURNS trigger
LANGUAGE plpgsql SET search_path=pg_catalog AS $$
BEGIN
    IF OLD.exit_classification IS NOT NULL AND NEW.exit_classification IS DISTINCT FROM OLD.exit_classification THEN
        RAISE EXCEPTION 'terminal_attempt_immutable';
    END IF;
    IF OLD.exit_classification IS NULL AND NEW.exit_classification IS NOT NULL THEN
        NEW.ended_at:=clock_timestamp(); NEW.cleanup_due_at:=clock_timestamp();
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER attempts_terminal BEFORE UPDATE ON labyrinth.attempts FOR EACH ROW EXECUTE FUNCTION labyrinth.guard_attempt_terminal();
CREATE INDEX attempts_cleanup_due ON labyrinth.attempts(cleanup_due_at,ended_at)
    WHERE exit_classification IS NOT NULL AND exit_classification<>'succeeded';
-- Protect the gap between the DB reference recheck and external object deletion.
-- An irreversibly failed attempt can never gain new artifact references.
CREATE FUNCTION labyrinth.guard_artifact_attempt() RETURNS trigger
LANGUAGE plpgsql SET search_path=pg_catalog AS $$
DECLARE a labyrinth.attempts%ROWTYPE;
BEGIN
    SELECT * INTO a FROM labyrinth.attempts WHERE staged_prefix=left(NEW.object_key,length(NEW.object_key)-length(NEW.name)) FOR UPDATE;
    IF FOUND AND (a.exit_classification IS NOT NULL AND a.exit_classification<>'succeeded') THEN
        RAISE EXCEPTION 'terminal_attempt_cannot_publish';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER artifacts_attempt_guard BEFORE INSERT ON labyrinth.artifacts FOR EACH ROW EXECUTE FUNCTION labyrinth.guard_artifact_attempt();

CREATE FUNCTION labyrinth.claim_cleanup(p_window integer) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE a labyrinth.attempts%ROWTYPE; token uuid;
BEGIN
    IF p_window NOT BETWEEN 86400 AND 2592000 THEN RAISE EXCEPTION 'invalid_cleanup_window'; END IF;
    SELECT * INTO a FROM labyrinth.attempts x WHERE x.exit_classification IS NOT NULL AND x.exit_classification<>'succeeded'
        AND x.ended_at<=clock_timestamp()-make_interval(secs=>p_window)
        AND x.cleanup_due_at<=clock_timestamp()
        AND (x.cleanup_lease_until IS NULL OR x.cleanup_lease_until<=clock_timestamp())
        AND NOT EXISTS(SELECT FROM labyrinth.artifacts f WHERE starts_with(f.object_key,x.staged_prefix))
        ORDER BY x.cleanup_due_at,x.id LIMIT 1 FOR UPDATE OF x SKIP LOCKED;
    IF NOT FOUND THEN RETURN NULL; END IF;
    token:=gen_random_uuid();
    UPDATE labyrinth.attempts SET cleanup_token=token,cleanup_lease_until=clock_timestamp()+interval '60 seconds' WHERE fence=a.fence;
    RETURN jsonb_build_object('fence',a.fence,'token',token,'prefix',a.staged_prefix);
END $$;
CREATE FUNCTION labyrinth.check_cleanup(p_fence uuid,p_token uuid) RETURNS text
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
DECLARE prefix text;
BEGIN
    SELECT a.staged_prefix INTO prefix FROM labyrinth.attempts a WHERE a.fence=p_fence AND a.cleanup_token=p_token
        AND a.cleanup_lease_until>clock_timestamp() AND a.exit_classification IS NOT NULL AND a.exit_classification<>'succeeded'
        AND NOT EXISTS(SELECT FROM labyrinth.artifacts f WHERE starts_with(f.object_key,a.staged_prefix)) FOR UPDATE;
    RETURN prefix;
END $$;
CREATE FUNCTION labyrinth.finish_cleanup(p_fence uuid,p_token uuid,p_success boolean) RETURNS boolean
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
BEGIN
    IF labyrinth.check_cleanup(p_fence,p_token) IS NULL THEN RETURN false; END IF;
    UPDATE labyrinth.attempts SET last_cleaned_at=CASE WHEN p_success THEN clock_timestamp() ELSE last_cleaned_at END,
        cleanup_due_at=clock_timestamp()+CASE WHEN p_success THEN interval '24 hours' ELSE interval '60 seconds' END,
        cleanup_token=NULL,cleanup_lease_until=NULL WHERE fence=p_fence;
    RETURN true;
END $$;
REVOKE ALL ON FUNCTION labyrinth.guard_attempt_terminal(),labyrinth.guard_artifact_attempt(),labyrinth.claim_cleanup(integer),labyrinth.check_cleanup(uuid,uuid),labyrinth.finish_cleanup(uuid,uuid,boolean) FROM PUBLIC,labyrinth_api;
GRANT EXECUTE ON FUNCTION labyrinth.claim_cleanup(integer),labyrinth.check_cleanup(uuid,uuid),labyrinth.finish_cleanup(uuid,uuid,boolean) TO labyrinth_worker;
