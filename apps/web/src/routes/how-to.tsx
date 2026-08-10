import { Link, createFileRoute } from "@tanstack/react-router"
import { m } from "#/paraglide/messages"
import { SiteHeader } from "@/components/site-header"
import { buttonVariants } from "@/components/ui/button"
import { cn } from "@/lib/utils"

// FR-1401: статичная страница «как участвовать» (п. 5–6)
export const Route = createFileRoute("/how-to")({
  head: () => ({ meta: [{ title: `${m.howto_title()} - ToU Rent` }] }),
  component: HowToPage,
})

const STEPS: (() => { title: string; text: string })[] = [
  () => ({
    title: m.howto_step_register_title(),
    text: m.howto_step_register_text(),
  }),
  () => ({
    title: m.howto_step_prepare_title(),
    text: m.howto_step_prepare_text(),
  }),
  () => ({
    title: m.howto_step_submit_title(),
    text: m.howto_step_submit_text(),
  }),
  () => ({
    title: m.howto_step_trade_title(),
    text: m.howto_step_trade_text(),
  }),
]

function HowToPage() {
  return (
    <>
      <SiteHeader />
      <main className="mx-auto flex w-full max-w-3xl flex-col gap-8 px-6 py-10">
        <header className="flex flex-col gap-3">
          <h1 className="font-heading text-3xl font-semibold tracking-tight">
            {m.howto_title()}
          </h1>
          <p className="text-lg text-muted-foreground">{m.howto_intro()}</p>
        </header>

        <ol className="flex list-none flex-col gap-6">
          {STEPS.map((step, index) => {
            const { title, text } = step()
            return (
              <li key={title} className="flex gap-4">
                <span
                  aria-hidden="true"
                  className="flex size-9 shrink-0 items-center justify-center rounded-full bg-primary font-heading font-semibold text-primary-foreground"
                >
                  {index + 1}
                </span>
                <div className="flex flex-col gap-1">
                  <h2 className="font-heading text-lg font-semibold">
                    {title}
                  </h2>
                  <p className="text-muted-foreground">{text}</p>
                </div>
              </li>
            )
          })}
        </ol>

        <div className="flex flex-wrap gap-3">
          <Link
            to="/auth/register"
            className={cn(buttonVariants({ size: "lg" }))}
          >
            {m.howto_cta_register()}
          </Link>
          <Link
            to="/tenders"
            className={cn(buttonVariants({ variant: "outline", size: "lg" }))}
          >
            {m.portal_cta_tenders()}
          </Link>
        </div>
      </main>
    </>
  )
}
