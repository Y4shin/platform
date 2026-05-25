import { Card, Stack } from '@junius/design';

export interface GreeterCardProps {
  /** Who the greeting is addressed to. */
  name: string;
  /** Template body with a `{name}` placeholder; defaults to a plain greeting. */
  template?: string;
}

/**
 * A self-contained greeting preview. The `hello` plugin exposes this via
 * `[exposes.components.GreeterCard]`; other plugins import it directly from
 * `@junius/plugin-hello` (a required-dep ES import) or look it up at runtime
 * through the component registry (`useComponent('hello.GreeterCard')`).
 */
export function GreeterCard({ name, template = 'Hello, {name}!' }: GreeterCardProps) {
  const message = template.replace('{name}', name || 'world');
  return (
    <Card>
      <Stack gap="sm">
        <h3 className="text-base font-semibold">Greeting preview</h3>
        <p className="text-lg">{message}</p>
        <p className="text-fg-2 text-xs">
          Contributed by the <code>hello</code> plugin's <code>GreeterCard</code>.
        </p>
      </Stack>
    </Card>
  );
}
