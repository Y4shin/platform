import { i18n } from '@lingui/core';
import { I18nProvider } from '@lingui/react';
import { render, screen } from '@testing-library/react';
import { beforeAll, describe, expect, it } from 'vitest';

import { ForbiddenView } from './ForbiddenPage.js';

// Activate an empty English catalog so Lingui renders each `<Trans>`'s source
// message, making the rendered copy assertable without a compiled catalog.
beforeAll(() => {
  i18n.load('en', {});
  i18n.activate('en');
});

function renderView(props: { missing: string[]; adminContactEmail: string | null }) {
  render(
    <I18nProvider i18n={i18n}>
      <ForbiddenView {...props} />
    </I18nProvider>,
  );
}

describe('ForbiddenView', () => {
  it('names the missing permission(s) when known', () => {
    renderView({ missing: ['events:write'], adminContactEmail: null });
    expect(screen.getByText(/missing the permission/i)).toBeTruthy();
    expect(screen.getByText('events:write')).toBeTruthy();
    // The generic fallback copy must NOT show when names are known.
    expect(screen.queryByText(/don't have permission to view this page/i)).toBeNull();
  });

  it('falls back to generic copy when no permissions are named', () => {
    renderView({ missing: [], adminContactEmail: null });
    expect(screen.getByText(/don't have permission to view this page/i)).toBeTruthy();
  });

  it('shows the admin contact when configured', () => {
    render(
      <I18nProvider i18n={i18n}>
        <ForbiddenView missing={['events:write']} adminContactEmail="ops@junius.local" />
      </I18nProvider>,
    );
    expect(screen.getByText('ops@junius.local')).toBeTruthy();
    expect(screen.getByRole('link').getAttribute('href')).toBe('mailto:ops@junius.local');
  });

  it('omits the contact line when the admin contact is unset', () => {
    renderView({ missing: ['events:write'], adminContactEmail: null });
    expect(screen.queryByText(/Contact your administrator/i)).toBeNull();
    expect(screen.queryByRole('link')).toBeNull();
  });
});
