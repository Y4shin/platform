# Exposed components

Plugins compose at the frontend through a **component registry**. A plugin
*exposes* React components in its manifest; other plugins *consume* them by name
through `useComponent`. This is the only sanctioned cross-plugin frontend
dependency — there's no importing another plugin's internals.

## Exposing a component

Declare it in `plugin.toml`:

```toml
[exposes.components.EventCard]
module      = "./lib/EventCard"   # relative to this plugin's frontend `src/`
description = "Compact event summary card (title, when, where, visibility)."

[exposes.components.EventPicker]
module      = "./lib/EventPicker"
description = "Searchable event selector for cross-plugin reuse."
```

`junius sync` registers each in the host component registry. Two rules:

1. Each declared component **must** be a named export of `frontend/src/index.ts`.
   `junius check`'s `FE.EXPORTS.MATCH_MANIFEST` rule fails the build otherwise.
2. The `module` path is relative to your frontend `src/`.

```ts
// frontend/src/index.ts
export { EventCard } from './lib/EventCard';
export { EventPicker } from './lib/EventPicker';
```

The exposed components live under `plugins/events/frontend/src/lib/`.

## Consuming a component

Another plugin reaches an exposed component by its registry key,
`<plugin>.<name>`, via `useComponent`:

```tsx
import { useComponent } from '@junius/sdk';

function MyPage() {
  const EventCard = useComponent('events.EventCard');
  return EventCard ? <EventCard eventId={someId} /> : null;
}
```

`useComponent` resolves through the host registry, so the consuming plugin never
imports `@junius/plugin-events` directly. If the providing plugin isn't enabled
in the deployment, the lookup returns nothing — handle the absent case.

## Design guidance

- Expose **stable, self-contained** components — a card, a picker — not pages or
  anything that assumes your plugin's routing context.
- Keep the props simple and serializable (ids, not live objects). The consumer
  shouldn't need your domain types.
- Document each exposed component's props near its definition; the manifest
  `description` is the one-liner an integrator sees first.

For cross-plugin **navigation**, there's no equivalent of `usePluginNavigate` —
deliberately. Plugins reach each other through exposed components, not by
deep-linking into one another's route trees.

That's the frontend. Next, the platform capabilities your plugin can pull in.
