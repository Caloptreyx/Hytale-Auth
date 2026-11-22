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
