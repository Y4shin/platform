import { type NavigateOptions, useNavigate } from '@tanstack/react-router';

/**
 * Navigate to one of this plugin's own routes by raw path.
 *
 * The composed route tree erases plugin sub-route types (`buildRoutes` returns
 * `AnyRoute[]`), so the typed router doesn't know paths like `/p/events/$eventId`
 * — a typed `<Link to=…>` / `navigate({ to })` to them won't compile. This thin
 * wrapper localizes the (single) escape hatch. (See the friction log.)
 */
export function usePluginNavigate() {
  const navigate = useNavigate();
  return (to: string, params?: Record<string, string>) => {
    void navigate({ to, params } as unknown as NavigateOptions);
  };
}
