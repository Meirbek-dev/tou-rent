import { Link, createFileRoute } from "@tanstack/react-router"

import { m } from "#/paraglide/messages"
import { PublicShell } from "@/components/public-shell"
import { buttonVariants } from "@/components/ui/button"
import { howToSteps } from "@/lib/how-to-steps"
import { cn } from "@/lib/utils"

// FR-1401: статичная страница «как участвовать» (п. 5–6)
export const Route = createFileRoute("/how-to")({
  head: () => ({
    meta: [
      { title: `${m.howto_title()} - ToU Rent` },
      { property: "og:title", content: m.howto_title() },
      { property: "og:description", content: m.howto_intro() },
    ],
  }),
  component: HowToPage,
})

function HowToPage() {
  const steps = howToSteps()

  return (
    <PublicShell className="mx-auto w-full max-w-3xl px-4 py-10 sm:px-6">
      <div className="flex flex-col gap-10">
        <header className="flex flex-col gap-3">
          <h1 className="text-3xl font-semibold tracking-tight">
            {m.howto_title()}
          </h1>
          <p className="max-w-[68ch] text-lg text-muted-foreground">
            {m.howto_intro()}
          </p>
        </header>

        <ol className="flex list-none flex-col gap-6">
          {steps.map((step, index) => (
            <li key={step.title} className="flex gap-4">
              <span
                aria-hidden="true"
                className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-primary/10 font-mono text-sm font-semibold text-primary ring-1 ring-primary/20"
              >
                {index + 1}
              </span>
              <div className="flex flex-col gap-1">
                <h2 className="text-lg font-semibold">{step.title}</h2>
                <p className="max-w-[68ch] text-muted-foreground">
                  {step.text}
                </p>
              </div>
            </li>
          ))}
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

        {/* Куда идти дальше: условия конкретной процедуры живут в объявлении */}
        <section
          aria-labelledby="howto-questions"
          className="flex flex-col gap-2 rounded-xl border bg-muted p-5"
        >
          <h2 id="howto-questions" className="text-base font-semibold">
            {m.howto_questions_title()}
          </h2>
          <p className="max-w-[68ch] text-sm text-muted-foreground">
            {m.howto_questions_text()}
          </p>
          <Link
            to="/tenders"
            className="text-sm font-medium text-primary underline-offset-4 hover:underline"
          >
            {m.tenders_title()}
          </Link>
        </section>
      </div>
    </PublicShell>
  )
}
