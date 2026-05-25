import { Card, Stack } from '@junius/design';
import { useState } from 'react';

import { VenuePicker } from '../../lib/VenuePicker.js';

export function WidgetsPage() {
  const [venue, setVenue] = useState('');
  return (
    <Stack gap="md">
      <Card>
        <Stack gap="sm">
          <h1 className="text-xl font-semibold">Widgets</h1>
          <p className="text-fg-2 text-sm">
            This UI-only plugin exposes reusable components. Below is its <code>VenuePicker</code>,
            which the <code>greetings</code> plugin consumes via the component registry.
          </p>
        </Stack>
      </Card>
      <Card>
        <Stack gap="sm">
          <VenuePicker value={venue} onChange={setVenue} />
          <p className="text-fg-2 text-sm">Selected: {venue || '(none)'}</p>
        </Stack>
      </Card>
    </Stack>
  );
}
