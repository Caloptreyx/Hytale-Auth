import { type DefinedTranslations, defineTranslations, type GetPlaceholders, type PathValue } from 'shared';

const translations = defineTranslations({
  items: {},
  translations: {
    title: 'Hytale Auth',
    card: {
      download: {
        title: 'Log in to download Hytale',
        description:
          'The server needs a Hytale account once to download the game files. Open the link, sign in and approve: the download starts by itself.',
        success: 'Logged in. The server is downloading the game files.',
        failed: 'The download login did not complete. Restart the server for a new code.',
      },
      server: {
        title: 'Sign this server in to Hytale',
        description:
          'Players can only join once the server is signed in to a Hytale account. Open the link, sign in and approve.',
        success: 'The server is signed in to Hytale.',
        failed: 'The login did not complete.',
        cancelled: 'The login was cancelled.',
      },
      login: 'Log in with Hytale',
      code: 'Code',
      copyCode: 'Copy the code',
      expiresIn: 'Expires in {time}',
      expired: 'Code expired',
      expiredDownload: 'This code expired. Restart the server for a new one.',
      expiredServer: 'This code expired.',
      newCode: 'Get a new code',
      restart: 'Restart',
      manualHint: 'Or open {url} and enter the code.',
      unauthenticated: {
        title: 'This server is not signed in to Hytale',
        description: 'Players cannot join until it is. Signing in takes one login with your Hytale account.',
      },
      failedAuth: {
        title: 'The Hytale login of this server failed',
        description: 'Its login was rejected or expired. Sign it in again so players can join.',
      },
      authenticate: 'Authenticate server',
      profileChoice: {
        title: 'Choose the game profile',
        description: 'The Hytale account has more than one game profile. Pick the one the server runs as.',
        unlisted: 'Run /auth select <number> in the console, with the number listed there.',
      },
      memoryOnly: {
        title: 'Signed in until the next restart',
        description: 'The server keeps its login in memory only. Save it, encrypted in the server files, to keep it.',
        persist: 'Keep login after restarts',
        persisting: 'Saving the login encrypted…',
      },
      status: {
        signedIn: 'Signed in to Hytale',
        as: 'as {profile}',
        unknown: 'Hytale login: no status in the console yet',
        check: 'Check status',
        mode: {
          OAUTH_DEVICE: 'device login',
          OAUTH_BROWSER: 'browser login',
          OAUTH_STORE: 'saved login',
          EXTERNAL_SESSION: 'linked account',
        },
        storage: {
          Encrypted: 'saved encrypted',
          Memory: 'in memory only',
        },
      },
      noConsole: 'Ask someone with console access to the server to sign it in.',
      auto: {
        failed: 'The automatic login did not work at this start',
        fallback: 'The server started without a login. Sign it in here, or fix the account and restart.',
        signedIn: 'Signed in automatically with {account}',
      },
    },
    source: {
      user: 'your own Hytale account',
      userOther: "the owner's Hytale account",
      host: 'the host account',
    },
    errors: {
      relink: 'The login expired (unused for 30 days) or was revoked. Link the account again.',
      limit:
        'Hytale refused a game session: the account reached its limit of servers running at the same time (500 per game license), or it may not host servers.',
      profile: 'The selected game profile no longer exists. Choose another one.',
      no_profile: 'This Hytale account has no game profile. Does it own Hytale?',
      unavailable: 'Hytale could not be reached.',
      unknown: 'Something went wrong with the Hytale login.',
    },
    link: {
      link: 'Link a Hytale account',
      relink: 'Link again',
      waiting: 'Waiting for the approval…',
      instructions: 'Open the link, sign in to the Hytale account and approve. This page continues by itself.',
      expired: 'The code expired before it was approved. Start again for a new one.',
      denied: 'The login was declined.',
      linked: 'Linked {profile}.',
      cancel: 'Cancel',
      none: 'No Hytale account is linked.',
      profile: 'Game profile',
      profileDescription: 'The profile servers run as.',
      reload: 'Reload profiles',
      unlink: 'Unlink',
      unlinkTitle: 'Unlink the Hytale account?',
      unlinkBody:
        'Servers stop signing in with it at their next start. Servers already running keep their login until they stop.',
      status: {
        linked: 'Linked',
        relink: 'Needs linking again',
      },
      refreshed: 'Login last renewed {time}',
      servers: 'Used by {count} servers at their last start',
      stored:
        'The panel keeps the login (an OAuth refresh token) encrypted with its encryption key and renews it before it expires. It never leaves the panel: servers only get a game session for each start.',
    },
    host: {
      title: 'Host account',
      description:
        'Signs in every Hytale server whose owner has not linked an account of their own, at every start. Players can join without anyone logging in on the console.',
    },
    settings: {
      title: 'Settings',
      hostAccount: 'Use the host account',
      hostAccountDescription: 'Servers without an account of their own sign in with the host account.',
      userAccounts: 'Let users link their own account',
      userAccountsDescription:
        "A user's own Hytale account signs in the servers they own, instead of the host account.",
      save: 'Save',
      saved: 'Settings saved.',
    },
    account: {
      title: 'Hytale Account',
      description:
        'Link your Hytale account and your Hytale servers sign in with it at every start, so players can join without a console login.',
      disabled: 'Linking your own Hytale account is turned off on this panel.',
      hostFallback: 'Without an account of your own, your servers sign in with the host account when there is one.',
    },
    server: {
      title: 'Hytale login',
      automatic: 'Sign in automatically at start',
      automaticDescription: 'Starts the server with a game session of a linked Hytale account.',
      next: 'Next start signs in with {account}.',
      nextProfile: 'Next start signs in with {account} as {profile}.',
      noAccount: 'No linked Hytale account is available: sign the server in on the console.',
      off: 'Automatic login is off: sign the server in on the console.',
      linkOwn: 'Link your own account',
      saved: 'Saved. It applies from the next start.',
      lastOk: 'Last start: signed in as {profile}.',
    },
  },
});

export const useExtTranslations = translations.useTranslations.bind(translations);
export const getExtTranslations = translations.getTranslations.bind(translations);

type Messages = (typeof translations)['obj'];
type ExtKey = typeof translations extends DefinedTranslations<infer _I, infer _O, infer P> ? P : never;
type ExtValues<K extends ExtKey> = Record<GetPlaceholders<PathValue<Messages, K> & string>[number], string | number>;
/** `t` of `useExtTranslations`, for helpers outside components. */
export type ExtT = <K extends ExtKey>(key: K, values: ExtValues<K>) => string;

export default translations;
