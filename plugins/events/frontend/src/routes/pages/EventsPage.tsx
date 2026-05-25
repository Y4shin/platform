import { Card, Stack } from '@junius/design';

/**
 * Placeholder events page (Stage 1). The real list/detail/edit + invite pages
 * land in Stage 9, once the RPC surface and the forms library exist.
 */
export function EventsPage() {
  return (
    <Stack gap="md">
      <Card>
        <Stack gap="sm">
          <h1 className="text-xl font-semibold">Events</h1>
          <p className="text-fg-2 text-sm">
            The events plugin is wired in. Event management lands in a later stage.
          </p>
        </Stack>
      </Card>
    </Stack>
  );
}
