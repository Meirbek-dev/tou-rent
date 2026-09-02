// Кодоген просит TypeScript 5, дерево пакетов закреплено на TypeScript 7.
//
// `openapi-typescript` объявляет peer `typescript@^5.x` и зовет классический
// API компилятора (`ts.factory`). В репозитории `overrides.typescript` держит
// весь граф на 7.0.2 - нативном порте, у которого из корня пакета экспортируется
// только версия (`lib/version.cjs`). Отсюда `ts.factory is undefined` и падение
// кодогена сразу после `import`.
//
// Понизить общий TypeScript нельзя: на 7.0.2 работает `vp check` (tsgo). Bun
// вложенные `overrides` не поддерживает, а peer-зависимость поднимает в корень
// - точечно выдать пакету другую версию установкой не выходит. Поэтому рядом
// живет alias `typescript-5`, а подмена делается крючком разрешения модулей.
//
// Запускается как `node --import ./scripts/ts5-register.mjs ...` (см. codegen).
import { register } from "node:module"

register("./ts5-hooks.mjs", import.meta.url)
