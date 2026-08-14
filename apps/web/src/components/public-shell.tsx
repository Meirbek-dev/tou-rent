import { SiteFooter } from "@/components/site-footer"
import { SiteHeader } from "@/components/site-header"
import { cn } from "@/lib/utils"

/**
 * Каркас публичной страницы: шапка, содержимое и подвал.
 *
 * `id="main"` живет здесь, а не в каждом маршруте: ссылка «перейти
 * к содержимому» в __root.tsx ведет именно на него, и раньше ее цель
 * приходилось помнить руками на каждой странице.
 *
 * `contained={false}` отдает разметку полос странице целиком - так главная
 * делает полосы во всю ширину, а свой контейнер заводит каждая полоса.
 */
export function PublicShell({
  children,
  className,
  contained = true,
}: {
  children: React.ReactNode
  className?: string
  contained?: boolean
}) {
  return (
    <div className="flex min-h-dvh flex-col">
      <SiteHeader />
      <main
        id="main"
        className={cn(
          "flex-1",
          contained && "mx-auto w-full max-w-6xl px-4 py-10 sm:px-6",
          className
        )}
      >
        {children}
      </main>
      <SiteFooter />
    </div>
  )
}
