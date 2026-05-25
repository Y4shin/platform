/**
 * Cross-plugin component registry. A plugin exposes components via
 * `[exposes.components]` in its `plugin.toml`; `junius sync` generates the
 * runtime registry (the host's `componentRegistry`, handed to the provider here)
 * and, for each consuming plugin, a typed `useComponent` wrapper in its
 * `src/generated/component-registry.ts` keyed to the components its declared
 * dependencies expose. The SDK's bare `useComponent` (see `getComponent.ts`) is
 * the untyped lookup the wrapper builds on.
 */

import { type ComponentType, createContext, type ReactNode, useContext } from 'react';

/**
 * Runtime store the provider holds: keys → components, type-erased (the per-key
 * types live in the augmentable `ComponentRegistry` interface). `ComponentType<never>`
 * accepts any component for storage; `useComponent` casts back to the precise
 * type on lookup.
 */
export type ComponentRegistryValue = Record<string, ComponentType<never>>;

const RegistryContext = createContext<ComponentRegistryValue>({});

export interface ComponentRegistryProviderProps {
  registry?: ComponentRegistryValue;
  children: ReactNode;
}

export function ComponentRegistryProvider({
  registry = {},
  children,
}: ComponentRegistryProviderProps) {
  return <RegistryContext.Provider value={registry}>{children}</RegistryContext.Provider>;
}

export function useComponentRegistry(): ComponentRegistryValue {
  return useContext(RegistryContext);
}
