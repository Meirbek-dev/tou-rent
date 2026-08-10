import { describe, expect, it } from "vite-plus/test"

import { resolveTheme } from "./theme"

describe("theme preference", () => {
  it("uses the system preference by default", () => {
    expect(resolveTheme(null, true)).toBe("dark")
    expect(resolveTheme(null, false)).toBe("light")
  })

  it("uses a saved preference instead of the system preference", () => {
    expect(resolveTheme("light", true)).toBe("light")
    expect(resolveTheme("dark", false)).toBe("dark")
  })
})
