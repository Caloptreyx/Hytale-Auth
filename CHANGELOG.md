# Changelog

## 0.1.0

- Console helper for Hytale servers: the downloader's first-start login and the server's
  `/auth login device` login become a card above the console with a **Log in with Hytale** button,
  the code with a copy button and an expiry countdown, then a success or failure message.
- **Authenticate server** sends `/auth login device` and, once the login went through,
  `/auth persistence Encrypted`, so the login survives restarts.
- Game profile choice (`/auth select`), the server's login status and **Check status**
  (`/auth status`).
