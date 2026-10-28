# Dependency refresh — 2026-10-28

Use Bun 1.4.4 and Rust 1.99.1. Vite+ 1.4.0, its Vite core alias and
Vitest 5.0.5 are aligned across the workspace. The Vite+ migrator preserves
Vitest 4 mock-history behavior explicitly. Node must be at least 24.11.0.

Direct JavaScript dependencies and Cargo requirements were checked against the
npm and crates.io registries. Both lockfiles include compatible transitive
updates. Container and CI pins cover Bun, Rust, Playwright 1.64.1, Alpine 3.24.3,
Redis 8.10.3, RustFS 1.0.4, Zitadel 4.20.0, AWS CLI 2.39.0, RustFS CLI 0.1.36,
Squawk 2.70.0, Trivy 0.76.1 and Schemathesis 4.32.7. Unchanged pins were already
current, including Caddy 2.11.7 and PostgreSQL 18.6.

## Compatibility decisions

- PostgreSQL 18.6 is the latest stable release; PostgreSQL 19 is still an RC.
  Development now matches production. **Do not attach an existing PostgreSQL 19
  development data directory to PostgreSQL 18.** Export its data first and restore
  into a separate PostgreSQL 18 volume. This change does not migrate or delete
  any existing volume.
- `tower-sessions` remains 0.14.0: the latest Redis store 0.16.0 requires that
  session-core API. Installing 0.15 would introduce incompatible session types.
- `typescript-5` remains a private compiler-API adapter for openapi-typescript;
  the workspace checker uses TypeScript 7.0.2. The adapter now uses Node's
  synchronous `registerHooks` API instead of deprecated `register`.
- Keep `@neodrag/core` on its existing next release: npm's `latest` tag still
  points to an older, incompatible 0.0.3 release. Nitro's latest tag is a beta.
- Keep transitive js-yaml and nanoid on patched versions of the majors their
  callers support. A scoped Bun override updates typography's exact selector
  parser dependency to patched 6.1.5 without downgrading shadcn's 7.x copy.

## Required source adaptations

Sentry 11 retains the explicit data-collection restrictions. Router error props
use the framework type. MRP year state initializes lazily. ZIP 9 entry-name
errors are handled in the archive test. Utoipa 6 requires explicit OpenAPI 3.1
response descriptions; the generated contract/client and a response-description
test accompany that migration. The locale switcher's accessible name includes
its visible locale code, as required by the updated accessibility checks.

## Remaining upstream advisories

`bun audit` reports low-severity GHSA-gcqx-w48m-w2h4 in deepmerge 4.3.1,
a shadcn development CLI dependency. npm publishes no patched release.

`cargo deny check advisories` remains red for RUSTSEC-2026-0387 in
crypto-bigint 0.5.5 through the latest openidconnect 4.0.1, and
RUSTSEC-2024-0449 in unmaintained unsafe-libyaml through the latest Typst.
Neither has an available compatible release path in this dependency graph.
The protected deny policy and its advisory exceptions were not changed.

## Documentation consulted

- https://viteplus.dev/guide/upgrade-project
- https://viteplus.dev/guide/vitest-v5
- https://bun.sh/docs/pm/cli/update
- https://bun.sh/docs/pm/overrides
- https://github.com/getsentry/sentry-javascript/blob/develop/MIGRATION.md
- https://docs.rs/zip/latest/zip/read/struct.ZipArchive.html
- https://www.postgresql.org/support/versioning/
- https://docs.docker.com/build/building/best-practices/
