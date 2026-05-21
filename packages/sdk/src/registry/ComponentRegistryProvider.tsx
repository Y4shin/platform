/**
 * Cross-plugin component registry. At M04 the registry is always empty —
 * `getComponent(key)` returns `undefined` for every key. M09 wires up real
 * cross-plugin component sharing and `junius sync` populates the registry
 * from each plugin's `[exposes.components]` manifest section.
 */

import { type ComponentType, createContext, type ReactNode, useContext } from 'react';

export type ComponentRegistry = Record<string, ComponentType<unknown>>;

const RegistryContext = createContext<ComponentRegistry>({});

export interface ComponentRegistryProviderProps {
  registry?: ComponentRegistry;
  children: ReactNode;
}

export function ComponentRegistryProvider({
  registry = {},
  children,
}: ComponentRegistryProviderProps) {
  return <RegistryContext.Provider value={registry}>{children}</RegistryContext.Provider>;
}

export function useComponentRegistry(): ComponentRegistry {
  return useContext(RegistryContext);
}
