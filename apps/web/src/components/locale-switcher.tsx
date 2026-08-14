import { ChevronDownIcon, GlobeIcon } from "lucide-react"

import { getLocale, locales, setLocale } from "#/paraglide/runtime"
import { m } from "#/paraglide/messages"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

type Locale = (typeof locales)[number]

const LANGUAGE_LABELS: Record<Locale, () => string> = {
  kk: m.language_kazakh,
  ru: m.language_russian,
  en: m.language_english,
}

function isLocale(value: unknown): value is Locale {
  return (
    typeof value === "string" && (locales as readonly string[]).includes(value)
  )
}

export default function LocaleSwitcher() {
  const current = getLocale()
  const currentLanguage = LANGUAGE_LABELS[current]()

  const switchLocale = (locale: unknown) => {
    if (isLocale(locale) && locale !== current) {
      void setLocale(locale)
    }
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="gap-1.5 px-2"
            aria-label={m.language_switcher_label({
              language: currentLanguage,
            })}
          />
        }
      >
        <GlobeIcon className="size-4" aria-hidden="true" />
        <span className="min-w-5 text-center font-semibold tracking-wide">
          {current.toUpperCase()}
        </span>
        <ChevronDownIcon
          className="size-3 text-muted-foreground transition-transform group-aria-expanded/button:rotate-180"
          aria-hidden="true"
        />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" sideOffset={8} className="w-52 p-1.5">
        <DropdownMenuRadioGroup value={current} onValueChange={switchLocale}>
          <DropdownMenuLabel className="px-2 pt-1.5 pb-1">
            {m.language_label()}
          </DropdownMenuLabel>
          {locales.map((locale) => (
            <DropdownMenuRadioItem
              key={locale}
              value={locale}
              className="min-h-10 gap-3 px-2 py-2 pr-9"
            >
              <span
                className="flex size-4 shrink-0 items-center justify-center rounded-md bg-muted text-[0.7rem] font-semibold tracking-wide text-muted-foreground"
                aria-hidden="true"
              >
                {locale.toUpperCase()}
              </span>
              <span className="font-medium">{LANGUAGE_LABELS[locale]()}</span>
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
