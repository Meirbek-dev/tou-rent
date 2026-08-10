import { Moon02Icon, Sun03Icon } from "@hugeicons/core-free-icons"
import { HugeiconsIcon } from "@hugeicons/react"
import { useEffect, useState } from "react"

import { m } from "#/paraglide/messages"
import { Button } from "@/components/ui/button"
import {
  THEME_STORAGE_KEY,
  applyTheme,
  readStoredTheme,
  resolveTheme,
  type Theme,
} from "@/lib/theme"

export function ThemeToggle() {
  const [theme, setTheme] = useState<Theme | null>(null)

  useEffect(() => {
    const colorScheme = window.matchMedia("(prefers-color-scheme: dark)")
    const syncTheme = () => {
      const resolvedTheme = resolveTheme(readStoredTheme(), colorScheme.matches)
      applyTheme(resolvedTheme)
      setTheme(resolvedTheme)
    }

    syncTheme()
    colorScheme.addEventListener("change", syncTheme)
    window.addEventListener("storage", syncTheme)

    return () => {
      colorScheme.removeEventListener("change", syncTheme)
      window.removeEventListener("storage", syncTheme)
    }
  }, [])

  const toggleTheme = () => {
    const currentTheme =
      theme ??
      (document.documentElement.classList.contains("dark") ? "dark" : "light")
    const nextTheme = currentTheme === "dark" ? "light" : "dark"

    applyTheme(nextTheme)
    setTheme(nextTheme)
    try {
      localStorage.setItem(THEME_STORAGE_KEY, nextTheme)
    } catch {
      // The visual preference still applies for this page when storage is blocked.
    }
  }

  const label =
    theme === "dark" ? m.theme_switch_to_light() : m.theme_switch_to_dark()

  return (
    <Button
      type="button"
      variant="ghost"
      size="icon"
      aria-label={label}
      title={label}
      onClick={toggleTheme}
    >
      <HugeiconsIcon
        icon={Moon02Icon}
        strokeWidth={2}
        className="dark:hidden"
        aria-hidden="true"
      />
      <HugeiconsIcon
        icon={Sun03Icon}
        strokeWidth={2}
        className="hidden dark:block"
        aria-hidden="true"
      />
    </Button>
  )
}
