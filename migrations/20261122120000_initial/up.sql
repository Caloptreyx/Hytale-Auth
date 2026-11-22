-- Linked Hytale accounts: the host account (user_uuid NULL, linked by an admin) and the accounts
-- users linked for the servers they own. Tokens are encrypted with the panel's encryption key.
CREATE TABLE dev_caloptreyx_hytaleauth_accounts (
    id uuid NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
    user_uuid uuid REFERENCES users(uuid) ON DELETE CASCADE,
    -- the game profile servers run as, and every profile of the account
    profile_uuid uuid,
    profile_name VARCHAR(64),
    profiles JSONB NOT NULL DEFAULT '[]',
    refresh_token BYTEA NOT NULL,
    access_token BYTEA,
    access_expires TIMESTAMPTZ,
    -- linked, or relink once the refresh token died
    status VARCHAR(15) NOT NULL DEFAULT 'linked',
    error VARCHAR(31),
    error_message TEXT,
    error_at TIMESTAMPTZ,
    -- when the refresh token was last used (it lives 30 days from there)
    refreshed TIMESTAMPTZ NOT NULL DEFAULT now(),
    linked_by uuid REFERENCES users(uuid) ON DELETE SET NULL,
    created TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX dev_caloptreyx_hytaleauth_accounts_user_idx
    ON dev_caloptreyx_hytaleauth_accounts (user_uuid) WHERE user_uuid IS NOT NULL;
CREATE UNIQUE INDEX dev_caloptreyx_hytaleauth_accounts_host_idx
    ON dev_caloptreyx_hytaleauth_accounts ((user_uuid IS NULL)) WHERE user_uuid IS NULL;

-- Device code logins in progress, one per account (host or user). The device code is encrypted.
CREATE TABLE dev_caloptreyx_hytaleauth_links (
    id uuid NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
    user_uuid uuid REFERENCES users(uuid) ON DELETE CASCADE,
    started_by uuid REFERENCES users(uuid) ON DELETE CASCADE,
    device_code BYTEA NOT NULL,
    user_code VARCHAR(64) NOT NULL,
    verification_uri TEXT NOT NULL,
    verification_uri_complete TEXT,
    interval_seconds INTEGER NOT NULL,
    expires TIMESTAMPTZ NOT NULL,
    polled TIMESTAMPTZ,
    created TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX dev_caloptreyx_hytaleauth_links_user_idx
    ON dev_caloptreyx_hytaleauth_links (user_uuid) WHERE user_uuid IS NOT NULL;
CREATE UNIQUE INDEX dev_caloptreyx_hytaleauth_links_host_idx
    ON dev_caloptreyx_hytaleauth_links ((user_uuid IS NULL)) WHERE user_uuid IS NULL;

-- Per server: automatic login on or off, and how its last start went.
CREATE TABLE dev_caloptreyx_hytaleauth_servers (
    server_uuid uuid NOT NULL PRIMARY KEY REFERENCES servers(uuid) ON DELETE CASCADE,
    automatic BOOLEAN NOT NULL DEFAULT TRUE,
    account_id uuid REFERENCES dev_caloptreyx_hytaleauth_accounts(id) ON DELETE SET NULL,
    -- user or host
    source VARCHAR(7),
    profile_name VARCHAR(64),
    minted TIMESTAMPTZ,
    session_expires TIMESTAMPTZ,
    error VARCHAR(31),
    error_message TEXT,
    error_at TIMESTAMPTZ
);

CREATE INDEX dev_caloptreyx_hytaleauth_servers_account_idx
    ON dev_caloptreyx_hytaleauth_servers (account_id);
