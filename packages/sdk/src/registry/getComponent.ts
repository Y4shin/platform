import type { ComponentType } from 'react';

import { useComponentRegistry } from './ComponentRegistryProvider.js';

/**
 * Look up a cross-plugin component by its `'<plugin>.<Component>'` key. Returns
 * `undefined` if the producing plugin is disabled (optional-dep semantics) or
 * the key isn't registered; consumers should fall back to a sensible default.
 *
 * This is the **untyped** lookup. Plugins get a key-checked, precisely-typed
 * `useComponent` from their generated `src/generated/component-registry.ts`
 * (emitted by `junius sync` from their declared dependencies' exposed
 * components), which wraps this.
 */
export function useComponent(key: string): ComponentType<unknown> | undefined {
  const registry = useComponentRegistry() as Record<string, ComponentType<unknown> | undefined>;
  return registry[key];
}
