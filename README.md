# Hytale Auth

A [Calagopus Panel](https://calagopus.com) extension that makes the Hytale logins of a server
painless. A new Hytale server needs a Hytale account twice: once for the downloader that fetches
the game files, and once for the server itself so players can join. Both are device code logins
printed in the console; this extension turns them into a card with a button.

Package name: `dev.caloptreyx.hytaleauth` · Requires panel `>=1.2.10`

Made for servers on the Caloptreyx Hytale egg
([game-eggs `hytale/egg-hytale.yml`](https://github.com/Caloptreyx/game-eggs/tree/main/hytale),
image `ghcr.io/caloptreyx/games:hytale`), and works with any egg that runs `HytaleServer.jar`.

## Console helper

Above the console of every Hytale server (a start command running `HytaleServer.jar`, or an image
named `hytale`), the extension reads the console output and shows what needs doing:

- **Download login.** On the first start the Hytale downloader prints a link and a code. The card
  shows a **Log in with Hytale** button (the link with the code already filled in), the code with
  a copy button, and a countdown until the code expires. Once the download starts, the card says
  so and goes away.
- **Server login.** When the server runs without a login, the card says that players cannot join
  and offers **Authenticate server**, which sends `/auth login device`. The server's link and code
  then show the same way. After the login went through, the card sends
  `/auth persistence Encrypted` by itself, so the server keeps its login, encrypted in its files,
  across restarts. A login typed by hand in the console is picked up too; the card then offers
  **Keep login after restarts**.
- **Profiles.** When the Hytale account has more than one game profile, the card lists them; a
  click sends `/auth select <number>`.
- **Status.** While the server runs, a slim line shows whether it is signed in, as which profile,
  how (device login, saved login, linked account) and whether the login is saved. **Check status**
  sends `/auth status`.
- **Failures.** An expired code, a cancelled or failed login and tokens the server rejected are
  shown with a way to try again.

Only links to Hytale's own pages (`https://…hytale.com/…`) are ever shown as links.

### Permissions

The card uses the panel's own permissions; the extension adds none for the helper:

- Seeing the card needs `control.read-console` (it reads the console output).
- **Authenticate server**, the profile buttons, **Keep login after restarts** and **Check status**
  send console commands and need `control.console`. Without it the card tells the user to ask
  someone who has console access.
- **Restart** (shown when the downloader's code expired) needs `control.restart`.

## Development

Pure logic lives in `frontend/src/lib`; the console fixtures in `tests/fixtures` are recorded in
the formats the Hytale downloader and server print. From the repository:

```sh
node --test "tests/*.test.ts"
```

With the frontend linked into a panel checkout (`frontend/extensions/dev_caloptreyx_hytaleauth`):
`pnpm typecheck`, `pnpm exec oxlint extensions/dev_caloptreyx_hytaleauth` and
`pnpm exec oxfmt --check extensions/dev_caloptreyx_hytaleauth`. Every push runs the full check
(typecheck, lint, format, frontend build, these tests and `cargo test`) on Blacksmith through
`.github/workflows/check.yml`.

`python3 scripts/package.py` builds `dist/dev_caloptreyx_hytaleauth.c7s.zip` for installing by
hand; releases get the panel's own export from `.github/workflows/release.yml`.

## License

MIT
