import { Link, type LinkProps, type NavigateOptions, useNavigate } from '@tanstack/react-router';

/**
 * Navigate to one of this plugin's own routes by raw path (M16 item D).
 *
 * Why this exists: `buildRoutes` returns `AnyRoute[]`, so the composed route
 * tree loses child paths' types — a typed `navigate({ to: '/p/<plugin>/$id' })`
 * to a plugin's own sub-route won't compile (only the top-level parent
 * `/p/<plugin>` is known). This hook localizes the (single) escape hatch.
 *
 * Pass the raw path string + any path-param record; the helper coerces both
 * past the typed router's check. Internal to the calling plugin — never use it
 * for cross-plugin navigation (the dependency surface there is the manifest
 * `[exposes.components]` + `useComponent`).
 */
export function usePluginNavigate() {
  const navigate = useNavigate();
  return (to: string, params?: Record<string, string>) => {
    void navigate({ to, params } as unknown as NavigateOptions);
  };
}

export type PluginLinkProps = Omit<LinkProps, 'to' | 'params'> & {
  to: string;
  params?: Record<string, string>;
};

/**
 * `<Link>` for one of this plugin's own routes. Same rationale as
 * [`usePluginNavigate`]: takes the raw path string + params and coerces past
 * the typed router's narrowing. Use anywhere a `to="/p/<plugin>/$id"` link
 * doesn't typecheck.
 */
export function PluginLink(props: PluginLinkProps) {
  // Same escape hatch as `usePluginNavigate`: the typed router doesn't know
  // plugin sub-route paths, so we coerce `to`/`params` past its narrowing and
  // let the runtime route tree resolve them. Single cast through `LinkProps`
  // is enough; the rest of the props pass through unchanged.
  return <Link {...(props as unknown as LinkProps)} />;
}
