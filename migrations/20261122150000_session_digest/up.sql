-- SHA-256 of the session token a server got at its start, to hand the same session out again
-- while the server boots (Wings syncs) or restarts soon after.
ALTER TABLE dev_caloptreyx_hytaleauth_servers ADD COLUMN session_digest VARCHAR(64);
