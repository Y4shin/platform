import { Card } from '@junius/design';

export function NotFound() {
  return (
    <Card>
      <h2 className="text-lg font-semibold">Not found</h2>
      <p className="text-fg-2 mt-2">The page you requested isn't part of any enabled plugin.</p>
    </Card>
  );
}
