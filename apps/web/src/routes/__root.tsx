import {
  HeadContent,
  Link,
  Scripts,
  createRootRouteWithContext,
} from "@tanstack/react-router"
import { Suspense, lazy, useSyncExternalStore } from "react"

import PostHogProvider from "../integrations/posthog/provider"

import {
  baseLocale,
  getLocale,
  localizeHref,
  locales,
} from "#/paraglide/runtime"
import * as m from "#/paraglide/messages"
import { buttonVariants } from "@/components/ui/button"
import {
  isStaleChunkError,
  markReloadTried,
  reloadTriedRecently,
} from "@/lib/stale-chunk"
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

const subscribeToNothing = () => () => undefined

export const Route = createRootRouteWithContext<MyRouterContext>()({
  head: ({ matches }) => {
    // Локаль живет в пути (стратегия "url": /kk/..., /en/..., ru без
    // префикса), поэтому у каждой страницы есть три различимых адреса -
    // и поисковику надо сказать, что это одна и та же страница.
    const pathname = matches.at(-1)?.pathname ?? "/"

    return {
      meta: [
        {
          charSet: "utf-8",
        },
        {
          name: "viewport",
          content: "width=device-width, initial-scale=1",
        },
        // Цвет служебных полос браузера повторяет --background обеих тем
        {
          name: "theme-color",
          media: "(prefers-color-scheme: light)",
          content: "#ffffff",
        },
        {
          name: "theme-color",
          media: "(prefers-color-scheme: dark)",
          content: "#161616",
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
        ...locales.map((locale) => ({
          rel: "alternate",
          hrefLang: locale,
          href: localizeHref(pathname, { locale }),
        })),
        {
          rel: "alternate",
          hrefLang: "x-default",
          href: localizeHref(pathname, { locale: baseLocale }),
        },
      ],
    }
  },
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
 *
 * Отдельная ветка - рассинхрон манифеста: вкладка открыта до выката, SSR
 * отдает целую страницу, а гидратация падает на импорте исчезнувшего чанка.
 * `reset()` там повторяет тот же импорт того же несуществующего файла, и
 * экран не менялся никогда. Помогает только полная перезагрузка - она берет
 * свежий манифест; ссылка «Главная» на этом экране тоже обычная, потому что
 * переход роутером уперся бы в тот же манифест.
 */
function ErrorPage({ error, reset }: { error: Error; reset: () => void }) {
  const stale = isStaleChunkError(error)
  // Серверный снимок всегда false; после гидратации React перечитает
  // sessionStorage как внешний браузерный источник, не меняя state в effect.
  const reloadFailed = useSyncExternalStore(
    subscribeToNothing,
    () => stale && reloadTriedRecently(),
    () => false
  )

  const onRetry = () => {
    if (!stale) {
      reset()
      return
    }
    markReloadTried()
    window.location.reload()
  }

  return (
    <main
      id="main"
      className="mx-auto flex min-h-[70vh] max-w-3xl flex-col items-center justify-center gap-4 px-4 text-center"
    >
      <h1 className="text-3xl font-bold tracking-tight">
        {m.page_error_title()}
      </h1>
      <p className="max-w-xl text-muted-foreground">
        {stale ? m.page_error_stale_text() : m.page_error_text()}
      </p>
      <div className="flex flex-wrap items-center justify-center gap-3">
        {reloadFailed ? (
          <p className="max-w-xl font-medium">{m.page_error_reload_manual()}</p>
        ) : (
          <button
            type="button"
            onClick={onRetry}
            className={buttonVariants({ size: "lg" })}
          >
            {stale ? m.page_error_reload() : m.page_error_retry()}
          </button>
        )}
        {stale ? (
          <a
            href={localizeHref("/")}
            className={buttonVariants({ variant: "outline" })}
          >
            {m.nav_home()}
          </a>
        ) : (
          <Link to="/" className={buttonVariants({ variant: "outline" })}>
            {m.nav_home()}
          </Link>
        )}
      </div>
    </main>
  )
}

function NotFoundPage() {
  return (
    <main
      id="main"
      className="mx-auto flex min-h-[70vh] max-w-3xl flex-col items-center justify-center gap-4 px-4 text-center"
    >
      <p className="text-sm font-semibold text-primary tabular-nums">404</p>
      <h1 className="text-3xl font-bold tracking-tight">
        {m.page_not_found_title()}
      </h1>
      <p className="max-w-xl text-muted-foreground">
        {m.page_not_found_text()}
      </p>
      <Link to="/" className={buttonVariants({ size: "lg" })}>
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
        {/* Первая точка табуляции страницы: клавиатурному пользователю не
            приходится проходить всю шапку ради содержимого (SC 2.4.1) */}
        <a
          href="#main"
          className="sr-only focus:not-sr-only focus:fixed focus:top-2 focus:left-2 focus:z-50 focus:rounded-lg focus:bg-primary focus:px-4 focus:py-2 focus:text-sm focus:font-medium focus:text-primary-foreground focus:shadow-lg focus:outline-2 focus:outline-offset-2 focus:outline-ring"
        >
          {m.skip_to_content()}
        </a>
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
