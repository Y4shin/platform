import { useMutation, useQuery } from '@connectrpc/connect-query';
import { Button, Card, Stack } from '@junius/design';
import { rpc } from '@junius/generated/greetings/rpc';
import { GreeterCard } from '@junius/plugin-hello';
import { useState } from 'react';

// Typed `useComponent`, generated from this plugin's declared dependencies'
// exposed components (see src/generated/component-registry.ts).
import { useComponent } from '../../generated/component-registry.js';

/** Stable template names (seeded by hello's migration 0003) + their bodies. */
const TEMPLATES = [
  { name: 'default', body: 'Hello, {name}!' },
  { name: 'formal', body: 'Good day, {name}.' },
];

export function GreetingFormPage() {
  const [templateName, setTemplateName] = useState('default');
  const [recipient, setRecipient] = useState('');
  const [venue, setVenue] = useState('');

  // Optional-dep component: VenuePicker when widgets is enabled, else free text.
  const VenuePicker = useComponent('widgets.VenuePicker');

  const list = useQuery(rpc.GreetingService.listGreetings, {});
  // Cross-plugin RPC (declared in [dependencies.hello].rpc_methods): hello
  // renders the server-side greeting preview.
  const preview = useQuery(rpc.HelloService.greet, { name: recipient || 'world' });
  const create = useMutation(rpc.GreetingService.createGreeting, {
    onSuccess: () => {
      setRecipient('');
      setVenue('');
      void list.refetch();
    },
  });

  const selectedBody = TEMPLATES.find((t) => t.name === templateName)?.body;

  return (
    <Stack gap="md">
      <Card>
        <Stack gap="sm">
          <h1 className="text-xl font-semibold">New greeting</h1>
          <p className="text-fg-2 text-sm">
            Built from a <code>hello</code> template (required dep) with an optional venue from{' '}
            <code>widgets</code>.
          </p>

          <label className="text-sm font-medium" htmlFor="template">
            Template
          </label>
          <select
            id="template"
            className="border-fg-3 rounded border px-2 py-1 text-sm"
            value={templateName}
            onChange={(e) => setTemplateName(e.target.value)}
          >
            {TEMPLATES.map((t) => (
              <option key={t.name} value={t.name}>
                {t.name}
              </option>
            ))}
          </select>

          <label className="text-sm font-medium" htmlFor="recipient">
            Recipient
          </label>
          <input
            id="recipient"
            className="border-fg-3 rounded border px-2 py-1 text-sm"
            value={recipient}
            onChange={(e) => setRecipient(e.target.value)}
            placeholder="alice"
          />

          {VenuePicker ? (
            <VenuePicker value={venue} onChange={setVenue} />
          ) : (
            <Stack gap="sm">
              <label className="text-sm font-medium" htmlFor="venue">
                Venue
              </label>
              <input
                id="venue"
                className="border-fg-3 rounded border px-2 py-1 text-sm"
                value={venue}
                onChange={(e) => setVenue(e.target.value)}
                placeholder="Venue (widgets disabled — free text)"
              />
            </Stack>
          )}

          <Button
            onClick={() => create.mutate({ templateName, recipient, venue })}
            disabled={!recipient || create.isPending}
          >
            Create greeting
          </Button>
        </Stack>
      </Card>

      <Card>
        <Stack gap="sm">
          <h2 className="text-base font-semibold">Preview</h2>
          <GreeterCard name={recipient || 'world'} template={selectedBody} />
          <p className="text-fg-2 text-sm">
            Server reply (hello RPC): {preview.data?.message ?? '…'}
          </p>
        </Stack>
      </Card>

      <Card>
        <Stack gap="sm">
          <h2 className="text-base font-semibold">Greetings</h2>
          {list.data?.greetings.length ? (
            <ul className="text-sm">
              {list.data.greetings.map((g) => (
                <li key={g.id}>
                  {g.message}
                  {g.venue ? ` — ${g.venue}` : ''}
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-fg-2 text-sm">No greetings yet.</p>
          )}
        </Stack>
      </Card>
    </Stack>
  );
}
