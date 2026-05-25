import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { VENUES, VenuePicker } from './VenuePicker.js';

describe('VenuePicker', () => {
  it('renders an option per curated venue plus the placeholder', () => {
    render(<VenuePicker />);
    const options = screen.getAllByRole('option');
    // One placeholder + one per venue.
    expect(options).toHaveLength(VENUES.length + 1);
    for (const venue of VENUES) {
      expect(screen.getByRole('option', { name: venue })).toBeTruthy();
    }
  });

  it('reports the selected venue via onChange', () => {
    const onChange = vi.fn();
    render(<VenuePicker onChange={onChange} />);
    const select = screen.getByRole('combobox') as HTMLSelectElement;
    select.value = 'Rooftop Lounge';
    select.dispatchEvent(new Event('change', { bubbles: true }));
    expect(onChange).toHaveBeenCalledWith('Rooftop Lounge');
  });
});
