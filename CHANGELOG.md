# Changelog

## 0.1.0

- Console helper for Hytale servers: the downloader's first-start login and the server's
  `/auth login device` login become a card above the console with a **Log in with Hytale** button,
  the code with a copy button and an expiry countdown, then a success or failure message.
- **Authenticate server** sends `/auth login device` and, once the login went through,
  `/auth persistence Encrypted`, so the login survives restarts.
- Game profile choice (`/auth select`), the server's login status and **Check status**
  (`/auth status`).
- Automatic login: a host account linked by an admin, and accounts users link for the servers they
  own (theirs wins), sign every start of a Hytale server in. The panel creates a game session when
  Wings starts the server and passes it as `HYTALE_SERVER_SESSION_TOKEN` and
  `HYTALE_SERVER_IDENTITY_TOKEN`; no egg or image change. Linking runs Hytale's device code flow in
  the panel, with profile choice, unlinking and linking again.
- Refresh and access tokens are stored encrypted with the panel's key, never sent to the browser or
  logged, and rotated one refresh at a time per account (row lock across panel instances). Idle
  logins are renewed weekly so they do not expire after 30 days.
- Errors (login expired, session limit reached, Hytale unreachable) are shown on the account, the
  server's settings card and the console card; the server starts without a login and the console
  helper takes over.
- Server permission `hytale-auth.manage` for the per-server switch; admin settings for the host
  account and for users' own accounts.
