import { Link } from '@tanstack/react-router';

/**
 * Top-level nav. At M04 the entries are static; M07 derives this from the
 * composed route tree + permission gates.
 */
export function NavLinks() {
  return (
    <nav className="flex items-center gap-4 text-sm">
      <Link to="/" className="hover:text-primary">
        Home
      </Link>
      <Link to="/p/events" className="hover:text-primary">
        Events
      </Link>
    </nav>
  );
}
