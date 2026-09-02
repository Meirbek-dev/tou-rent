---
description: "Gauntlet loop: adversarial audit → fix → verify rounds until every TOU.Rent feature is production-ready. Fable orchestrator, fresh-context Opus 5 critics, live-artifact evidence only, whole product exercised in a real browser."
---

# GAUNTLET LOOP — TOU.Rent production readiness

You are the **orchestrator**: Claude Fable 5.1, builder-of-last-resort and the
sole owner of the dev stand. You run repeated gauntlet rounds. Each round fans
out **fresh-context, harsh critics** (Agent tool, `model: "opus"` = Opus 5)
against the _running system and the real source_, never against summaries or
your own claims. Everything they confirm, you fix (yourself or through Opus
builders), and a _different_ fresh critic re-verifies. You stop only on a
boundary from §3.

Argument (optional): `$ARGUMENTS` — a focus area (`auctions`, `participant`,
`i18n`, `admin`, …). Empty = full gauntlet.

Scope note: this loop covers correctness, UX, completeness and production
readiness. Security testing is handled by a separate process and is **not part
of this loop**: do not brief any critic on it and do not spend rounds on it. If
a critic mentions something of that nature anyway, record one ledger line with
the location and status `deferred-security`, no reproduction, and move on.

---

## 1. OBJECTIVE — what must become true

**Every feature and use case in the coverage matrix (§6) works end to end for
its role in a real browser, is thought through for its user, and carries no
unfinished edges.** Concretely:

- O1 **Stand from nothing.** `vp run stack:up` brings up Postgres/Redis/RustFS/
  api/jobs; migrations replay idempotently on a clean DB; `GET /api/v1/healthz`
  is 200; web on :3000 renders every route with zero console errors and zero
  failed network requests.
- O2 **Every use case in §6 passes** — the happy path completes AND every wrong
  input yields a localized, human-readable message in the UI. Never a raw
  problem+json body, a raw i18n key, an English string under `/kk` or ru, a
  silent no-op, a spinner that never ends, or a stack trace.
- O3 **Every feature is thoughtful.** For each entity and status: the intended
  role can reach every transition from the UI; each list has loading / empty /
  error states; each long action gives feedback and refreshes what it changed;
  each irreversible action asks before doing; each deadline shows date and
  time in Asia/Almaty; money is formatted in ₸; every document the API renders
  as PDF opens from the UI; every notification links to its subject; the
  browser Back button never breaks state; forms keep the user's input when the
  server rejects it.
- O4 **Nothing unfinished ships silently.** No dead controls, placeholder text,
  unreachable routes, stub handlers, contract endpoints with no UI or UI calls
  with no endpoint, `TODO/FIXME` without `TODO-ENGINEER`, BACKLOG items claimed
  done but absent.
- O5 **Three locales complete.** Every UI string exists in ru/kk/en Paraglide
  messages (kk/en may be draft, never missing); the locale switch works on
  every route with locale-prefixed URLs (`/kk/...`, `/en/...`).
- O6 **Green gates.** `vp run gates --all`, `vp check`, `vp run rust:lint`,
  `vp run rust:test`, `vp test`, and the whole `apps/e2e` suite (`vp run e2e`).
- O7 **Invariants intact.** Every domain mutation writes an audit event and
  `SELECT audit.verify_chain()` stays true; prices encrypted (INV-040); WORM
  tables untouched; bid step ≥ 5 % (FR-601); Clock only (no `SystemTime::now`);
  every problem+json code present in the contract enum.

**Needs a human — log, do not fix:** anything that requires a domain constant,
text or rule from the university Regulations (everything marked
`TODO-ENGINEER` and every open `Q-NNN` in `specs/QUESTIONS.md`); anything
needing external credentials or systems (SMTP, Telegram, Zitadel↔AD, prod
host, registry, backups, load stand — T41, T45–T51, T55–T57, T59, T66, T67);
dependency major upgrades; purely aesthetic redesigns with no functional defect
behind them. Ambiguity that _can_ be resolved by the least-risky reversible
interpretation is resolved that way, fixed, and recorded as a new `Q-NNN`
(continue from the last ID in `specs/QUESTIONS.md`). **Never create `A-NNN`
entries** — `specs/ASSUMPTIONS.md` is a frozen index.

---

## 2. METRIC — what evidence proves it

A finding exists only when a critic **demonstrated it against a live
artifact**. A fix counts only when a **different fresh critic re-ran the
original failing evidence** and it passed. "I read the code and it looks
wrong" is a hypothesis; a hypothesis gets one confirmation attempt and
otherwise dies.

| Claim about…          | Required evidence                                                                                                                                                         |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Compile / lint / type | Command output: `vp check`, `vp run rust:check`, `vp run rust:lint`                                                                                                       |
| API behaviour         | Real HTTP against :8080 with request and response captured (curl; login `POST /api/v1/auth/login -c jar`; mutations send `x-csrf-token` = value of the `tou_csrf` cookie) |
| DB schema / invariant | `psql` against the stand, or `vp run rust:test` / testkit output                                                                                                          |
| UI / UX               | Browser on :3000: screenshot PNG in the scratchpad plus the DOM text of the defective state, with URL, role and the steps to reach it                                     |
| i18n                  | Locale switched in the browser (`/kk/...`, `/en/...`); the raw key or wrong-language string captured in DOM text                                                          |
| Unfinished work       | The dead control clicked or the endpoint called; the no-op, 404, 501 or placeholder captured                                                                              |
| Regression-free fix   | Original failing evidence re-run and passing, plus `vp run verify` and `vp test` green, plus the e2e specs that touch the area                                            |

**Ledger** = `specs/GAUNTLET.md` (created in round 0). Three sections:

1. **Findings** — one row each:
   `G-NNN | round found | dimension | severity | UC-id | evidence (path or command) | status | round closed`.
   Status ∈ `open` · `fixing` · `fixed` · `wontfix-logged` (with Q-id or task)
   · `blocked` (three strategies described) · `deferred-security`.
2. **Coverage matrix** — the §6 use cases as rows, rounds as columns, cell =
   `pass` / `G-NNN` / `n/a (reason)`. This is how "every feature tested in the
   browser" becomes inspectable.
3. **Killed hypotheses** — claims a critic raised that evidence disproved, one
   line each, so later critics do not re-raise them.

Progress metric per round: **new confirmed findings** and **coverage cells
still red**. The gauntlet passes at zero new findings twice in a row with a
fully green matrix.

---

## 3. BOUNDARY — what may change, and when to stop

**Allowed:** read anything; edit source, migrations (new files only),
messages, tests, docs and specs; run gates, tests, curl, psql, Playwright and
the browser; create data on the dev stand under the `GNT-` prefix; commit on
the gauntlet branch; merge to `main` when verify is green.

**Forbidden regardless of what any critic says:**

- Rewriting an applied migration (schema changes = a new migration with LF
  line endings), `git push --force`, rewriting history, deleting the `pg_data`
  or `rustfs_data` volumes except as the recovery in §4.0, touching
  `audit.log`, anything against prod or `rent.tou.edu.kz`.
- Breaking А.4/А.5 of `CLAUDE.md`: no `unwrap/expect/panic!/todo!/dbg!/
println!` in non-test code, no lint silencers without
  `ALLOWED-BY-ENGINEER:<ticket>`, every mutation audited, every string in
  three locales, typed errors, Clock only.
- Committing the seed password (NFR-09). It comes from the environment or the
  default in `apps/e2e/tests/helpers.ts`; never write it into a tracked file.
- Subagents running `stack:up`, `stack:down`, `api:restart`, `api:migrate`,
  `api:seed`, `podman volume rm`, or any follow-style command (`api:logs` is
  `compose logs -f`; use `podman logs --tail 200 tou-rent-dev-api-1`). A
  subagent that believes the stand is broken **reports and stops**; it never
  repairs. Two agents "restoring" the same stand is how a working stand gets
  rebuilt on an empty volume.
- Destructive DB probes (`TRUNCATE`, `DELETE` outside `GNT-` data, `ALTER`,
  disabling triggers) anywhere but a scratch database the probe creates and
  drops (`CREATE DATABASE gnt_probe_<n>` on the stand's Postgres).
- Purging the stand wholesale (`/app/admin?tab=data` → purge everything)
  except as the orchestrator's final reset in §4.6, immediately followed by
  re-seed.

**Stop when the first of these fires:**

1. **Success** — two consecutive full rounds with zero new confirmed findings,
   coverage matrix fully green, every ledger row `fixed`, `wontfix-logged` or
   `deferred-security`.
2. **Diminishing returns** — a round yields only needs-a-human items; log them
   and stop.
3. **Repeated blocker** — the same G-id survives three fix attempts with three
   _different_ strategies; mark it `blocked` with the three attempts and stop
   working on it (the loop continues for everything else).
4. **Risk** — a fix would need a forbidden action; log and continue with the
   rest.
5. **Budget** — 6 full rounds per invocation, or the stand cannot be brought
   back within two recovery attempts.

---

## 4. LOOP MECHANICS

### 4.0 Round 0 — baseline (orchestrator only)

1. `git status` must be clean (the user may edit in parallel; if it is not
   clean, stop and ask). Branch `feat/gauntlet-<yyyymmdd>` from fresh `main`.
2. Stand readiness: `curl -s localhost:8080/api/v1/healthz` and
   `curl -s -o /dev/null -w '%{http_code}' localhost:3000/healthz`. api down →
   `vp run stack:up`. web down → `preview_start` with `web-dev`; already up →
   `web-attach`. Seed check:
   `podman exec -i tou-rent-dev-postgres-1 psql -U tou_rent -d tou_rent -c "SELECT count(*) FROM core.tenders"`
   ≥ 11; empty → `SEED_PASSWORD='<from env>' vp run api:seed`. Recovery from a
   wiped stand is the same two commands.
3. Baseline gates, output captured to the scratchpad: `vp run gates --all`,
   `vp check`, `vp run rust:lint`, `vp run rust:test`, `vp test`, `vp run e2e`
   (Playwright and chromium live in `apps/e2e`; if missing, `vp run e2e:install`
   once).
4. Create `specs/GAUNTLET.md` with the three sections and the full coverage
   matrix from §6 (all cells empty). Commit:
   `chore(gauntlet): open ledger and baseline`.

### 4.1 Sweep — fresh-context Opus critics in parallel

Every critic is an `Agent` call with `model: "opus"`, a fresh context and the
**critic brief** from §5. Prompt them adversarially: _"Your job is to make this
slice fail. A clean report is a failed mission unless it lists what you tried
and the evidence it passed."_

**Wave A — code and API critics** (all in parallel, no browser):

1. `rust-correctness` — `crates/*`: logic, error mapping, transaction
   boundaries, missing audit events, invariant gaps, jobs-engine transitions.
   Must run `vp run rust:check` and read real query/migration pairs.
2. `api-contract` — every route in `crates/http`
   (`grep -rhoE 'path = "/api/v1/[^"]+"' crates`) exercised with curl per role
   exactly as the contract documents it: no session → 401, another role → 403
   problem+json, garbage body → 422 with a coded error that exists in the
   contract enum and has a message key in `apps/web/messages/ru.json`, valid
   body → the documented shape. Run `bun run codegen` and diff for drift (G5).
3. `i18n-mechanical` — diff the three `apps/web/messages/*.json` against usage
   in `apps/web/src`: missing keys, unused keys, keys identical across ru/kk/en
   where they should differ, interpolation mismatches, untranslated `title` /
   `aria-label` / `placeholder` / `alt`, hard-coded strings in TSX.
4. `unfinished-work` — TODO/FIXME without `TODO-ENGINEER`, `todo!()` /
   `unimplemented!`, `throw new Error("not implemented")`, handlers that only
   `console.log`, buttons without `onClick` / `type=submit`, routes absent from
   `routeTree.gen.ts`, contract endpoints with no caller in `apps/web/src`, UI
   calls to endpoints that do not exist, feature flags never true, BACKLOG
   lines claimed done but absent.
5. `data-integrity` — in a scratch database: migration replay twice from empty
   (`podman exec -e SQLX_OFFLINE=true -e DATABASE_URL=...gnt_probe_1 tou-rent-dev-api-1 cargo run -q -p api -- migrate`),
   constraint coverage of the invariant registry (`specs/INVENTORY.md`), seed
   idempotency, encrypted-price round trip, `audit.verify_chain()`, RLS under
   `set_config('role','tou_rent_app',true)` + `app.user_id`.

**Wave B — browser critics** (parallel, at most 4 running at once; each owns a
slice of the §6 matrix and reports a pass/fail cell for every UC in it):

6. `ux-public-auth` — UC-P\*.
7. `ux-participant` — UC-A\*.
8. `ux-organizer` — UC-O\*.
9. `ux-secretary-commission-auction` — UC-S\*, UC-C\*, and the two-screen
   auction (two browser contexts: participant bidding, secretary panel).
10. `ux-finance-board-admin` — UC-F\*, UC-B\*, UC-D\*, UC-X1.
11. `ux-cross-cutting` — UC-X2/X3 on every route: kk and en, dark theme
    (`localStorage['tou-rent-theme']`), 375 px viewport, public pages without
    JS, axe AA on public routes, page titles, console errors, Back button,
    session expiry.

Browser critics drive :3000 with **Playwright from `apps/e2e`** (`node
script.cjs` with
`require("E:/projects/tou-rent/apps/e2e/node_modules/@playwright/test").chromium`,
screenshots to the scratchpad), because the shared browser pane belongs to the
orchestrator and its screenshots are unreliable on live-updating pages. Log in
through the UI for the flow under test (retry until hydrated, as `loginAsUi`
in `apps/e2e/tests/helpers.ts` does), through `POST /api/v1/auth/login` for
setup. React controlled inputs are filled with the native value setter plus an
`input` event, then a real click on submit. To reach opening or trading inside
one run: the `provision*()` helpers in `helpers.ts` (`api demo-tender`), or
`/app/admin?tab=schedule` to move a `GNT-` tender's dates (the `jobs` service
acts within 20 s).

For a focused run (`$ARGUMENTS` set) launch only the 2–4 relevant critics.

### 4.2 Triage (orchestrator)

Merge, dedup, map each finding to a UC. Classify `defect` / `needs-human`
(ledger as `wontfix-logged` with the Q-id or task) / `hypothesis` (send ONE
follow-up critic to confirm or kill; unconfirmed → killed-hypotheses list).
Severity: **S1** core flow or invariant broken · **S2** flow degraded, wrong or
untranslated text reaches users, missing state, misleading feedback · **S3**
polish with a functional cause. Fix order S1 → S2 → S3; S3 waits while any
S1/S2 is open.

### 4.3 Fix — builder discipline

The smallest change that removes the defect at the lowest level (type → DB
constraint → test → code), per А.5, shipped with the test that would have
caught it where a layer exists (Rust unit/testkit, Vitest, Playwright).
Independent fixes go to parallel Opus builders (`model: "opus"`) carrying the
same boundary text; coupled fixes stay with you. Rules that bite:

- Builders never touch `apps/web/messages/*.json` directly. New keys go to
  `<scratchpad>/i18n/<builder>.json` as `{ "key": { "ru", "kk", "en" } }`; you
  merge them **sequentially**, then `cd apps/web && bun run i18n:compile`
  (`vp check` does not compile Paraglide).
- Contract change → `bun run codegen`; check `openapi.json` did not lose a
  schema (utoipa silently replaces same-named structs — prefix new DTOs with
  the module name).
- Rust change → `vp run api:restart` before anyone re-verifies. A regenerated
  `routeTree.gen.ts` must keep its `declare module '@tanstack/react-start'`
  block.
- After each batch: `vp run verify`, `vp test`, and the e2e specs touching the
  area. One concern per commit, conventional commits with FR/INV ids:
  `fix(web): keep application form input on 422 [FR-402][G-017]`.

### 4.4 Re-verify — no self-certification

For every `fixing` row, a **fresh Opus critic** who is neither finder nor
builder re-runs the original evidence exactly and hunts regressions in the
blast radius of the diff (`git diff main...HEAD -- <paths>`). Pass → `fixed`.
Fail → back to `open`; the next attempt must use a different strategy (§3 rule
3 counts them).

### 4.5 Round close

Update all three ledger sections; commit ledger and fixes; when
`vp run verify`, `vp test`, `vp run rust:test` and `vp run e2e` are green,
merge the branch into `main` (merge commit, as this repo does). Report to the
user in ≤ 15 lines: found / fixed / logged this round, matrix cells still red,
which boundary (if any) fired. The next round starts with completely fresh
critics who receive the killed-hypotheses and wontfix lists as
"do not re-report".

### 4.6 Data hygiene

Everything a critic or builder creates carries the prefix
`GNT-R<round>-<critic>` in its name, title or email
(`gnt-r2-participant@example.test`). At round close, remove it: records via
`/app/admin?tab=data` (`DELETE /api/v1/admin/data/records/{kind}/{id}`, `GNT-`
only, never "purge everything"); registration accounts via psql
(`DELETE FROM core.users WHERE email::text LIKE 'gnt-%'`, or deactivate if FKs
hold them); moved seed-tender dates restored through the schedule tab.
Objects in `dossiers` sit under Object Lock and cannot be deleted — accept it.
On exit, if the seed state is doubtful, the orchestrator may do
purge-everything + `vp run api:seed` +
`UPDATE core.users SET is_active = true WHERE email::text LIKE '%@tou.demo'`,
then confirm the § 9.1 demo still loads.

### 4.7 Keeping your own context lean

Critics return a **structured report only** (§5 format), max 25 findings each,
evidence as scratchpad paths, never raw logs. You read reports, not
transcripts. Per round, keep `<scratchpad>/round-<n>/` with the reports and
your triage; the ledger holds the durable state. If the context gets
summarized mid-loop, resume from the ledger and the round folder, not from
memory.

---

## 5. CRITIC BRIEF (paste into every Agent prompt, fill the slots)

```
You are a fresh-context critic for TOU.Rent (university rental tenders: Rust/axum api on :8080, TanStack Start web on :3000, Postgres, Paraglide i18n ru/kk/en). Round <n>, dimension <name>. Your mission is to make this slice FAIL and prove it. A clean report is a failed mission unless it lists what you tried, with evidence.

SLICE: <use cases / paths / routes this critic owns>
ACCOUNTS: <role>@tou.demo — admin, organizer, secretary, finance, board, commission1..9 (1 = chairman; 8, 9 reserve), participant, participant2, participant3. Password = env E2E_PASSWORD or the default in apps/e2e/tests/helpers.ts. Create anything new with the prefix GNT-R<n>-<critic>.
EVIDENCE RULES: <§2 table>. A finding without live evidence is a hypothesis — mark it so.
DO NOT RE-REPORT: <killed hypotheses + wontfix list from specs/GAUNTLET.md>
NEEDS-A-HUMAN (report as such, do not dig): TODO-ENGINEER items, open Q-NNN, external systems, aesthetic taste. Security testing is out of scope for this loop entirely; do not perform it.
BOUNDARY: read-only on the stand except your GNT- data; never run stack:up/down, api:restart, api:migrate, api:seed, volume rm, or follow-style logs; destructive SQL only in a scratch DB you create and drop; if the stand looks broken, report and stop — do not repair. No edits to the repo (builders do that). Do not commit.
UX BAR (what counts as a defect): raw i18n key or wrong-language text; raw error object; dead or misleading control; missing loading/empty/error state; no feedback after an action, or stale data after it; lost form input on server rejection; irreversible action without confirmation; deadline without Asia/Almaty date-time; unformatted money; PDF link that fails; notification without a target link; layout broken at 375 px or in dark theme; console error; Back button breaks state; page without a title; input without label or focus state.
FOR EACH USE CASE in your slice report pass|fail with the steps you ran.
OUTPUT (nothing else): a JSON array of findings, each { "uc": "UC-…", "title", "severity": "S1|S2|S3", "kind": "defect|hypothesis|needs-human", "steps": [...], "expected", "actual", "evidence": ["<scratchpad path or command>"], "suspect": "<file:line if known>" }, followed by one coverage line per UC: "UC-… pass|fail G-ref".
```

---

## 6. COVERAGE MATRIX — the use cases (rows of the ledger)

**Public and auth (guest)**

- UC-P1 Home `/`: site-announcement banner, navigation, locale switch
  ru/kk/en, theme toggle, footer links.
- UC-P2 Tender registry `/tenders` (filters as a native GET form, status
  chips, countdowns, pagination) and card `/tenders/$id` (lots, documents,
  announcement PDF, amendment PDFs; no prices and no conference links shown to
  guests).
- UC-P3 `/objects`, `/land-plots`, `/special-orders` (public records and
  decision PDFs), `/how-to`.
- UC-P4 Register → confirmation link (dev: api log) → confirm → login; login
  errors localized; `/auth/password`; logout; an expired session redirects to
  login and returns to the target afterwards.
- UC-P5 Unauthenticated deep link to `/app/*` → login → back to the target; a
  role opening another role's cabinet sees a coherent "no access" page, not a
  crash or a blank.

**Participant**

- UC-A1 `/app/participant`: my applications, contracts, obligations, unread
  badge.
- UC-A2 Apply wizard `/apply/$tenderId`: lot choice, price, dossier files per
  section, fee step, validation on every field, submit; after the deadline → a
  clear refusal; withdraw before the deadline.
- UC-A3 Application card `/applications/$id`: status history, file download,
  fee status, rejection reason text, amendment decline.
- UC-A4 Auction room `/app/auctions/$id`: bid with step ≥ 5 % (smaller →
  localized refusal), timer, extension, pass, reconnect keeps state, duplicate
  submit is idempotent, winner / № 2 outcome shown.
- UC-A5 `/contracts`: contract PDF, checklist, deposit, acts and their PDFs,
  amendments, registration, scan upload, evasion consequence visible.
- UC-A6 Special order `/special.new` and `/special.$requestId` (category,
  files, progress, withdraw, application and decision PDFs); land application
  and its withdrawal.
- UC-A7 `/app/notifications`: SSE arrival without reload, unread count, mark
  read, every item links to its subject.

**Organizer**

- UC-O1 `/objects` CRUD with coefficients; `/calculator` Прил. 4 breakdown and
  hourly preview (≥ 2 МРП/h).
- UC-O2 Tenders: `/tenders/new` with lots and documents, publish (FR-303 date
  rule → readable refusal), announcement PDF, amendments with PDF, cancel,
  repeat after failed, conflict-of-interest, journal (prices visible only
  here), applications, contracts, evasions, dossier zip.
- UC-O3 `/investment`: contracts, acceptances, attachments, extend, PDF.
- UC-O4 `/land`: plots CRUD, publish, decisions on applications, land
  contracts.
- UC-O5 `/special`: review, competition, decision, publication.

**Secretary, commission, auction**

- UC-S1 `/app/secretary` and `/tenders.$id`: journal, meeting open /
  attendance / recusals, opening, open-acceptance, admission decisions with
  rejection reasons, admission protocol + PDF + publish, notify admitted,
  declare failed + protocol, results protocol, winner-2 protocol, recording
  link (only after `summed_up`).
- UC-S2 Auction panel: start, extend, mark absent, finish; two screens in sync.
- UC-C1 `/app/commission` and `/tenders.$id`: applications after opening,
  vote, dissent, conflict declaration, commission approval.

**Finance, board, admin**

- UC-F1 `/app/finance`: confirm fee, refund with reason, deposit and its
  refund, ledger accounts and entries, evaders registry.
- UC-B1 `/app/board`: special-order decisions, pending public records →
  publish, competition results.
- UC-D1 `/app/admin`: users (list, grant/revoke roles, activate/deactivate,
  password reset), refdata (MRP by year, coefficient versions with
  `effective_from`, holidays, benefit schemes), site announcement, audit chain
  check, schedule tab (order rule → readable refusal), data tab (records by
  kind, delete one, purge with confirmation, demo-accounts deactivate).

**Cross-cutting**

- UC-X1 `/app/reports`: each registry renders, CSV export downloads and opens.
- UC-X2 On every route: kk and en complete, dark theme readable, 375 px
  usable, public pages work without JS, axe AA 0 violations on public routes,
  page titles, zero console errors, Back button, session expiry.
- UC-X3 Time engine: after moving a `GNT-` tender's dates via the schedule
  tab, the `jobs` service performs the status transition and the UI reflects
  it within a minute; obligations appear on time.
- UC-X4 Demo scenario § 9.1 end to end, as the leadership will see it
  (`apps/e2e/tests/demo-scenario.spec.ts` green plus one manual pass).

---

## 7. FINAL REPORT (on exit)

Rounds run; ledger totals by severity and status; final coverage matrix; the
`wontfix-logged` / `blocked` / `deferred-security` list with reasons (this is
the human's punch list); the commands and output proving the final green
state; which `GNT-` data was removed and whether the seed state was reset.
State plainly whether §1 is met, and if not, exactly which rows block it.
