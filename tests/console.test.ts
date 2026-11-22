import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { describe, test } from 'node:test';
import {
  cleanLine,
  type ConsoleAuthState,
  formatCountdown,
  initialState,
  isHytaleServer,
  isHytaleUrl,
  loginUrl,
  reduce,
  reduceAll,
  secondsLeft,
  view,
} from '../frontend/src/lib/console.ts';

const fixture = (name: string): string[] =>
  readFileSync(new URL(`./fixtures/${name}`, import.meta.url), 'utf8').split('\n');

const NOW = 1_800_000_000_000;

function run(...names: string[]): ConsoleAuthState {
  return names.reduce((state, name) => reduceAll(state, fixture(name), NOW), initialState());
}

describe('the downloader prompt', () => {
  test('a waiting login shows the link with the code, the code and a countdown', () => {
    const state = run('downloader-waiting.log');
    const prompt = state.download!;
    assert.equal(prompt.status, 'waiting');
    assert.equal(prompt.code, 'Xk4mPq2T');
    assert.equal(prompt.completeUrl, 'https://oauth.accounts.hytale.com/oauth2/device/verify?user_code=Xk4mPq2T');
    assert.equal(prompt.url, 'https://oauth.accounts.hytale.com/oauth2/device/verify');
    assert.equal(loginUrl(prompt), prompt.completeUrl);
    assert.equal(prompt.expiresIn, 600);
    assert.equal(secondsLeft(prompt, NOW + 90_500), 510);
    assert.deepEqual(view(state, NOW, false), { kind: 'prompt', prompt, expired: false });
    assert.deepEqual(view(state, NOW + 601_000, false), { kind: 'prompt', prompt, expired: true });
  });

  test('the version line after the prompt means the login went through', () => {
    const state = run('downloader-first-start.log');
    assert.equal(state.download!.status, 'success');
    assert.equal(state.download!.endedAt, NOW);
    assert.equal(view(state, NOW + 1000, false).kind, 'ended');
    assert.equal(view(state, NOW + 61_000, false).kind, 'none');
  });

  test('an expired code is replaced by the next prompt', () => {
    const state = run('downloader-expired.log');
    assert.equal(state.download!.status, 'waiting');
    assert.equal(state.download!.code, 'Rb7wZn3Q');
    assert.match(state.download!.completeUrl!, /user_code=Rb7wZn3Q$/);
  });

  test('an expired code alone ends the prompt as failed', () => {
    const lines = fixture('downloader-expired.log').slice(0, 7);
    const state = reduceAll(initialState(), lines, NOW);
    assert.equal(state.download!.status, 'failed');
  });
});

describe('the server prompt', () => {
  test('`/auth login device` shows a prompt with the printed expiry', () => {
    const lines = fixture('server-device-login.log');
    const until = lines.findIndex((line) => line.includes('Waiting for authorization'));
    const state = reduceAll(initialState(), lines.slice(0, until + 1), NOW);
    assert.equal(state.auth.status, 'missing');
    const prompt = state.server!;
    assert.equal(prompt.source, 'server');
    assert.equal(prompt.code, 'JuXnmQxD');
    assert.equal(prompt.url, 'https://oauth.accounts.hytale.com/oauth2/device/verify');
    assert.equal(prompt.completeUrl, 'https://oauth.accounts.hytale.com/oauth2/device/verify?user_code=JuXnmQxD');
    assert.equal(prompt.expiresIn, 600);
    assert.equal(view(state, NOW, true).kind, 'prompt');
  });

  test('success ends the prompt and the memory-only warning asks for persistence', () => {
    const state = run('server-device-login.log');
    assert.equal(state.server!.status, 'success');
    assert.equal(state.auth.status, 'authenticated');
    assert.equal(state.auth.mode, 'OAUTH_DEVICE');
    assert.equal(state.auth.profile, 'TinyTaru');
    assert.equal(state.auth.storage, 'Memory');
    assert.equal(view(state, NOW, true).kind, 'ended');
    assert.equal(view(state, NOW + 120_000, true).kind, 'memoryOnly');
  });

  test('`/auth persistence Encrypted` settles it', () => {
    const state = run('server-device-login.log', 'server-persistence.log');
    assert.equal(state.auth.storage, 'Encrypted');
    assert.equal(view(state, NOW + 120_000, true).kind, 'authenticated');
  });

  test('an expired device code fails the prompt', () => {
    const state = run('server-device-failed.log');
    assert.equal(state.server!.status, 'failed');
    assert.equal(view(state, NOW, true).kind, 'ended');
  });

  test('several game profiles wait for `/auth select` and list each profile once', () => {
    const state = run('server-device-login.log', 'server-profile-select.log');
    assert.equal(state.auth.profileChoice, true);
    assert.deepEqual(state.auth.profiles, [
      { index: 1, name: 'Builder' },
      { index: 2, name: 'Explorer' },
    ]);
    assert.equal(view(state, NOW, true).kind, 'profileChoice');
  });

  test('a stopped server shows no status card', () => {
    const state = run('server-device-login.log', 'server-persistence.log');
    assert.equal(view(state, NOW + 120_000, false).kind, 'none');
  });
});

describe('the authentication status', () => {
  test('tokens passed by the panel count as authenticated, also with memory storage', () => {
    const state = run('server-device-login.log', 'server-external-session.log');
    assert.equal(state.server, null, 'a new run forgets the previous prompt');
    assert.equal(state.auth.status, 'authenticated');
    assert.equal(state.auth.mode, 'EXTERNAL_SESSION');
    assert.equal(state.auth.profile, 'HostAccount');
    assert.equal(state.auth.storage, 'Memory');
    assert.equal(view(state, NOW, true).kind, 'authenticated');
  });

  test('stored credentials restore the login after the "no tokens" notice', () => {
    const state = run('server-stored-credentials.log');
    assert.equal(state.auth.status, 'authenticated');
    assert.equal(state.auth.mode, 'OAUTH_STORE');
    assert.equal(state.auth.storage, 'Encrypted');
    assert.equal(state.auth.profile, 'TinyTaru');
  });

  test('`/auth status` is read', () => {
    const state = run('server-device-login.log', 'server-auth-status.log');
    assert.equal(state.auth.mode, 'OAuth Store');
    assert.equal(state.auth.profile, 'TinyTaru');
  });

  test('rejected tokens are a failure', () => {
    const state = run('server-token-rejected.log');
    assert.equal(state.auth.status, 'failed');
    assert.equal(view(state, NOW, true).kind, 'failed');
  });

  test('a fresh server without login is unauthenticated once it runs', () => {
    const lines = fixture('server-device-login.log');
    const booted = lines.findIndex((line) => line.includes('Hytale Server Booted'));
    const state = reduceAll(initialState(), lines.slice(0, booted + 2), NOW);
    assert.equal(view(state, NOW, true).kind, 'unauthenticated');
  });

  test('offline mode needs no login', () => {
    const state = reduce(
      initialState(),
      '[2026/03/04 04:59:54   INFO]           [HytaleServer] Authentication mode: OFFLINE',
      NOW,
    );
    assert.equal(state.auth.status, 'offline');
    assert.equal(view(state, NOW, true).kind, 'none');
  });
});

describe('line handling', () => {
  test('colours, carriage returns and the log prefix are removed', () => {
    const [line] = fixture('ansi-success.log');
    assert.equal(cleanLine(line), 'Authentication successful! Mode: OAUTH_DEVICE');
    assert.equal(reduce(initialState(), line, NOW).auth.mode, 'OAUTH_DEVICE');
    assert.equal(cleanLine('Downloading... 10%\rDownloading... 55%\r'), 'Downloading... 55%');
  });

  test('unrelated lines keep the same state object', () => {
    const state = initialState();
    assert.equal(reduce(state, '[2026/03/04 05:00:03   INFO] [World|default] Player joined', NOW), state);
    assert.equal(reduce(state, '', NOW), state);
  });

  test('only Hytale login pages become links', () => {
    assert.ok(isHytaleUrl('https://oauth.accounts.hytale.com/oauth2/device/verify?user_code=AB12'));
    assert.ok(isHytaleUrl('https://accounts.hytale.com/device'));
    assert.ok(!isHytaleUrl('https://hytale.com.evil.example/device'));
    assert.ok(!isHytaleUrl('http://accounts.hytale.com/device'));
    assert.ok(!isHytaleUrl('https://evil.example/?u=hytale.com/'));
    const forged = reduceAll(
      initialState(),
      [
        'Please visit the following URL to authenticate:',
        'https://evil.example/device?user_code=AAAA1111',
        'Authorization code: AAAA1111',
      ],
      NOW,
    );
    assert.equal(forged.download!.completeUrl, null);
    assert.equal(forged.download!.code, 'AAAA1111');
  });

  test('countdowns', () => {
    assert.equal(formatCountdown(600), '10:00');
    assert.equal(formatCountdown(61), '1:01');
    assert.equal(formatCountdown(0), '0:00');
  });

  test('Hytale servers are recognised by start command or image', () => {
    const java = 'java -Xms128M -jar Server/HytaleServer.jar --assets Assets.zip';
    assert.ok(isHytaleServer({ startup: java, image: 'ghcr.io/caloptreyx/java:25' }));
    assert.ok(isHytaleServer({ startup: './start.sh', image: 'ghcr.io/caloptreyx/games:hytale' }));
    assert.ok(!isHytaleServer({ startup: 'java -jar server.jar', image: 'ghcr.io/ptero-eggs/yolks:java_21' }));
    assert.ok(!isHytaleServer({ startup: 'java -jar server.jar', image: 'example/hytalesque:1' }));
  });
});
