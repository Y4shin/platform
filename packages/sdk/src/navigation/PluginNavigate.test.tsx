import { render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const navigateSpy = vi.fn();

vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => navigateSpy,
  // PluginLink renders a plain `<a>` in tests — fine for asserting `to`/`params`
  // are passed through; the real `Link` requires a live router context.
  Link: ({ to, params, children, ...rest }: Record<string, unknown> & { children?: unknown }) => (
    <a data-testid="link" data-to={String(to)} data-params={JSON.stringify(params)} {...rest}>
      {children as React.ReactNode}
    </a>
  ),
}));

import { PluginLink, usePluginNavigate } from './PluginNavigate.js';

function NavProbe({ to, params }: { to: string; params?: Record<string, string> }) {
  const navigate = usePluginNavigate();
  return (
    <button type="button" onClick={() => navigate(to, params)}>
      go
    </button>
  );
}

beforeEach(() => {
  navigateSpy.mockClear();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('usePluginNavigate', () => {
  it('forwards a raw path + params to useNavigate', () => {
    render(<NavProbe to="/p/events/$eventId" params={{ eventId: 'abc' }} />);
    screen.getByRole('button').click();
    expect(navigateSpy).toHaveBeenCalledTimes(1);
    expect(navigateSpy).toHaveBeenCalledWith({
      to: '/p/events/$eventId',
      params: { eventId: 'abc' },
    });
  });

  it('omits params when none are supplied', () => {
    render(<NavProbe to="/p/events" />);
    screen.getByRole('button').click();
    expect(navigateSpy).toHaveBeenCalledWith({ to: '/p/events', params: undefined });
  });
});

describe('PluginLink', () => {
  it('passes raw `to` + `params` through to the underlying Link', () => {
    render(
      <PluginLink to="/p/events/$eventId" params={{ eventId: 'abc' }}>
        view
      </PluginLink>,
    );
    const link = screen.getByTestId('link');
    expect(link.dataset.to).toBe('/p/events/$eventId');
    expect(link.dataset.params).toBe(JSON.stringify({ eventId: 'abc' }));
    expect(link.textContent).toBe('view');
  });
});
