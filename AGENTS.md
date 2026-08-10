<!--VITE PLUS START-->

# Using Vite+, the Unified Toolchain for the Web

This project is using Vite+, a unified toolchain built on top of Vite, Rolldown, Vitest, tsdown, Oxlint, Oxfmt, and Vite Task. Vite+ wraps runtime management, package management, and frontend tooling in a single global CLI called `vp`. Vite+ is distinct from Vite, and it invokes Vite through `vp dev` and `vp build`. Run `vp help` to print a list of commands and `vp <command> --help` for information about a specific command.

Docs are local at `node_modules/vite-plus/docs` or online at <https://viteplus.dev/guide/>.

## Review Checklist

- [ ] Run `vp install` after pulling remote changes and before getting started.
- [ ] Run `vp check` and `vp test` to format, lint, type check and test changes.
- [ ] Check if there are `vite.config.ts` tasks or `package.json` scripts necessary for validation, run via `vp run <script>`.
- [ ] If setup, runtime, or package-manager behavior looks wrong, run `vp env doctor` and include its output when asking for help.

<!--VITE PLUS END-->

# AGENTS.md - регламент работы ИИ-агента в TOU.Rent

## А.1 Источники истины

Порядок приоритета: ТЗ v2 (docs/tou-rent-tz-v2.md) → ТЗ v1 (docs/tou-rent-tz-v1.md) →
Архитектура v3 (docs/tou-rent-architecture-v3.md) → specs/INVENTORY.md → этот файл.
При расхождении v2 и v1 действует v2; Архитектура v2 сохраняется как история замысла.
Правила университета (PDF) - первоисточник предметной области; при расхождении с ТЗ - записать
вопрос в specs/QUESTIONS.md и следовать ТЗ.

Допущение о предметной константе, тексте формы или перечне из Правил больше не создается:
такие места уже накоплены в TODO-ENGINEER и в specs/QUESTIONS.md, и увеличивать этот долг
запрещено - вместо нового допущения новый вопрос (ТЗ v2 § 9).

## А.2 Рабочий цикл

1. Возьми верхнюю невыполненную задачу из specs/BACKLOG.md (контуры 1–3 - ТЗ v1 § 9 и § 4, контур 4 - ТЗ v2 § 5).
2. Ветка `feat/<T-id>-<slug>` от свежего main.
3. Реализация по DoD (ТЗ § 10). Самопроверка: `vp check` (fmt→lint→typecheck) + `vp test` (Vitest)
   - `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace`. Кодоген
     `vp check` не запускает - после правки контракта нужен `bun run codegen` (гейт G5 это проверит).
4. MR в main c описанием: задача, FR/INV-ID, что покрыто тестами, допущения.
5. Merge при зеленом пайплайне (auto-merge). Ревью людей нет - пайплайн и есть ревью.

Дев-стенд: `vp run stack:up` - Postgres/Redis/RustFS + api в контейнере (:8080,
health `/api/v1/healthz`; миграции накатываются перед стартом). После правок Rust -
`vp run api:restart`, логи - `vp run api:logs`. Web - `vp run dev` (:3000). Только
инфраструктура без api: `podman compose -f infra/compose/docker-compose.dev.yml up -d`.

## А.3 Коммиты и MR

Conventional commits, английский: `feat(auction): enforce 5% bid step [FR-601][INV-063]`.
MR ≤ 400 строк диффа (сгенерированные файлы не в счет). Одна задача - один MR.

## А.4 Запрещено (гейты это проверяют, не пытайся обойти)

- unwrap/expect/panic!/todo!/unimplemented!/dbg!/println! в не-тестовом коде; #[allow(...)], #[ignore], .skip(, oxlint-disable - только с токеном `ALLOWED-BY-ENGINEER:<ticket>` в той же строке.

## А.5 Обязательно

- Каждая мутация домена пишет audit-событие (проверь перечень INV-AUDIT при создании таблиц).
- Каждая новая строка UI - через ключ Paraglide в трех локалях (kk/en можно draft-переводом).
- Каждый инвариант из ТЗ закрепляй на самом нижнем достижимом уровне: тип → constraint БД → тест.
- Ошибки - типизированные, problem+json; новые коды ошибок добавляй в enum контракта.
- Время - только через Clock-абстракцию домена (SystemTime::now запрещен G2).

## А.6 Неоднозначность

Не блокируйся. Выбери наименее рискованную интерпретацию, запиши в specs/ASSUMPTIONS.md:
`A-NNN | дата | контекст | принятое допущение | что проверить инженеру`, сошлись на A-NNN в MR.

## А.7 Самоконтроль перед MR (чек-лист)

[ ] vp check зеленый [ ] cargo clippy/test зеленые [ ] новые FR покрыты тестами и упомянуты в коммитах
[ ] нет TODO без TODO-ENGINEER [ ] миграции идемпотентно накатываются на чистую БД и на прод-дамп
[ ] i18n-ключи добавлены [ ] audit-события есть [ ] ASSUMPTIONS.md обновлен при допущениях
