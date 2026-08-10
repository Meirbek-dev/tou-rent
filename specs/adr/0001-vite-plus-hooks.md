# ADR-0001: нативные git-хуки Vite+ вместо lefthook

Статус: accepted · 2026-08-06 · связано: A-001, A-007

## Контекст

Арх. § 4 предусматривает `lefthook.yml`. Репозиторий фактически использует
Vite+ (A-001), который включает собственный диспетчер git-хуков
(`vp config` → `.vite-hooks/`) и раннер staged-проверок (`vp staged`,
конфиг - блок `staged` в `vite.config.ts`).

## Решение

lefthook не вводится. Хуки - проектные скрипты в `.vite-hooks/`:

- `pre-commit` - `vp staged`: oxfmt по staged TS/CSS/JSON/MD, rustfmt по staged `*.rs`;
- `commit-msg` - регэксп Conventional Commits (регламент А.3);
- `pre-push` - `vp check` + `cargo fmt --all --check`.

Диспетчер ставится `vp config --no-agent` (script `prepare` в корневом
package.json). Отключение на машине: `VP_GIT_HOOKS=0`.

## Следствия

- Одним инструментом меньше; конфиг staged-проверок живет рядом с lint/fmt в `vite.config.ts`.
- Rust-часть хуков ограничена rustfmt: clippy/test на Windows-хосте невозможны (A-003), их закрывает CI (G1–G2, G8).
