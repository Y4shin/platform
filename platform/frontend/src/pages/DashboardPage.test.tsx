import { i18n } from '@lingui/core';
import { I18nProvider } from '@lingui/react';
import { render, screen } from '@testing-library/react';
import { beforeAll, describe, expect, it } from 'vitest';

import type { MeResponse } from '../api/me.js';
import { DashboardView } from './DashboardPage.js';

// Activate an empty English catalog so Lingui renders each `<Trans>`'s default
// (source) message — making the rendered copy assertable without a compiled
// catalog.
beforeAll(() => {
  i18n.load('en', {});
  i18n.activate('en');
});

function renderView(me: MeResponse) {
  render(
    <I18nProvider i18n={i18n}>
      <DashboardView me={me} />
    </I18nProvider>,
  );
}

const base: MeResponse = {
  id: 'u1',
  email: 'alice@example.org',
  displayName: 'Alice',
  locale: null,
  oidcSub: 'sub-1',
  memberships: [],
  userRoles: [],
  sessions: [],
  adminContactEmail: null,
};

const membership: MeResponse['memberships'][number] = {
  groupId: 'g1',
  groupName: 'Committee',
  role: { id: 'r1', name: 'chair' },
  permissions: ['speakers:read'],
  managedBy: 'manual',
};

describe('DashboardView', () => {
  it('greets a user who holds a group membership (no empty state)', () => {
    renderView({ ...base, memberships: [membership] });
    expect(screen.getByRole('heading', { name: /Welcome back, Alice/ })).toBeTruthy();
    expect(screen.queryByText(/have access yet/i)).toBeNull();
  });

  it('greets a user who holds only a user-role (no empty state)', () => {
    renderView({ ...base, userRoles: [{ roleId: 'r1', roleName: 'admin', permissions: ['*'] }] });
    expect(screen.getByRole('heading', { name: /Welcome back, Alice/ })).toBeTruthy();
    expect(screen.queryByText(/have access yet/i)).toBeNull();
  });

  it('shows the no-access empty state naming the admin contact when set', () => {
    renderView({ ...base, adminContactEmail: 'ops@junius.local' });
    expect(screen.getByRole('heading', { name: /have access yet/i })).toBeTruthy();
    expect(screen.getByText('ops@junius.local')).toBeTruthy();
    // The mailto link points at the configured address.
    expect(screen.getByRole('link').getAttribute('href')).toBe('mailto:ops@junius.local');
    expect(screen.queryByRole('heading', { name: /Welcome back/ })).toBeNull();
  });

  it('degrades gracefully when the admin contact is unset', () => {
    renderView({ ...base, adminContactEmail: null });
    // Empty state still renders…
    expect(screen.getByRole('heading', { name: /have access yet/i })).toBeTruthy();
    // …but with no contact line, no link, and no `null` leaking into the DOM.
    expect(screen.queryByText(/Contact your administrator/i)).toBeNull();
    expect(screen.queryByRole('link')).toBeNull();
    expect(screen.queryByText(/null/)).toBeNull();
  });
});
