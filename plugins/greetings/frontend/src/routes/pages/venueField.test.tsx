import { VenuePicker } from '@junius/plugin-widgets';
import { ComponentRegistryProvider } from '@junius/sdk';
import { render, screen } from '@testing-library/react';
import type { ReactNode } from 'react';
import { describe, expect, it } from 'vitest';

import { useComponent } from '../../generated/component-registry.js';

// Mirrors GreetingFormPage's optional-dep venue field: render widgets' VenuePicker
// when the plugin is enabled (its component is in the registry), else a plain
// text input. This is the deterministic counterpart to the Playwright E2E.
function VenueField() {
  const Picker = useComponent('widgets.VenuePicker');
  return Picker ? <Picker /> : <input aria-label="venue" />;
}

function withRegistry(node: ReactNode, registry: Record<string, typeof VenuePicker>) {
  return render(<ComponentRegistryProvider registry={registry}>{node}</ComponentRegistryProvider>);
}

describe('greetings venue field (optional widgets dep)', () => {
  it('renders the VenuePicker when widgets is enabled', () => {
    withRegistry(<VenueField />, { 'widgets.VenuePicker': VenuePicker });
    expect(screen.getByRole('combobox')).toBeTruthy();
    expect(screen.queryByRole('textbox')).toBeNull();
  });

  it('falls back to a free-text input when widgets is disabled', () => {
    withRegistry(<VenueField />, {});
    expect(screen.getByRole('textbox')).toBeTruthy();
    expect(screen.queryByRole('combobox')).toBeNull();
  });
});
