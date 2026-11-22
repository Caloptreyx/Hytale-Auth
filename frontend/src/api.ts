import { axiosInstance } from '@/api/axios.ts';

// The backend's payloads keep their snake_case names. None of them carry tokens.

export const PACKAGE = 'dev.caloptreyx.hytaleauth';
const ADMIN_BASE = `/api/admin/extensions/${PACKAGE}`;
const ACCOUNT_BASE = '/api/client/hytale-auth/account';
const serverBase = (server: string) => `/api/client/servers/${server}/hytale-auth`;

export interface Settings {
  host_account: boolean;
  user_accounts: boolean;
}

export interface Profile {
  uuid: string;
  username: string;
}

/** relink, limit, profile, no_profile, unavailable */
export type ErrorCode = string;

export interface ErrorInfo {
  code: ErrorCode;
  message: string;
  at: string;
}

export interface Account {
  id: string;
  profile: Profile | null;
  profiles: Profile[];
  status: 'linked' | 'relink';
  error: ErrorInfo | null;
  refreshed: string;
  created: string;
}

export interface PendingLink {
  user_code: string;
  verification_uri: string;
  verification_uri_complete: string | null;
  expires: string;
  interval: number;
}

export interface AccountOverview {
  account: Account | null;
  link: PendingLink | null;
  /** Host account only: servers whose last start signed in with it. */
  servers: number | null;
  /** User accounts only. */
  enabled?: boolean;
  host_account?: boolean;
}

export type PollResult =
  | { status: 'none' }
  | { status: 'pending'; link: PendingLink }
  | { status: 'expired' }
  | { status: 'denied' }
  | { status: 'linked'; account: Account }
  | { status: 'failed'; error: ErrorCode; message: string };

export type Source = 'user' | 'host';

export interface ServerStatus {
  hytale: boolean;
  automatic: boolean;
  can_manage: boolean;
  is_owner: boolean;
  user_accounts: boolean;
  owner_linked: boolean;
  host_linked: boolean;
  /** The account the next start signs in with. */
  next: { source: Source; profile_name: string | null } | null;
  own_problem: { status: Account['status']; error: ErrorInfo | null } | null;
  host_problem: { status: Account['status']; error: ErrorInfo | null } | null;
  last: {
    source: Source | null;
    profile_name: string | null;
    minted: string | null;
    session_expires: string | null;
    error: ErrorCode | null;
    error_message: string | null;
    error_at: string | null;
  } | null;
}

/** The host account (admin) or the user's own account. */
export interface AccountApi {
  overview(): Promise<AccountOverview>;
  startLink(): Promise<PendingLink>;
  poll(): Promise<PollResult>;
  cancelLink(): Promise<void>;
  selectProfile(profile: string): Promise<Account>;
  reloadProfiles(): Promise<Account>;
  unlink(): Promise<void>;
}

function accountApi(base: string): AccountApi {
  return {
    overview: async () => (await axiosInstance.get(base)).data,
    startLink: async () => (await axiosInstance.post(`${base}/link`)).data.link,
    poll: async () => (await axiosInstance.post(`${base}/link/poll`)).data,
    cancelLink: async () => {
      await axiosInstance.delete(`${base}/link`);
    },
    selectProfile: async (profile) => (await axiosInstance.put(`${base}/profile`, { profile })).data.account,
    reloadProfiles: async () => (await axiosInstance.post(`${base}/profiles`)).data.account,
    unlink: async () => {
      await axiosInstance.delete(base);
    },
  };
}

export const hostAccount = accountApi(`${ADMIN_BASE}/host`);
export const ownAccount = accountApi(ACCOUNT_BASE);

export async function getSettings(): Promise<Settings> {
  return (await axiosInstance.get(`${ADMIN_BASE}/settings`)).data.settings;
}

export async function updateSettings(settings: Settings): Promise<Settings> {
  return (await axiosInstance.put(`${ADMIN_BASE}/settings`, settings)).data.settings;
}

export async function getServerStatus(server: string): Promise<ServerStatus> {
  return (await axiosInstance.get(`${serverBase(server)}/status`)).data;
}

export async function setAutomatic(server: string, automatic: boolean): Promise<boolean> {
  return (await axiosInstance.put(`${serverBase(server)}/automatic`, { automatic })).data.automatic;
}
