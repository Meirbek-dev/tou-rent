# @tou/api-client

Сгенерированный типобезопасный клиент REST API `apps/api` (OpenAPI 3.1 из utoipa).

- Цепочка: `utoipa` → `openapi.json` → `openapi-typescript` → `src/schema.d.ts` → `openapi-fetch` + `openapi-react-query`.
- **Руками не редактировать** - только через `bun run codegen` (гейт G5 «кодоген без диффа», защищенный путь по арх. § 8).
- Фронтенд (`apps/web`) обращается к API только через этот пакет (гейт G7).
