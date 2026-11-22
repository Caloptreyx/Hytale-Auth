import { faKey } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { createElement } from 'react';
import { Extension, type ExtensionContext } from 'shared';
import ConsoleLoginCard from './components/ConsoleLoginCard.tsx';

class DevCaloptreyxHytaleAuthExtension extends Extension {
  public cardIcon = createElement(FontAwesomeIcon, { icon: faKey });

  public initialize(ctx: ExtensionContext): void {
    ctx.extensionRegistry.pages.server.console.enterContainer((container) =>
      container.prependContentComponent(ConsoleLoginCard),
    );
  }
}

export default new DevCaloptreyxHytaleAuthExtension();
