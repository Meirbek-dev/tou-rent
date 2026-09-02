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
    // Шаблоны установленных навыков - чужой вендоренный код (примеры чужих
    // стеков), а не исходники проекта: правила проекта к ним неприменимы,
    // и их правка сорвется при следующей установке навыка.
    ignorePatterns: [".claude/skills/**", ".agents/skills/**"],
    jsPlugins: [{ name: "vite-plus", specifier: "vite-plus/oxlint-plugin" }],
    plugins: [
      "eslint",
      "typescript",
      "oxc",
      "react",
      "react-perf",
      "jsx-a11y",
      "import",
      "promise",
      "unicorn",
      "vitest",
      "node",
    ],
    categories: {
      correctness: "error",
      suspicious: "error",
    },
    rules: {
      "vite-plus/prefer-vite-plus-imports": "error",
      // Правило из времен, когда JSX компилировался в React.createElement
      // и требовал React в области видимости. Автоматическое JSX-преобразование
      // (jsx: "react-jsx") импортирует runtime само - правило дает ошибку
      // на каждый тег в проекте и ничего не проверяет.
      "react/react-in-jsx-scope": "off",
      // Правило советует заменить `role="status"` на <output>. Это не замена:
      // <output> связан с формой и вычисленным по ней значением, а role=status
      // - живая область, которую диктор объявляет при изменении (лента торгов,
      // FR-603). Кроме того, правило срабатывает на примитивах shadcn, где
      // роль ставит верстка донора.
      "jsx-a11y/prefer-tag-over-role": "off",
      // Прокручиваемая область - штатное исключение из правила: до правой
      // части широкой таблицы (реестр лотов) с клавиатуры иначе не добраться
      // вовсе (SC 2.1.1), а сама область интерактивной не становится.
      // Разрешение точечное: добавлена роль region, остальное правило работает.
      "jsx-a11y/no-noninteractive-tabindex": [
        "error",
        {
          tags: [],
          roles: ["tabpanel", "region"],
          allowExpressionValues: true,
        },
      ],
      // Утверждение типа не бесплатно, но и не всегда неоправданно: разбор
      // «сырых» query-параметров (lib/*-search.ts) сужает unknown после
      // собственной проверки, и заменить это на type guard - отдельная работа
      // на 60 мест в 22 файлах. Правило включается вместе с этой работой,
      // а не вместо нее: до тех пор оно молчало бы выборочно и приучало
      // проходить мимо (TODO-ENGINEER: отдельная задача).
      "typescript/no-unsafe-type-assertion": "off",
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
    // Примитивы shadcn (components/ui) кладет генератор, и он же их заменяет
    // при обновлении: правки в них живут до следующего `shadcn add`. Держать
    // на них те же правила, что и на своем коде, значит либо чинить одно
    // и то же после каждого обновления, либо (что и происходит на деле)
    // отключить правило целиком - и потерять его на своем коде тоже.
    // Поэтому послабления перечислены поименно и только для этого каталога.
    overrides: [
      {
        files: ["apps/web/src/components/ui/**"],
        rules: {
          "no-shadow": "off",
          "typescript/consistent-return": "off",
          "no-underscore-dangle": "off",
          "react/no-unstable-nested-components": "off",
          "jsx-a11y/label-has-associated-control": "off",
          "jsx-a11y/anchor-has-content": "off",
          "jsx-a11y/click-events-have-key-events": "off",
          "jsx-a11y/no-noninteractive-element-interactions": "off",
        },
      },
    ],
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
      // Установленные навыки - вендоренные файлы, как и в lint выше.
      ".claude/skills/",
      ".agents/skills/",
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
