// Reads the Hytale logins out of console output: the downloader's device code prompt on the first
// start, the server's `/auth login device` prompt and what the server says about its
// authentication. Pure functions only, so they run under `node --test` too.
//
// The downloader prints (hytale-downloader 2026.x):
//   Please visit the following URL to authenticate:
//   https://oauth.accounts.hytale.com/oauth2/device/verify?user_code=Xk4mPq2T
//   Or visit the following URL and enter the code:
//   https://oauth.accounts.hytale.com/oauth2/device/verify
//   Authorization code: Xk4mPq2T
// The server logs the same through its logger, with a `[date time LEVEL] [Logger]` prefix:
//   DEVICE AUTHORIZATION / Visit: … / Enter code: … / Or visit: … /
//   Waiting for authorization (expires in 600 seconds)...

export type Source = 'downloader' | 'server';

export type PromptStatus = 'waiting' | 'success' | 'failed' | 'cancelled';

export interface Prompt {
  source: Source;
  /** The page where the code is typed in. */
  url: string | null;
  /** The page with the code filled in (`verification_uri_complete`). */
  completeUrl: string | null;
  code: string | null;
  /** Seconds the code is valid, from when the panel first saw the prompt. */
  expiresIn: number;
  seenAt: number;
  status: PromptStatus;
  /** When the prompt succeeded, failed or was cancelled. */
  endedAt: number | null;
}

export type AuthStatus = 'unknown' | 'missing' | 'authenticated' | 'failed' | 'offline';

export interface AuthState {
  status: AuthStatus;
  /** The server's auth mode: OAUTH_DEVICE, OAUTH_STORE, EXTERNAL_SESSION, … */
  mode: string | null;
  profile: string | null;
  /** Credential storage: Memory or Encrypted. */
  storage: string | null;
  /** The account has more than one game profile and waits for `/auth select`. */
  profileChoice: boolean;
  /** The profiles listed for `/auth select <number>`. */
  profiles: { index: number; name: string }[];
}

export interface ConsoleAuthState {
  download: Prompt | null;
  server: Prompt | null;
  auth: AuthState;
  /** Prompt lines arrive one by one; which one comes next. */
  expecting: 'downloaderUrl' | 'downloaderPlainUrl' | null;
}

/** Device code lifetime the downloader does not print: what Hytale's OAuth server answered in 2026. */
export const DOWNLOADER_CODE_SECONDS = 600;
/** Fallback when the server prompt's expiry line was missed. */
export const SERVER_CODE_SECONDS = 600;

const UNKNOWN_AUTH: AuthState = {
  status: 'unknown',
  mode: null,
  profile: null,
  storage: null,
  profileChoice: false,
  profiles: [],
};

export function initialState(): ConsoleAuthState {
  return { download: null, server: null, auth: UNKNOWN_AUTH, expecting: null };
}

// eslint-disable-next-line no-control-regex
const ANSI = /\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b[@-Z\\-_]/g;
const LOG_PREFIX = /^\[\d{4}\/\d{2}\/\d{2} \d{2}:\d{2}:\d{2}\s+[A-Z]+\]\s+(?:\[[^\]]*\]\s*)?/;
/** Only links to Hytale's own login pages are ever shown as links. */
const HYTALE_URL = /^https:\/\/(?:[a-z0-9-]+\.)*hytale\.com\/[^\s"'<>]*$/i;
const CODE = /^[A-Za-z0-9-]{4,32}$/;
const VERSION_LINE = /^\d{4}\.\d{2}\.\d{2}-[0-9a-f]{6,}$/;

/** Removes colours, carriage returns and the server's log prefix. */
export function cleanLine(raw: string): string {
  const noAnsi = raw.replace(ANSI, '');
  const lastSegment = noAnsi.split('\r').filter((part) => part.trim() !== '');
  const line = (lastSegment.length ? lastSegment[lastSegment.length - 1] : '').trim();
  return line.replace(LOG_PREFIX, '').trim();
}

export function isHytaleUrl(value: string): boolean {
  return HYTALE_URL.test(value);
}

function newPrompt(source: Source, now: number): Prompt {
  return {
    source,
    url: null,
    completeUrl: null,
    code: null,
    expiresIn: source === 'downloader' ? DOWNLOADER_CODE_SECONDS : SERVER_CODE_SECONDS,
    seenAt: now,
    status: 'waiting',
    endedAt: null,
  };
}

function setUrl(prompt: Prompt, url: string): Prompt {
  if (!isHytaleUrl(url)) return prompt;
  return url.includes('user_code=') ? { ...prompt, completeUrl: url } : { ...prompt, url };
}

function end(prompt: Prompt | null, status: PromptStatus, now: number): Prompt | null {
  if (!prompt || prompt.status !== 'waiting') return prompt;
  return { ...prompt, status, endedAt: now };
}

function authenticated(state: ConsoleAuthState, mode: string | null, now: number): ConsoleAuthState {
  return {
    ...state,
    server: end(state.server, 'success', now),
    auth: {
      ...state.auth,
      status: 'authenticated',
      mode: mode ?? state.auth.mode,
      profileChoice: false,
      profiles: [],
    },
  };
}

function storage(state: ConsoleAuthState, value: string): ConsoleAuthState {
  const name = /encrypted/i.test(value) ? 'Encrypted' : /memory/i.test(value) ? 'Memory' : value;
  return { ...state, auth: { ...state.auth, storage: name } };
}

/** Folds one console line into the state. Returns the same object when nothing changed. */
export function reduce(state: ConsoleAuthState, raw: string, now: number): ConsoleAuthState {
  const line = cleanLine(raw);
  if (!line) return state;
  let match: RegExpMatchArray | null;

  // the downloader
  if (line === 'Please visit the following URL to authenticate:') {
    return { ...state, download: newPrompt('downloader', now), expecting: 'downloaderUrl' };
  }
  if (line === 'Or visit the following URL and enter the code:') {
    return { ...state, expecting: 'downloaderPlainUrl' };
  }
  if (state.expecting && state.download && line.startsWith('https://')) {
    return { ...state, download: setUrl(state.download, line), expecting: null };
  }
  if ((match = line.match(/^Authorization code:\s*(\S+)$/)) && state.download) {
    return CODE.test(match[1]) ? { ...state, download: { ...state.download, code: match[1] }, expecting: null } : state;
  }
  if (state.download?.status === 'waiting') {
    if (
      VERSION_LINE.test(line) ||
      /^downloading latest\b/i.test(line) ||
      /^Download(?:ing\.\.\.|ed:)/.test(line) ||
      /^\[hytale\] (?:Server .* is up to date|Installed server)/.test(line)
    ) {
      return { ...state, download: end(state.download, 'success', now), expecting: null };
    }
    if (
      /^error (?:obtaining token|requesting device code)/i.test(line) ||
      line.startsWith('[hytale] Download failed') ||
      /did not complete\. Check the log in above/.test(line)
    ) {
      return { ...state, download: end(state.download, 'failed', now), expecting: null };
    }
  }

  // the server: a new run forgets what the previous one said (its first lines, before any
  // authentication)
  if (line === 'Starting HytaleServer') {
    return { ...state, server: null, auth: UNKNOWN_AUTH, expecting: null };
  }
  if ((match = line.match(/^Authentication mode: (\w+)$/))) {
    const offline = match[1].toUpperCase() === 'OFFLINE';
    return {
      ...state,
      server: null,
      auth: { ...UNKNOWN_AUTH, status: offline ? 'offline' : 'unknown' },
      expecting: null,
    };
  }

  // `/auth login device`
  if (line === 'DEVICE AUTHORIZATION') {
    return { ...state, server: newPrompt('server', now) };
  }
  if (state.server?.status === 'waiting') {
    if ((match = line.match(/^Visit:\s*(\S+)$/)) || (match = line.match(/^Or visit:\s*(\S+)$/))) {
      return { ...state, server: setUrl(state.server, match[1]) };
    }
    if ((match = line.match(/^Enter code:\s*(\S+)$/))) {
      return CODE.test(match[1]) ? { ...state, server: { ...state.server, code: match[1] } } : state;
    }
    if ((match = line.match(/^Waiting for authorization \(expires in (\d+) seconds\)/))) {
      return { ...state, server: { ...state.server, expiresIn: Number(match[1]) } };
    }
    if (/^Device authentication failed\b/.test(line) || /^OAuth device flow failed\b/.test(line)) {
      return { ...state, server: end(state.server, 'failed', now) };
    }
    if (/^Active login flow cancelled\b/.test(line)) {
      return { ...state, server: end(state.server, 'cancelled', now) };
    }
  }

  // authentication state
  if ((match = line.match(/^Authentication successful! Mode: (\w+)/))) {
    return authenticated(state, match[1], now);
  }
  if (line.startsWith('Authentication successful!') || line.startsWith('Profile selected!')) {
    return authenticated(state, null, now);
  }
  if ((match = line.match(/^Auth mode: (EXTERNAL_SESSION|SINGLEPLAYER)\b/))) {
    return authenticated(state, match[1], now);
  }
  if (/^Session restored from stored credentials\b/.test(line)) {
    return authenticated(state, 'OAUTH_STORE', now);
  }
  if (/^Server is already authenticated\b/.test(line)) {
    return { ...state, auth: { ...state.auth, status: 'authenticated' } };
  }
  if (/^No server tokens configured\b/.test(line)) {
    return state.auth.status === 'authenticated' ? state : { ...state, auth: { ...state.auth, status: 'missing' } };
  }
  if (/^Server logged out\b/.test(line)) {
    return { ...state, auth: { ...UNKNOWN_AUTH, status: 'missing', storage: state.auth.storage } };
  }
  if (
    /Token validation failed/i.test(line) ||
    /^(?:Identity|Session) token validation failed\b/.test(line) ||
    /^All refresh attempts failed\b/.test(line) ||
    /^Failed to restore session from stored credentials\b/.test(line)
  ) {
    return { ...state, auth: { ...state.auth, status: 'failed' } };
  }
  if (/^Multiple profiles available\b/.test(line) || /^Profile selection pending\b/.test(line)) {
    // the device login itself went through; the server now waits for `/auth select`
    return {
      ...state,
      server: end(state.server, 'success', now),
      auth: { ...state.auth, profileChoice: true, profiles: [] },
    };
  }
  if (state.auth.profileChoice && (match = line.match(/^\[(\d+)\] (.+?) \([0-9a-f-]{36}\)$/))) {
    const index = Number(match[1]);
    if (state.auth.profiles.some((profile) => profile.index === index)) return state;
    return {
      ...state,
      auth: { ...state.auth, profiles: [...state.auth.profiles, { index, name: match[2] }] },
    };
  }
  if (
    (match = line.match(
      /^(?:Auto-selected profile(?: from storage)?|Selected profile(?: from --owner-uuid)?): (.+?) \([0-9a-f-]{36}\)$/,
    ))
  ) {
    return { ...state, auth: { ...state.auth, profile: match[1], profileChoice: false, profiles: [] } };
  }
  if ((match = line.match(/^Identity token validated for (.+?) \([0-9a-f-]{36}\)$/))) {
    return match[1] === 'null' ? state : { ...state, auth: { ...state.auth, profile: match[1] } };
  }

  // credential storage
  if (/^WARNING: Credentials stored in memory only\b/.test(line)) {
    return storage(state, 'Memory');
  }
  if (
    (match = line.match(/^Credential storage(?: changed to)?: (\w+)$/)) ||
    (match = line.match(/^Credentials saved using: (\w+)$/)) ||
    (match = line.match(/^Auth credential store: (\w+)$/)) ||
    (match = line.match(/^Swapped credential store to: (\w+)$/))
  ) {
    return storage(state, match[1]);
  }

  // `/auth status`
  if ((match = line.match(/^Token Source: (.+)$/))) {
    const source = match[1];
    if (source === 'Not authenticated') return { ...state, auth: { ...state.auth, status: 'missing', mode: null } };
    return { ...state, auth: { ...state.auth, status: 'authenticated', mode: source } };
  }
  if ((match = line.match(/^Connection Auth: Offline$/))) {
    return { ...state, auth: { ...state.auth, status: 'offline' } };
  }
  if ((match = line.match(/^Profile: (.+?)(?: \([0-9a-f-]{36}\))?$/)) && state.auth.status !== 'unknown') {
    const profile = match[1];
    return /^(?:none|null|-|Unknown)$/i.test(profile) ? state : { ...state, auth: { ...state.auth, profile } };
  }

  return state;
}

/** Folds many lines (the console history the panel replays on connect, or a fixture). */
export function reduceAll(state: ConsoleAuthState, lines: Iterable<string>, now: number): ConsoleAuthState {
  let current = state;
  for (const line of lines) current = reduce(current, line, now);
  return current;
}

/** Seconds left on a waiting prompt's code (0 when it expired). */
export function secondsLeft(prompt: Prompt, now: number): number {
  return Math.max(0, Math.ceil((prompt.seenAt + prompt.expiresIn * 1000 - now) / 1000));
}

export function formatCountdown(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  return `${minutes}:${rest.toString().padStart(2, '0')}`;
}

/** The link the login button opens: the one with the code filled in when the console has it. */
export function loginUrl(prompt: Prompt): string | null {
  return prompt.completeUrl ?? prompt.url;
}

/** How long a finished prompt stays on screen. */
export const ENDED_VISIBLE_MS = 60_000;

export type CardView =
  | { kind: 'prompt'; prompt: Prompt; expired: boolean }
  | { kind: 'ended'; prompt: Prompt }
  | { kind: 'unauthenticated' }
  | { kind: 'failed' }
  | { kind: 'profileChoice' }
  | { kind: 'memoryOnly' }
  | { kind: 'authenticated' }
  | { kind: 'none' };

/**
 * What the card shows. `running` is the server's power state; the prompts win over the status
 * because they are what the user has to act on.
 */
export function view(state: ConsoleAuthState, now: number, running: boolean): CardView {
  for (const prompt of [state.download, state.server]) {
    if (prompt?.status === 'waiting' && (prompt.code || prompt.completeUrl || prompt.url)) {
      return { kind: 'prompt', prompt, expired: secondsLeft(prompt, now) === 0 };
    }
  }
  if (running && state.auth.profileChoice) return { kind: 'profileChoice' };
  for (const prompt of [state.download, state.server]) {
    if (prompt && prompt.status !== 'waiting' && prompt.endedAt !== null && now - prompt.endedAt < ENDED_VISIBLE_MS) {
      return { kind: 'ended', prompt };
    }
  }
  if (!running) return { kind: 'none' };
  switch (state.auth.status) {
    case 'missing':
      return { kind: 'unauthenticated' };
    case 'failed':
      return { kind: 'failed' };
    case 'authenticated':
      return state.auth.storage === 'Memory' && state.auth.mode !== 'EXTERNAL_SESSION'
        ? { kind: 'memoryOnly' }
        : { kind: 'authenticated' };
    default:
      return { kind: 'none' };
  }
}

/** Whether a server looks like Hytale: our egg's start command or image, or any egg running HytaleServer.jar. */
export function isHytaleServer(server: { startup: string; image: string }): boolean {
  return /HytaleServer\.jar/.test(server.startup) || /(?:^|[/:_-])hytale(?:$|[/:_@-])/i.test(server.image);
}
