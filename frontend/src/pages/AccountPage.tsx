import { faKey } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import AccountContentContainer from '@/elements/containers/AccountContentContainer.tsx';
import TitleCard from '@/elements/data-display/TitleCard.tsx';
import Stack from '@/elements/layout/Stack.tsx';
import Text from '@/elements/typography/Text.tsx';
import { ownAccount, PACKAGE } from '../api.ts';
import AccountLinker from '../components/AccountLinker.tsx';
import { useExtTranslations } from '../translations.ts';

const OWN_ACCOUNT_KEY = [PACKAGE, 'own-account'] as const;

/** Linking the user's own Hytale account, for the servers they own. */
export default function AccountPage() {
  const { t } = useExtTranslations();

  return (
    <AccountContentContainer title={t('account.title', {})}>
      <TitleCard title={t('account.title', {})} icon={<FontAwesomeIcon icon={faKey} />}>
        <Stack gap='md'>
          <Text size='sm' c='dimmed'>
            {t('account.description', {})}
          </Text>
          <AccountLinker
            api={ownAccount}
            queryKey={OWN_ACCOUNT_KEY}
            canManage
            footer={(overview) =>
              overview.enabled === false ? (
                <Text size='sm' c='red'>
                  {t('account.disabled', {})}
                </Text>
              ) : overview.host_account && !overview.account ? (
                <Text size='sm' c='dimmed'>
                  {t('account.hostFallback', {})}
                </Text>
              ) : null
            }
          />
        </Stack>
      </TitleCard>
    </AccountContentContainer>
  );
}
