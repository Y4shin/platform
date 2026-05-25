/**
 * Cross-plugin component registry. A plugin exposes components via
 * `[exposes.components]` in its `plugin.toml`; `junius sync` generates the
 * runtime registry (the host's `componentRegistry`) and, for each consuming
 * plugin, a `declare module '@junius/sdk'` augmentation of [`ComponentRegistry`]
 * so `useComponent` is keyed and typed per consumer.
 */

import { type ComponentType, createContext, type ReactNode, useContext } from 'react';

/**
 * Typed map of `'<plugin>.<Component>'` → that component's React type. Empty by
 * default and **augmented** per consumer by `junius sync` (declaration merging),
 * so `keyof ComponentRegistry` lists exactly the components a consumer's declared
 * dependencies expose. Drives `useComponent`'s key checking and return type.
 */
// biome-ignore lint/suspicious/noEmptyInterface: open for cross-package augmentation.
export interface ComponentRegistry {}

/**
 * Runtime store the provider holds: keys → components, type-erased (the per-key
 * types live in [`ComponentRegistry`]). `ComponentType<never>` accepts any
 * component for storage; `useComponent` casts back to the precise type on lookup.
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
