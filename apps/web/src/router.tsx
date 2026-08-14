import { createRouter as createTanStackRouter } from "@tanstack/react-router"
import { routeTree } from "./routeTree.gen"

import { setupRouterSsrQueryIntegration } from "@tanstack/react-router-ssr-query"
import { getContext } from "./integrations/tanstack-query/root-provider"
import { deLocalizeUrl, localizeUrl } from "./paraglide/runtime"
import { PendingPage } from "./components/pending-page"

export function getRouter() {
  const context = getContext()

  const router = createTanStackRouter({
    routeTree,
    context,
    scrollRestoration: true,
    defaultPreload: "intent",
    defaultPreloadStaleTime: 0,
    // Переход, который не отвечает, выглядит как непринятый щелчок. Порог
    // в 150 мс отсекает мгновенные переходы (мигание заглушки хуже ее
    // отсутствия), а минимум в 300 мс не дает заглушке мелькнуть, если
    // данные пришли сразу после нее.
    defaultPendingComponent: PendingPage,
    defaultPendingMs: 150,
    defaultPendingMinMs: 300,
    // Paraglide url-стратегия: /kk/... и /en/... локализуются вне дерева маршрутов
    // (https://github.com/TanStack/router/tree/main/examples/react/i18n-paraglide)
    rewrite: {
      input: ({ url }) => deLocalizeUrl(url),
      output: ({ url }) => localizeUrl(url),
    },
  })

  setupRouterSsrQueryIntegration({ router, queryClient: context.queryClient })

  return router
}

declare module "@tanstack/react-router" {
  interface Register {
    router: ReturnType<typeof getRouter>
  }
}
