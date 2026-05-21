# 4. Frontend

## 4.1 Stack

- **React + TypeScript.**
- **TanStack Router** for routing — code-based, fully type-safe, composable.
- **TanStack Query** + `@connectrpc/connect-query` for data fetching, caching, and mutations.
- **Vite** as the build tool.
- **SPA mode** (no SSR). The platform sits behind authentication; SSR/SEO benefits don't apply.

## 4.2 Why this stack over SvelteKit / Next.js / Leptos

- **Code-based routing fits plugin composition natively.** Routes are data; each plugin exports a route subtree that `junius` concatenates. File-based routers (SvelteKit, Next App Router, Nuxt, Remix) require build-time codegen shims to compose plugins.
- **Best-in-class ecosystem for SPA-heavy UI.** Rich text editors, data grids, drag-and-drop, complex forms — all most mature on React.
- **Largest hiring pool.**
- **Connect-RPC integration is first-party** via `connect-query`.
- **Leptos rejected** for production: 0.x churn, thin component ecosystem, niche hiring.
- **SvelteKit rejected** despite better per-component DX: file-based routing fights the plugin model.

## 4.3 Plugin contract (frontend side)

Each plugin is also a pnpm workspace package (`@junius/plugin-<name>`) that exports:

```ts
// plugins/speakers/frontend/index.ts
export { routes } from './routes';              // TanStack Router subtree
export { SpeakerPicker, SpeakerCard } from './lib'; // public components
export { createSpeakersClient } from './rpc';    // Connect-RPC client factory
export type { Speaker, SpeakerId } from './types';
```
