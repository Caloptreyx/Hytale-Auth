import { faKey } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { createElement } from 'react';
import { Extension, type ExtensionContext } from 'shared';
import ConsoleLoginCard from './components/ConsoleLoginCard.tsx';
import ServerSettingsCard from './components/ServerSettingsCard.tsx';
import AccountPage from './pages/AccountPage.tsx';
import SettingsPage from './pages/SettingsPage.tsx';
import { getExtTranslations } from './translations.ts';

class DevCaloptreyxHytaleAuthExtension extends Extension {
  public cardIcon = createElement(FontAwesomeIcon, { icon: faKey });
  public cardConfigurationPage: React.FC | null = SettingsPage;

  public initialize(ctx: ExtensionContext): void {
    ctx.extensionRegistry.pages.server.console.enterContainer((container) =>
      container.prependContentComponent(ConsoleLoginCard),
    );
    ctx.extensionRegistry.pages.server.settings.enterSettingContainers((containers) =>
      containers.appendComponent(ServerSettingsCard),
    );
    ctx.extensionRegistry.enterRoutes((routes) =>
      routes.addAccountRoute({
        name: () => getExtTranslations().t('account.title', {}),
        icon: faKey,
        path: '/hytale',
        element: AccountPage,
        // the backend's `user_accounts` setting
        filter: () => this.getIndexVariable<boolean>('userAccounts') !== false,
      }),
    );
    ctx.extensionRegistry.enterPermissionIcons((icons) =>
      icons.addServerPermissionIcon('hytale-auth', createElement(FontAwesomeIcon, { icon: faKey })),
    );
  }
}

export default new DevCaloptreyxHytaleAuthExtension();
