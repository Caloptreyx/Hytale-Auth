import {
  faArrowUpRightFromSquare,
  faCopy,
  faLink,
  faLinkSlash,
  faRotateRight,
  faTriangleExclamation,
} from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { Button as MantineButton } from '@mantine/core';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef, useState } from 'react';
import { httpErrorToHuman } from '@/api/axios.ts';
import ActionIcon from '@/elements/buttons/ActionIcon.tsx';
import Button from '@/elements/buttons/Button.tsx';
import Badge from '@/elements/data-display/Badge.tsx';
import Alert from '@/elements/feedback/Alert.tsx';
import Spinner from '@/elements/feedback/Spinner.tsx';
import Select from '@/elements/input/Select.tsx';
import Group from '@/elements/layout/Group.tsx';
import Stack from '@/elements/layout/Stack.tsx';
import ConfirmationModal from '@/elements/modals/ConfirmationModal.tsx';
import Tooltip from '@/elements/overlays/Tooltip.tsx';
import Code from '@/elements/typography/Code.tsx';
import Text from '@/elements/typography/Text.tsx';
import { handleRawCopyToClipboard } from '@/lib/clipboard/copy.ts';
import { useToast } from '@/providers/ToastProvider.tsx';
import type { Account, AccountApi, AccountOverview, ErrorInfo, PendingLink, PollResult } from '../api.ts';
import { formatCountdown, isHytaleUrl } from '../lib/console.ts';
import { type ExtT, useExtTranslations } from '../translations.ts';

const ERROR_CODES = ['relink', 'limit', 'profile', 'no_profile', 'unavailable'] as const;

/** The translated explanation of an error code, with Hytale's own text for outages. */
export function errorText(t: ExtT, code: string, message?: string | null): string {
  const known = (ERROR_CODES as readonly string[]).includes(code)
    ? t(`errors.${code as (typeof ERROR_CODES)[number]}`, {})
    : t('errors.unknown', {});
  return code === 'unavailable' && message ? `${known} ${message}` : known;
}

/** Links, shows and unlinks one Hytale account: the host account or the user's own. */
export default function AccountLinker({
  api,
  queryKey,
  canManage,
  footer,
}: {
  api: AccountApi;
  queryKey: readonly unknown[];
  canManage: boolean;
  footer?: (overview: AccountOverview) => React.ReactNode;
}) {
  const { t } = useExtTranslations();
  const { addToast } = useToast();
  const queryClient = useQueryClient();
  const query = useQuery({ queryKey, queryFn: api.overview });
  const [link, setLink] = useState<PendingLink | null>(null);
  const [notice, setNotice] = useState<{ color: string; text: string } | null>(null);
  const [busy, setBusy] = useState<'link' | 'profile' | 'reload' | null>(null);
  const [confirmUnlink, setConfirmUnlink] = useState(false);

  useEffect(() => {
    if (query.data?.link) setLink(query.data.link);
  }, [query.data?.link?.user_code]);

  const refresh = () => queryClient.invalidateQueries({ queryKey });

  const startLink = async () => {
    setBusy('link');
    setNotice(null);
    try {
      setLink(await api.startLink());
    } catch (error) {
      addToast(httpErrorToHuman(error), 'error');
    } finally {
      setBusy(null);
    }
  };

  const onPolled = (result: PollResult) => {
    switch (result.status) {
      case 'pending':
        setLink(result.link);
        return;
      case 'linked':
        setLink(null);
        setNotice({ color: 'green', text: t('link.linked', { profile: result.account.profile?.username ?? '' }) });
        refresh();
        return;
      case 'expired':
      case 'none':
        setLink(null);
        setNotice({ color: 'yellow', text: t('link.expired', {}) });
        return;
      case 'denied':
        setLink(null);
        setNotice({ color: 'red', text: t('link.denied', {}) });
        return;
      case 'failed':
        setNotice({ color: 'yellow', text: errorText(t, result.error, result.message) });
    }
  };

  if (query.isLoading) {
    return (
      <Group justify='center' py='md'>
        <Spinner />
      </Group>
    );
  }
  if (query.error || !query.data) {
    return <Alert color='red'>{httpErrorToHuman(query.error)}</Alert>;
  }

  const { account } = query.data;

  return (
    <Stack gap='md'>
      {notice ? (
        <Alert color={notice.color} withCloseButton onClose={() => setNotice(null)}>
          {notice.text}
        </Alert>
      ) : null}

      {link ? (
        <LinkPrompt
          link={link}
          poll={api.poll}
          onPolled={onPolled}
          onCancel={async () => {
            await api.cancelLink().catch(() => undefined);
            setLink(null);
          }}
          onCopy={(code) => handleRawCopyToClipboard(code, addToast)}
        />
      ) : null}

      {account ? (
        <AccountDetails
          account={account}
          servers={query.data.servers}
          canManage={canManage}
          busy={busy}
          onSelect={async (profile) => {
            setBusy('profile');
            try {
              await api.selectProfile(profile);
              refresh();
            } catch (error) {
              addToast(httpErrorToHuman(error), 'error');
            } finally {
              setBusy(null);
            }
          }}
          onReload={async () => {
            setBusy('reload');
            try {
              await api.reloadProfiles();
              refresh();
            } catch (error) {
              addToast(httpErrorToHuman(error), 'error');
            } finally {
              setBusy(null);
            }
          }}
        />
      ) : link ? null : (
        <Text size='sm' c='dimmed'>
          {t('link.none', {})}
        </Text>
      )}

      {canManage && !link ? (
        <Group gap='sm'>
          <Button
            leftSection={<FontAwesomeIcon icon={account ? faRotateRight : faLink} />}
            variant={account && account.status === 'linked' ? 'default' : 'filled'}
            loading={busy === 'link'}
            onClick={startLink}
          >
            {account ? t('link.relink', {}) : t('link.link', {})}
          </Button>
          {account ? (
            <Button
              color='red'
              variant='subtle'
              leftSection={<FontAwesomeIcon icon={faLinkSlash} />}
              onClick={() => setConfirmUnlink(true)}
            >
              {t('link.unlink', {})}
            </Button>
          ) : null}
        </Group>
      ) : null}

      <Text size='xs' c='dimmed'>
        {t('link.stored', {})}
      </Text>
      {footer?.(query.data)}

      <ConfirmationModal
        opened={confirmUnlink}
        onClose={() => setConfirmUnlink(false)}
        title={t('link.unlinkTitle', {})}
        confirm={t('link.unlink', {})}
        onConfirmed={async () => {
          await api.unlink();
          setConfirmUnlink(false);
          refresh();
        }}
      >
        {t('link.unlinkBody', {})}
      </ConfirmationModal>
    </Stack>
  );
}

function AccountDetails({
  account,
  servers,
  canManage,
  busy,
  onSelect,
  onReload,
}: {
  account: Account;
  servers: number | null;
  canManage: boolean;
  busy: string | null;
  onSelect: (profile: string) => void;
  onReload: () => void;
}) {
  const { t } = useExtTranslations();
  const error: ErrorInfo | null = account.error;

  return (
    <Stack gap='sm'>
      <Group gap='sm'>
        <Text fw={600}>{account.profile?.username ?? '—'}</Text>
        <Badge color={account.status === 'linked' ? 'green' : 'red'} variant='light'>
          {t(account.status === 'linked' ? 'link.status.linked' : 'link.status.relink', {})}
        </Badge>
      </Group>
      {error ? (
        <Alert
          color={error.code === 'relink' ? 'red' : 'yellow'}
          icon={<FontAwesomeIcon icon={faTriangleExclamation} />}
        >
          {errorText(t, error.code, error.message)}
        </Alert>
      ) : null}
      <Group gap='sm' align='flex-end'>
        <Select
          label={t('link.profile', {})}
          description={t('link.profileDescription', {})}
          data={account.profiles.map((profile) => ({ value: profile.uuid, label: profile.username }))}
          value={account.profile?.uuid ?? null}
          onChange={(value) => value && onSelect(value)}
          disabled={!canManage || busy === 'profile' || account.profiles.length < 2}
          allowDeselect={false}
          className='min-w-56'
        />
        {canManage ? (
          <Button
            variant='default'
            loading={busy === 'reload'}
            disabled={account.status !== 'linked'}
            onClick={onReload}
          >
            {t('link.reload', {})}
          </Button>
        ) : null}
      </Group>
      <Text size='sm' c='dimmed'>
        {t('link.refreshed', { time: new Date(account.refreshed).toLocaleString() })}
        {servers !== null ? ` · ${t('link.servers', { count: servers })}` : ''}
      </Text>
    </Stack>
  );
}

function LinkPrompt({
  link,
  poll,
  onPolled,
  onCancel,
  onCopy,
}: {
  link: PendingLink;
  poll: AccountApi['poll'];
  onPolled: (result: PollResult) => void;
  onCancel: () => void;
  onCopy: (code: string) => void;
}) {
  const { t } = useExtTranslations();
  const [now, setNow] = useState(() => Date.now());
  const onPolledRef = useRef(onPolled);
  onPolledRef.current = onPolled;

  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, []);

  // poll at the interval Hytale asked for (the backend enforces it too)
  useEffect(() => {
    let stopped = false;
    let timer: number | undefined;
    const tick = async () => {
      try {
        const result = await poll();
        if (stopped) return;
        onPolledRef.current(result);
        if (result.status !== 'pending' && result.status !== 'failed') return;
      } catch {
        // a network hiccup: try again at the next interval
      }
      if (!stopped) timer = window.setTimeout(tick, Math.max(link.interval, 1) * 1000);
    };
    timer = window.setTimeout(tick, Math.max(link.interval, 1) * 1000);
    return () => {
      stopped = true;
      window.clearTimeout(timer);
    };
  }, [link.user_code, link.interval]);

  const left = Math.max(0, Math.ceil((new Date(link.expires).getTime() - now) / 1000));
  const url = [link.verification_uri_complete, link.verification_uri].find((u) => u && isHytaleUrl(u)) ?? null;

  return (
    <Stack gap='sm' className='rounded-md border border-(--mantine-color-default-border) p-4'>
      <Group justify='space-between' gap='xs'>
        <Text fw={600}>{t('link.waiting', {})}</Text>
        <Badge color={left < 60 ? 'yellow' : 'blue'} variant='light'>
          {left ? t('card.expiresIn', { time: formatCountdown(left) }) : t('card.expired', {})}
        </Badge>
      </Group>
      <Text size='sm' c='dimmed'>
        {t('link.instructions', {})}
      </Text>
      <Group gap='md' align='center'>
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
        <Group gap={6} wrap='nowrap'>
          <Text size='sm' c='dimmed'>
            {t('card.code', {})}
          </Text>
          <Code className='text-base! tracking-widest select-all'>{link.user_code}</Code>
          <Tooltip label={t('card.copyCode', {})}>
            <ActionIcon variant='subtle' aria-label={t('card.copyCode', {})} onClick={() => onCopy(link.user_code)}>
              <FontAwesomeIcon icon={faCopy} />
            </ActionIcon>
          </Tooltip>
        </Group>
        <Button variant='subtle' color='gray' onClick={onCancel}>
          {t('link.cancel', {})}
        </Button>
      </Group>
    </Stack>
  );
}
