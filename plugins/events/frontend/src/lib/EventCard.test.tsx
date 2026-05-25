import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { EventCard } from './EventCard.js';

describe('EventCard', () => {
  it('renders the title, location, and visibility badge', () => {
    render(
      <EventCard
        event={{
          title: 'Launch party',
          startsAt: '2026-06-01T18:00:00Z',
          location: 'HQ',
          visibility: 'public',
        }}
      />,
    );
    expect(screen.getByText('Launch party')).toBeTruthy();
    expect(screen.getByText('public')).toBeTruthy();
    expect(screen.getByText(/HQ/)).toBeTruthy();
  });

  it('fires onSelect when clicked, and is inert without one', () => {
    const onSelect = vi.fn();
    const { rerender } = render(
      <EventCard event={{ title: 'X', startsAt: '' }} onSelect={onSelect} />,
    );
    fireEvent.click(screen.getByRole('button'));
    expect(onSelect).toHaveBeenCalledTimes(1);

    rerender(<EventCard event={{ title: 'X', startsAt: '' }} />);
    expect(screen.getByRole('button')).toHaveProperty('disabled', true);
  });
});
