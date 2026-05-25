import type { ComponentRegistry } from './ComponentRegistryProvider.js';
import { useComponentRegistry } from './ComponentRegistryProvider.js';

/**
 * Look up a cross-plugin component by its `'<plugin>.<Component>'` key. Returns
 * `undefined` if the producing plugin is disabled (optional-dep semantics) or
 * the key isn't registered; consumers should fall back to a sensible default.
 *
 * The key is constrained to `keyof ComponentRegistry`, which `junius sync`
 * augments per consumer from its declared dependencies' `[exposes.components]`
 * — so a typo or an undeclared cross-plugin component is a compile error, and
 * the return type is the exact component type.
 */
export function useComponent<K extends keyof ComponentRegistry>(
  key: K,
): ComponentRegistry[K] | undefined {
  const registry = useComponentRegistry() as Record<string, unknown>;
  return registry[key as string] as ComponentRegistry[K] | undefined;
}
