export type Theme = "light" | "dark"

export const THEME_STORAGE_KEY = "tou-rent-theme"

export const themeInitializer = `
  (() => {
    try {
      const savedTheme = localStorage.getItem("${THEME_STORAGE_KEY}")
      const theme = savedTheme === "light" || savedTheme === "dark"
        ? savedTheme
        : matchMedia("(prefers-color-scheme: dark)").matches
          ? "dark"
          : "light"
      document.documentElement.classList.toggle("dark", theme === "dark")
      document.documentElement.style.colorScheme = theme
    } catch {
      const isDark = matchMedia("(prefers-color-scheme: dark)").matches
      document.documentElement.classList.toggle("dark", isDark)
      document.documentElement.style.colorScheme = isDark ? "dark" : "light"
    }
  })()
`

export function readStoredTheme(): Theme | null {
  try {
    const theme = localStorage.getItem(THEME_STORAGE_KEY)
    return theme === "light" || theme === "dark" ? theme : null
  } catch {
    return null
  }
}

export function resolveTheme(
  storedTheme: Theme | null,
  systemPrefersDark: boolean
): Theme {
  return storedTheme ?? (systemPrefersDark ? "dark" : "light")
}

export function applyTheme(theme: Theme): void {
  document.documentElement.classList.toggle("dark", theme === "dark")
  document.documentElement.style.colorScheme = theme
}
