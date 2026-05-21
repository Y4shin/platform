import { AuthProvider, ComponentRegistryProvider } from '@junius/sdk';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { createRouter, RouterProvider } from '@tanstack/react-router';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { componentRegistry } from './generated/component-registry.js';
import { routeTree } from './generated/routes.js';

import './styles.css';

const queryClient = new QueryClient();
const router = createRouter({ routeTree });

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router;
  }
}

const container = document.getElementById('root');
if (!container) {
  throw new Error('root element missing from index.html');
}

createRoot(container).render(
  <StrictMode>
    <AuthProvider>
      <QueryClientProvider client={queryClient}>
        <ComponentRegistryProvider registry={componentRegistry}>
          <RouterProvider router={router} />
        </ComponentRegistryProvider>
      </QueryClientProvider>
    </AuthProvider>
  </StrictMode>,
);
