// Подмена `typescript` на TypeScript 5 для кодогена (см. ts5-register.mjs).
//
// Крючок ловит спецификатор, а не путь импортера: `openapi-typescript`
// распакован в общий стор (`node_modules/.bun/...`), и разрешение оттуда
// всегда приходит в поднятый корневой `typescript`. Перехват по имени
// работает независимо от того, где лежит сам пакет.
import { createRequire } from "node:module"
import { pathToFileURL } from "node:url"

const require = createRequire(import.meta.url)
const TYPESCRIPT_5 = pathToFileURL(require.resolve("typescript-5")).href

export function resolve(specifier, context, next) {
  if (specifier === "typescript") {
    // TypeScript 5 отдается CommonJS-модулем: `import ts from "typescript"`
    // получает его `module.exports` - то самое пространство имен с `factory`
    return { url: TYPESCRIPT_5, format: "commonjs", shortCircuit: true }
  }
  return next(specifier, context)
}
