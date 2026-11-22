import { faKey, faTriangleExclamation } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { Button as MantineButton } from '@mantine/core';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { Link } from 'react-router';
import { httpErrorToHuman } from '@/api/axios.ts';
import TitleCard from '@/elements/data-display/TitleCard.tsx';
import Alert from '@/elements/feedback/Alert.tsx';
import Switch from '@/elements/input/Switch.tsx';
import Stack from '@/elements/layout/Stack.tsx';
import Text from '@/elements/typography/Text.tsx';
import { useToast } from '@/providers/ToastProvider.tsx';
import { useServerStore } from '@/stores/server.ts';
import { getServerStatus, PACKAGE, type ServerStatus, setAutomatic } from '../api.ts';
import { isHytaleServer } from '../lib/console.ts';
import { type ExtT, useExtTranslations } from '../translations.ts';
import { errorText } from './AccountLinker.tsx';

export const serverStatusKey = (server: string) => [PACKAGE, 'server', server] as const;

/** "your own Hytale account", "the owner's Hytale account" or "the host account". */
export function accountName(t: ExtT, status: ServerStatus, source: 'user' | 'host'): string {
  if (source === 'host') return t('source.host', {});
  return status.is_owner ? t('source.user', {}) : t('source.userOther', {});
}

/** On the server settings page of Hytale servers: the automatic login switch and its account. */
export default function ServerSettingsCard() {
  const isHytale = useServerStore((state) => isHytaleServer(state.server));
  return isHytale ? <HytaleSettingsCard /> : null;
}

function HytaleSettingsCard() {
  const { t } = useExtTranslations();
  const { addToast } = useToast();
  const queryClient = useQueryClient();
  const serverUuid = useServerStore((state) => state.server.uuid);
  const query = useQuery({ queryKey: serverStatusKey(serverUuid), queryFn: () => getServerStatus(serverUuid) });
  const [saving, setSaving] = useState(false);
  const status = query.data;

  const toggle = async (automatic: boolean) => {
    setSaving(true);
    try {
      await setAutomatic(serverUuid, automatic);
      await queryClient.invalidateQueries({ queryKey: serverStatusKey(serverUuid) });
      addToast(t('server.saved', {}), 'success');
    } catch (error) {
      addToast(httpErrorToHuman(error), 'error');
    } finally {
      setSaving(false);
    }
  };

  return (
    <TitleCard title={t('server.title', {})} icon={<FontAwesomeIcon icon={faKey} />} className='h-full order-35'>
      {query.error ? (
        <Alert color='red'>{httpErrorToHuman(query.error)}</Alert>
      ) : !status?.hytale ? null : (
        <Stack gap='sm'>
          <Switch
            label={t('server.automatic', {})}
            description={t('server.automaticDescription', {})}
            checked={status.automatic}
            disabled={!status.can_manage || saving}
            onChange={(event) => toggle(event.currentTarget.checked)}
          />
          <Text size='sm'>
            {!status.automatic
              ? t('server.off', {})
              : status.next
                ? status.next.profile_name
                  ? t('server.nextProfile', {
                      account: accountName(t, status, status.next.source),
                      profile: status.next.profile_name,
                    })
                  : t('server.next', { account: accountName(t, status, status.next.source) })
                : t('server.noAccount', {})}
          </Text>
          {status.own_problem?.error ? (
            <Alert color='yellow' icon={<FontAwesomeIcon icon={faTriangleExclamation} />}>
              {`${t('source.user', {})}: ${errorText(t, status.own_problem.error.code, status.own_problem.error.message)}`}
            </Alert>
          ) : null}
          {status.last?.error ? (
            <Alert color='yellow' icon={<FontAwesomeIcon icon={faTriangleExclamation} />}>
              {errorText(t, status.last.error, status.last.error_message)}
            </Alert>
          ) : status.last?.minted && status.last.profile_name ? (
            <Text size='sm' c='dimmed'>
              {t('server.lastOk', { profile: status.last.profile_name })}
            </Text>
          ) : null}
          {status.is_owner && status.user_accounts ? (
            <div>
              <MantineButton component={Link} to='/account/hytale' variant='default' size='xs'>
                {status.owner_linked ? t('account.title', {}) : t('server.linkOwn', {})}
              </MantineButton>
            </div>
          ) : null}
        </Stack>
      )}
    </TitleCard>
  );
}
