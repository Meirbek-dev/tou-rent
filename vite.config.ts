import { defineConfig } from "vite-plus"

export default defineConfig({
  // Прекоммит-лазеры (арх. § 8): vp staged из .vite-hooks/pre-commit.
  // Выходы кодогена (fmt ignorePatterns ниже) исключены и из staged-глоба:
  // иначе коммит из одних сгенерированных файлов валит vp fmt
  // («Expected at least one target file»).
  // `query-*` - слепок `.sqlx` (гейт G3), такой же выход кодогена: его
  // формирует `sqlx prepare`, и переформатировать его нельзя - `sqlx prepare
  // --check` в rust-test сверяет файлы байт в байт с тем, что порождает sqlx.
  staged: {
    "!(openapi|schema.d|routeTree.gen|query-*).{ts,tsx,js,jsx,mjs,css,json,jsonc,md,yml,yaml}":
      "vp fmt --write",
    "*.rs": "rustfmt --edition 2024",
  },
  // Playwright-сценарии (apps/e2e, T14) гоняет свой раннер - vitest их
  // не собирает: у них другой test() и другой жизненный цикл.
  test: {
    exclude: ["**/node_modules/**", "**/dist/**", "apps/e2e/**"],
  },
  lint: {
    jsPlugins: [{ name: "vite-plus", specifier: "vite-plus/oxlint-plugin" }],
    rules: {
      "vite-plus/prefer-vite-plus-imports": "error",
      // Гейт G7 (арх. v3 § 9): запрет межслойных импортов. Правило дешевле
      // dependency-cruiser и стоит в том же конфиге, что и остальной линт.
      "no-restricted-imports": [
        "error",
        {
          // Правило единое для всех файлов, исключений по пути в нем нет,
          // поэтому «создавать клиент API только в @/lib/api» сюда не
          // выносится: запрет сработал бы и на самом @/lib/api, а глушитель
          // ради этого потребовал бы токена ALLOWED-BY-ENGINEER (гейт G2).
          patterns: [
            {
              group: ["**/routes/**"],
              message:
                "Маршрут не импортируется из другого маршрута: общий код выносится в @/components или @/lib.",
            },
            {
              group: ["**/paraglide/messages/*"],
              message:
                "Переводы берутся из #/paraglide/messages - вывод Paraglide по локалям внутренний.",
            },
          ],
        },
      ],
    },
    options: { typeAware: true, typeCheck: true },
  },
  fmt: {
    endOfLine: "lf",
    semi: false,
    singleQuote: false,
    tabWidth: 2,
    trailingComma: "es5",
    printWidth: 80,
    sortTailwindcss: {
      stylesheet: "apps/web/src/styles/globals.css",
      functions: ["cn", "cva"],
    },
    sortPackageJson: false,
    ignorePatterns: [
      "**/routeTree.gen.ts",
      "**/src/paraglide/",
      // Выход кодогена G5 - байт-в-байт как сгенерирован
      "packages/api-client/openapi.json",
      "packages/api-client/src/schema.d.ts",
      // То же для слепка схемы G3 (`cargo sqlx prepare`)
      ".sqlx/",
      "dist/",
      "node_modules/",
      ".turbo/",
      ".output/",
      ".nitro/",
      ".tanstack/",
      ".vinxi/",
      "coverage/",
      "pnpm-lock.yaml",
      ".pnpm-store/",
    ],
  },
})
