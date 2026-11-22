import {
  faArrowUpRightFromSquare,
  faCircleCheck,
  faCircleXmark,
  faCopy,
  faKey,
  faRotateRight,
  faShieldHalved,
  faTriangleExclamation,
} from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { Button as MantineButton } from '@mantine/core';
import { useEffect, useMemo, useRef, useState } from 'react';
import { useShallow } from 'zustand/react/shallow';
import ActionIcon from '@/elements/buttons/ActionIcon.tsx';
import Button from '@/elements/buttons/Button.tsx';
import Badge from '@/elements/data-display/Badge.tsx';
import Card from '@/elements/data-display/Card.tsx';
import Group from '@/elements/layout/Group.tsx';
import Stack from '@/elements/layout/Stack.tsx';
import Tooltip from '@/elements/overlays/Tooltip.tsx';
import Code from '@/elements/typography/Code.tsx';
import Text from '@/elements/typography/Text.tsx';
import { handleRawCopyToClipboard } from '@/lib/clipboard/copy.ts';
import { useServerCan } from '@/plugins/usePermissions.ts';
import useWebsocketEvent, { SocketEvent, SocketRequest } from '@/plugins/websocket/useWebsocketEvent.ts';
import { useToast } from '@/providers/ToastProvider.tsx';
import { useServerStore } from '@/stores/server.ts';
import {
  type ConsoleAuthState,
  ENDED_VISIBLE_MS,
  formatCountdown,
  initialState,
  isHytaleServer,
  loginUrl,
  type Prompt,
  reduce,
  secondsLeft,
  view,
} from '../lib/console.ts';
import { useExtTranslations } from '../translations.ts';

/** A login started from this tab gets `/auth persistence Encrypted` once it succeeds. */
const AUTO_PERSIST_WINDOW_MS = 20 * 60_000;
const ENCRYPTED_PERSISTENCE = '/auth persistence Encrypted';

/** Mounted above the console of every server; renders only for Hytale servers. */
export default function ConsoleLoginCard() {
  const isHytale = useServerStore((state) => isHytaleServer(state.server));
  return isHytale ? <HytaleLoginCard /> : null;
}

function HytaleLoginCard() {
  const { t } = useExtTranslations();
  const { addToast } = useToast();
  const { serverUuid, power, socketInstance } = useServerStore(
    useShallow((state) => ({
      serverUuid: state.server.uuid,
      power: state.state,
      socketInstance: state.socketInstance,
    })),
  );
  const canConsole = useServerCan('control.console');
  const canRestart = useServerCan('control.restart');

  const [auth, setAuth] = useState<ConsoleAuthState>(initialState);
  const [now, setNow] = useState(() => Date.now());
  const loginRequestedAt = useRef<number | null>(null);
  const [persisting, setPersisting] = useState(false);

  useEffect(() => {
    setAuth(initialState());
    loginRequestedAt.current = null;
  }, [serverUuid]);

  useWebsocketEvent(SocketEvent.CONSOLE_OUTPUT, (line) => {
    if (typeof line !== 'string') return;
    setAuth((current) => reduce(current, line, Date.now()));
  });

  const send = (command: string) => socketInstance?.send(SocketRequest.SEND_COMMAND, command);

  // after a login this tab asked for, keep it: the server holds it in memory otherwise
  const serverStatus = auth.server?.status;
  useEffect(() => {
    const requested = loginRequestedAt.current;
    if (serverStatus !== 'success' || requested === null) return;
    loginRequestedAt.current = null;
    if (Date.now() - requested > AUTO_PERSIST_WINDOW_MS || auth.auth.storage === 'Encrypted') return;
    if (canConsole) {
      setPersisting(true);
      send(ENCRYPTED_PERSISTENCE);
    }
  }, [serverStatus]);
  useEffect(() => {
    if (auth.auth.storage === 'Encrypted') setPersisting(false);
  }, [auth.auth.storage]);

  const current = view(auth, now, power === 'running');
  const ticking =
    current.kind === 'prompt' ||
    current.kind === 'ended' ||
    [auth.download, auth.server].some((p) => p?.endedAt != null && Date.now() - p.endedAt < ENDED_VISIBLE_MS);
  useEffect(() => {
    if (!ticking) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [ticking]);

  const authenticate = () => {
    loginRequestedAt.current = Date.now();
    send('/auth login device');
  };
  const authenticateButton = canConsole ? (
    <Button leftSection={<FontAwesomeIcon icon={faKey} />} onClick={authenticate} disabled={!socketInstance}>
      {t('card.authenticate', {})}
    </Button>
  ) : (
    <Text size='sm' c='dimmed'>
      {t('card.noConsole', {})}
    </Text>
  );

  switch (current.kind) {
    case 'prompt':
      return (
        <PromptCard
          prompt={current.prompt}
          expired={current.expired}
          now={now}
          onCopy={(code) => handleRawCopyToClipboard(code, addToast)}
          onNewCode={canConsole ? authenticate : null}
          onRestart={canRestart ? () => socketInstance?.send(SocketRequest.SET_STATE, 'restart') : null}
        />
      );
    case 'ended': {
      const { prompt } = current;
      const ok = prompt.status === 'success';
      const message =
        prompt.source === 'downloader'
          ? t(ok ? 'card.download.success' : 'card.download.failed', {})
          : prompt.status === 'cancelled'
            ? t('card.server.cancelled', {})
            : t(ok ? 'card.server.success' : 'card.server.failed', {});
      return (
        <Shell tone={ok ? 'green' : 'red'}>
          <Group justify='space-between' wrap='wrap' gap='sm'>
            <Group gap='sm' wrap='nowrap'>
              <FontAwesomeIcon
                icon={ok ? faCircleCheck : faCircleXmark}
                color={`var(--mantine-color-${ok ? 'green' : 'red'}-5)`}
              />
              <Text size='sm'>{message}</Text>
            </Group>
            {!ok && prompt.source === 'server' && canConsole ? (
              <Button variant='light' leftSection={<FontAwesomeIcon icon={faRotateRight} />} onClick={authenticate}>
                {t('card.newCode', {})}
              </Button>
            ) : null}
          </Group>
        </Shell>
      );
    }
    case 'unauthenticated':
    case 'failed':
      return (
        <Shell tone={current.kind === 'failed' ? 'red' : 'yellow'}>
          <Group justify='space-between' wrap='wrap' gap='md'>
            <Group gap='sm' wrap='nowrap' align='flex-start'>
              <FontAwesomeIcon
                icon={faTriangleExclamation}
                className='mt-1'
                color={`var(--mantine-color-${current.kind === 'failed' ? 'red' : 'yellow'}-5)`}
              />
              <Stack gap={2}>
                <Text fw={600}>
                  {t(current.kind === 'failed' ? 'card.failedAuth.title' : 'card.unauthenticated.title', {})}
                </Text>
                <Text size='sm' c='dimmed'>
                  {t(
                    current.kind === 'failed' ? 'card.failedAuth.description' : 'card.unauthenticated.description',
                    {},
                  )}
                </Text>
              </Stack>
            </Group>
            {authenticateButton}
          </Group>
        </Shell>
      );
    case 'profileChoice':
      return (
        <Shell tone='yellow'>
          <Stack gap='xs'>
            <Text fw={600}>{t('card.profileChoice.title', {})}</Text>
            <Text size='sm' c='dimmed'>
              {t('card.profileChoice.description', {})}
            </Text>
            {auth.auth.profiles.length && canConsole ? (
              <Group gap='xs'>
                {auth.auth.profiles.map((profile) => (
                  <Button key={profile.index} variant='light' onClick={() => send(`/auth select ${profile.index}`)}>
                    {profile.name}
                  </Button>
                ))}
              </Group>
            ) : (
              <Text size='sm'>{canConsole ? t('card.profileChoice.unlisted', {}) : t('card.noConsole', {})}</Text>
            )}
          </Stack>
        </Shell>
      );
    case 'memoryOnly':
      return (
        <Shell tone='yellow'>
          <Group justify='space-between' wrap='wrap' gap='md'>
            <Stack gap={2}>
              <Text fw={600}>{t('card.memoryOnly.title', {})}</Text>
              <Text size='sm' c='dimmed'>
                {t('card.memoryOnly.description', {})}
              </Text>
            </Stack>
            {canConsole ? (
              <Button
                leftSection={<FontAwesomeIcon icon={faShieldHalved} />}
                loading={persisting}
                onClick={() => {
                  setPersisting(true);
                  send(ENCRYPTED_PERSISTENCE);
                }}
              >
                {t('card.memoryOnly.persist', {})}
              </Button>
            ) : null}
          </Group>
        </Shell>
      );
    case 'authenticated':
      return <StatusLine auth={auth} onCheck={canConsole ? () => send('/auth status') : null} />;
    case 'none':
      return power === 'running' && auth.auth.status === 'unknown' && canConsole ? (
        <StatusLine auth={auth} onCheck={() => send('/auth status')} />
      ) : null;
  }
}

function Shell({ tone, children }: { tone: 'green' | 'red' | 'yellow' | 'blue' | 'gray'; children: React.ReactNode }) {
  return (
    <Card
      withBorder
      radius='md'
      mb='md'
      leftStripeClassName={`bg-(--mantine-color-${tone}-6)`}
      className='hytale-auth-card'
    >
      {children}
    </Card>
  );
}

function PromptCard({
  prompt,
  expired,
  now,
  onCopy,
  onNewCode,
  onRestart,
}: {
  prompt: Prompt;
  expired: boolean;
  now: number;
  onCopy: (code: string) => void;
  onNewCode: (() => void) | null;
  onRestart: (() => void) | null;
}) {
  const { t } = useExtTranslations();
  const url = loginUrl(prompt);
  const left = secondsLeft(prompt, now);
  const isDownload = prompt.source === 'downloader';
  const manualUrl = prompt.completeUrl && prompt.url ? prompt.url : null;

  return (
    <Shell tone={expired ? 'red' : 'blue'}>
      <Stack gap='sm'>
        <Group justify='space-between' wrap='wrap' gap='xs'>
          <Group gap='sm' wrap='nowrap'>
            <FontAwesomeIcon icon={faKey} color='var(--mantine-color-blue-5)' />
            <Text fw={600}>{t(isDownload ? 'card.download.title' : 'card.server.title', {})}</Text>
          </Group>
          <Badge color={expired ? 'red' : left < 60 ? 'yellow' : 'blue'} variant='light'>
            {expired ? t('card.expired', {}) : t('card.expiresIn', { time: formatCountdown(left) })}
          </Badge>
        </Group>
        <Text size='sm' c='dimmed'>
          {t(isDownload ? 'card.download.description' : 'card.server.description', {})}
        </Text>
        {expired ? (
          <Group gap='sm' wrap='wrap'>
            <Text size='sm'>{t(isDownload ? 'card.expiredDownload' : 'card.expiredServer', {})}</Text>
            {!isDownload && onNewCode ? (
              <Button variant='light' leftSection={<FontAwesomeIcon icon={faRotateRight} />} onClick={onNewCode}>
                {t('card.newCode', {})}
              </Button>
            ) : null}
            {isDownload && onRestart ? (
              <Button variant='light' leftSection={<FontAwesomeIcon icon={faRotateRight} />} onClick={onRestart}>
                {t('card.restart', {})}
              </Button>
            ) : null}
          </Group>
        ) : (
          <Group gap='md' wrap='wrap' align='center'>
            {url ? (
              <MantineButton
                component='a'
                href={url}
                target='_blank'
                rel='noopener noreferrer'
                leftSection={<FontAwesomeIcon icon={faArrowUpRightFromSquare} />}
                style={{ fontWeight: 'normal' }}
              >
                {t('card.login', {})}
              </MantineButton>
            ) : null}
            {prompt.code ? (
              <Group gap={6} wrap='nowrap'>
                <Text size='sm' c='dimmed'>
                  {t('card.code', {})}
                </Text>
                <Code className='text-base! tracking-widest select-all'>{prompt.code}</Code>
                <Tooltip label={t('card.copyCode', {})}>
                  <ActionIcon variant='subtle' aria-label={t('card.copyCode', {})} onClick={() => onCopy(prompt.code!)}>
                    <FontAwesomeIcon icon={faCopy} />
                  </ActionIcon>
                </Tooltip>
              </Group>
            ) : null}
          </Group>
        )}
        {!expired && manualUrl ? (
          <Text size='xs' c='dimmed'>
            {t('card.manualHint', { url: manualUrl })}
          </Text>
        ) : null}
      </Stack>
    </Shell>
  );
}

function StatusLine({ auth, onCheck }: { auth: ConsoleAuthState; onCheck: (() => void) | null }) {
  const { t } = useExtTranslations();
  const { mode, profile, storage, status } = auth.auth;
  const details = useMemo(() => {
    if (status !== 'authenticated') return null;
    const parts: string[] = [];
    if (profile) parts.push(t('card.status.as', { profile }));
    const modeKey = mode as keyof typeof MODE_KEYS | null;
    if (modeKey && modeKey in MODE_KEYS) parts.push(t(MODE_KEYS[modeKey], {}));
    else if (mode) parts.push(mode);
    if ((storage === 'Encrypted' || storage === 'Memory') && mode !== 'EXTERNAL_SESSION') {
      parts.push(t(storage === 'Encrypted' ? 'card.status.storage.Encrypted' : 'card.status.storage.Memory', {}));
    }
    return parts.join(' · ');
  }, [status, mode, profile, storage]);

  return (
    <Group
      justify='space-between'
      wrap='nowrap'
      gap='sm'
      mb='md'
      px='md'
      py={6}
      className='rounded-md border border-(--mantine-color-default-border)'
    >
      <Group gap='sm' wrap='nowrap' className='min-w-0'>
        <FontAwesomeIcon
          icon={status === 'authenticated' ? faCircleCheck : faShieldHalved}
          color={`var(--mantine-color-${status === 'authenticated' ? 'green' : 'gray'}-5)`}
        />
        <Text size='sm' truncate>
          {status === 'authenticated' ? t('card.status.signedIn', {}) : t('card.status.unknown', {})}
          {details ? (
            <Text span size='sm' c='dimmed'>
              {' '}
              {details}
            </Text>
          ) : null}
        </Text>
      </Group>
      {onCheck ? (
        <Button size='compact-sm' variant='subtle' onClick={onCheck}>
          {t('card.status.check', {})}
        </Button>
      ) : null}
    </Group>
  );
}

const MODE_KEYS = {
  OAUTH_DEVICE: 'card.status.mode.OAUTH_DEVICE',
  OAUTH_BROWSER: 'card.status.mode.OAUTH_BROWSER',
  OAUTH_STORE: 'card.status.mode.OAUTH_STORE',
  EXTERNAL_SESSION: 'card.status.mode.EXTERNAL_SESSION',
} as const;
