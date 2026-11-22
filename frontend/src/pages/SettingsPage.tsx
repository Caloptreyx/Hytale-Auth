import { faGear, faServer } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { httpErrorToHuman } from '@/api/axios.ts';
import Button from '@/elements/buttons/Button.tsx';
import TitleCard from '@/elements/data-display/TitleCard.tsx';
import Alert from '@/elements/feedback/Alert.tsx';
import Spinner from '@/elements/feedback/Spinner.tsx';
import Switch from '@/elements/input/Switch.tsx';
import Group from '@/elements/layout/Group.tsx';
import Stack from '@/elements/layout/Stack.tsx';
import Text from '@/elements/typography/Text.tsx';
import { useAdminCan } from '@/plugins/usePermissions.ts';
import { useToast } from '@/providers/ToastProvider.tsx';
import { getSettings, hostAccount, PACKAGE, type Settings, updateSettings } from '../api.ts';
import AccountLinker from '../components/AccountLinker.tsx';
import { useExtTranslations } from '../translations.ts';

const settingsKey = [PACKAGE, 'settings'] as const;
const hostKey = [PACKAGE, 'host'] as const;

/** The extension's admin page: the host account and the settings. */
export default function SettingsPage() {
  const { t } = useExtTranslations();
  const canManage = useAdminCan('extensions.manage');

  return (
    <Stack gap='md'>
      <TitleCard title={t('host.title', {})} icon={<FontAwesomeIcon icon={faServer} />}>
        <Stack gap='md'>
          <Text size='sm' c='dimmed'>
            {t('host.description', {})}
          </Text>
          <AccountLinker api={hostAccount} queryKey={hostKey} canManage={canManage} />
        </Stack>
      </TitleCard>
      <SettingsCard canManage={canManage} />
    </Stack>
  );
}

function SettingsCard({ canManage }: { canManage: boolean }) {
  const { t } = useExtTranslations();
  const { addToast } = useToast();
  const queryClient = useQueryClient();
  const query = useQuery({ queryKey: settingsKey, queryFn: getSettings });
  const [form, setForm] = useState<Settings | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (query.data) setForm(query.data);
  }, [query.data]);

  const save = async () => {
    if (!form) return;
    setSaving(true);
    try {
      const saved = await updateSettings(form);
      setForm(saved);
      queryClient.setQueryData(settingsKey, saved);
      addToast(t('settings.saved', {}), 'success');
    } catch (error) {
      addToast(httpErrorToHuman(error), 'error');
    } finally {
      setSaving(false);
    }
  };

  return (
    <TitleCard title={t('settings.title', {})} icon={<FontAwesomeIcon icon={faGear} />}>
      {query.isLoading ? (
        <Group justify='center' py='md'>
          <Spinner />
        </Group>
      ) : query.error || !form ? (
        <Alert color='red'>{httpErrorToHuman(query.error)}</Alert>
      ) : (
        <Stack gap='md'>
          <Switch
            label={t('settings.hostAccount', {})}
            description={t('settings.hostAccountDescription', {})}
            checked={form.host_account}
            onChange={(event) => setForm({ ...form, host_account: event.currentTarget.checked })}
            disabled={!canManage}
          />
          <Switch
            label={t('settings.userAccounts', {})}
            description={t('settings.userAccountsDescription', {})}
            checked={form.user_accounts}
            onChange={(event) => setForm({ ...form, user_accounts: event.currentTarget.checked })}
            disabled={!canManage}
          />
          {canManage ? (
            <Group>
              <Button loading={saving} onClick={save}>
                {t('settings.save', {})}
              </Button>
            </Group>
          ) : null}
        </Stack>
      )}
    </TitleCard>
  );
}
