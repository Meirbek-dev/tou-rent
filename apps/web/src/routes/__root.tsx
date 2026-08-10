import {
  HeadContent,
  Link,
  Scripts,
  createRootRouteWithContext,
} from "@tanstack/react-router"
import { Suspense, lazy } from "react"

import PostHogProvider from "../integrations/posthog/provider"

import { getLocale } from "#/paraglide/runtime"
import * as m from "#/paraglide/messages"
import { themeInitializer } from "@/lib/theme"

import faviconUrl from "../../favicon.ico?url"
import appCss from "../styles/globals.css?url"

import type { QueryClient } from "@tanstack/react-query"

// Инструменты разработчика грузятся только в дев-режиме: `import.meta.env.DEV`
// - статическая константа сборки, в проде ветка отсекается вместе с
// динамическим импортом (см. `@/components/devtools`).
const Devtools = import.meta.env.DEV
  ? lazy(() => import("@/components/devtools"))
  : () => null

interface MyRouterContext {
  queryClient: QueryClient
}

export const Route = createRootRouteWithContext<MyRouterContext>()({
  beforeLoad: async () => {
    // Other redirect strategies are possible; see
    // https://github.com/TanStack/router/tree/main/examples/react/i18n-paraglide#offline-redirect
    if (typeof document !== "undefined") {
      document.documentElement.setAttribute("lang", getLocale())
    }
  },

  head: () => ({
    meta: [
      {
        charSet: "utf-8",
      },
      {
        name: "viewport",
        content: "width=device-width, initial-scale=1",
      },
      {
        title: "ToU Rent",
      },
    ],
    links: [
      {
        rel: "icon",
        href: faviconUrl,
        sizes: "any",
      },
      {
        rel: "stylesheet",
        href: appCss,
      },
    ],
  }),
  notFoundComponent: NotFoundPage,
  errorComponent: ErrorPage,
  shellComponent: RootDocument,
})

/**
 * Отказ загрузчика или отрисовки на любом маршруте (NFR-15).
 *
 * Без своего `errorComponent` роутер показывает служебный экран: английский
 * текст и стек посреди русского портала. Для публичной части это еще
 * и утечка - в сообщении бывает адрес запроса и ответ api, поэтому наружу
 * идет только приглашение повторить, а подробности остаются в консоли
 * и в Sentry (событие туда отправляет сам роутер).
 */
function ErrorPage({ reset }: { error: Error; reset: () => void }) {
  return (
    <main className="mx-auto flex min-h-[70vh] max-w-3xl flex-col items-center justify-center gap-4 px-4 text-center">
      <h1 className="text-3xl font-bold tracking-tight">
        {m.page_error_title()}
      </h1>
      <p className="max-w-xl text-muted-foreground">{m.page_error_text()}</p>
      <div className="flex flex-wrap items-center justify-center gap-3">
        <button
          type="button"
          onClick={reset}
          className="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90"
        >
          {m.page_error_retry()}
        </button>
        <Link
          to="/"
          className="rounded-md border px-4 py-2 text-sm font-medium hover:bg-accent"
        >
          {m.nav_home()}
        </Link>
      </div>
    </main>
  )
}

function NotFoundPage() {
  return (
    <main className="mx-auto flex min-h-[70vh] max-w-3xl flex-col items-center justify-center gap-4 px-4 text-center">
      <p className="text-sm font-semibold text-primary">404</p>
      <h1 className="text-3xl font-bold tracking-tight">
        {m.page_not_found_title()}
      </h1>
      <p className="max-w-xl text-muted-foreground">
        {m.page_not_found_text()}
      </p>
      <Link
        to="/"
        className="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90"
      >
        {m.nav_home()}
      </Link>
    </main>
  )
}

function RootDocument({ children }: { children: React.ReactNode }) {
  return (
    <html lang={getLocale()} suppressHydrationWarning>
      <head>
        <script dangerouslySetInnerHTML={{ __html: themeInitializer }} />
        <HeadContent />
      </head>
      <body>
        <PostHogProvider>
          {children}
          <Suspense fallback={null}>
            <Devtools />
          </Suspense>
        </PostHogProvider>
        <Scripts />
      </body>
    </html>
  )
}
