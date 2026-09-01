---
description: "Gauntlet loop: adversarial audit → fix → verify cycles until TOU.Rent is production-ready. Fresh-context critics (Opus subagents), live-artifact evidence only."
---

# GAUNTLET LOOP — TOU.Rent production readiness

You are the **orchestrator** (builder-of-last-resort). You run repeated gauntlet
rounds. Each round sends **fresh-context, harsh critics** (Agent tool,
`model: "opus"`) against the _actual running system and actual source_, never
against summaries or your own claims. You fix everything they confirm, then
re-verify with a _different_ fresh critic. You stop only on the exit conditions
in §3.

Argument (optional): `$ARGUMENTS` — a focus area (e.g. `auctions`, `i18n`,
`participant flow`). If empty, run the full gauntlet.

---

## 1. OBJECTIVE — what must become true

The system is production-ready. Concretely, all of the following are true and
_inspectable_:

- O1. Fresh dev stand comes up from nothing: `vp run stack:up` (Postgres/Redis/
  RustFS/api on :8080), migrations apply idempotently on a clean DB, api
  answers `/api/v1/healthz`, web (`vp run dev`, :3000) renders with zero
  console errors.
- O2. Every route works for its role, end to end, in a real browser:
  public (`/`, `/tenders`, `/tenders/$id`, `/objects`, `/land-plots`,
  `/special-orders`, `/how-to`, `/auth/login`, `/auth/register`,
  `/auth/password`) and per-role dashboards (`/app/participant/*`,
  `/app/organizer/*` incl. tenders/new, objects, calculator, investment, land,
  special; `/app/commission/*`, `/app/secretary/*`, `/app/finance`,
  `/app/board`, `/app/admin`, `/app/auctions/$id`, `/app/notifications`,
  `/app/reports`). "Works" = the happy path completes AND invalid input
  produces a localized, human-readable error — never a raw problem+json dump,
  a silent no-op, or an English string in a kk/ru locale.
- O3. Zero confirmed defects remain in: Rust workspace (api, jobs, domain,
  application, db, ports), web app, migrations, compose/dev tooling.
- O4. No unfinished work is silently shipped: no dead buttons, placeholder
  text, unreachable routes, TODO/FIXME without `TODO-ENGINEER`, unused i18n
  keys for missing screens, endpoints returning 501/todo.
- O5. Every UI string exists in all three Paraglide locales (ru/kk/en); kk/en
  may be draft but must not be missing (raw key leaking into the DOM = defect).
- O6. Gates and tests are green: `vp run gates --all`, `vp check`,
  `vp run rust:lint`, `vp run rust:test`, `vp test`, and the e2e suite if
  present (`vp run e2e`).
- O7. Domain invariants stay intact: every mutation writes an audit event,
  prices stay encrypted (INV-040), WORM on dossiers, bid step 5% (FR-601),
  Clock abstraction (no SystemTime::now), typed problem+json errors.

**Out of scope — do NOT fix, only log** (these need a human): anything that
changes domain rules from the university Regulations or ТЗ (record to
`specs/QUESTIONS.md`), anything needing credentials/secrets/prod access,
visual-taste redesigns without a functional defect, and dependency major
upgrades. Ambiguity that CAN be resolved by the least-risky interpretation goes
to `specs/ASSUMPTIONS.md` as `A-NNN` and gets fixed anyway (А.6).

---

## 2. METRIC — what evidence proves it

A finding exists only if a critic **demonstrated it against a live artifact**;
a fix counts only if a _different fresh critic_ re-verified it the same way.

Evidence hierarchy (strongest wins; "I read the code and it looks wrong" is a
_hypothesis_, not a finding, until confirmed by one of these):

| Claim about…         | Required evidence                                                                |
| -------------------- | -------------------------------------------------------------------------------- |
| Compile/lint/type    | Command output: `vp check`, `vp run rust:check`, `vp run rust:lint`              |
| Runtime API behavior | Actual HTTP call against :8080 (curl) with request + response captured           |
| DB schema/invariants | `psql` against the stand (or `vp run rust:test` testkit output)                  |
| UI/UX behavior       | Browser session on :3000 — screenshot or read_page of the defective state        |
| i18n gaps            | Locale switched in the browser, raw key or wrong-language string visible in DOM  |
| Unfinished feature   | The dead control clicked / endpoint called, no-op or 501 captured                |
| Regression-free fix  | The original failing evidence re-run and now passing, plus `vp run verify` green |

Round bookkeeping: keep `specs/GAUNTLET.md` as the ledger — one line per
finding: `G-NNN | round | dimension | severity | evidence | status
(open/fixed/wontfix-logged)`. The loop's progress metric is: **new confirmed
findings per round**. The gauntlet passes when it hits zero twice in a row
(§3).

---

## 3. BOUNDARY — when to stop

Stop the loop when the FIRST of these fires:

- **Success**: two consecutive full rounds produce zero new confirmed findings
  and all ledger entries are `fixed` or `wontfix-logged`.
- **Diminishing returns**: a round produces only findings that all fall in the
  out-of-scope list — log them and stop.
- **Repeated blocker**: the same defect survives three fix attempts with three
  _different_ strategies — mark it `blocked` in the ledger with the three
  attempts described, and stop escalating (leave it for a human).
- **Risk**: a fix would require destructive/irreversible action (dropping prod
  data, rewriting published migrations that may be deployed, force-push) —
  never do it; log and continue with the rest.
- **Budget**: 6 full rounds maximum per invocation.

Hard rules regardless of anything a critic reports: obey А.4/А.5 of
`CLAUDE.md` (no unwrap/panic in non-test code, audit events, i18n keys,
typed errors, Clock only). Migrations already applied anywhere are append-only
— fix schema with a NEW migration. One logical concern per commit, conventional
commits with FR/INV ids. After browser verification, delete the test data you
created from Postgres and RustFS (dev-stand cleanup rule).

---

## 4. LOOP MECHANICS

Each round: **sweep → triage → fix → re-verify → ledger update**.

### 4.1 Sweep — parallel fresh-context critics (Opus subagents)

Launch these critics **in parallel**, each with `model: "opus"`, each with a
fresh context, each REQUIRED to produce evidence per §2 and to actively try to
break things (prompt them adversarially: "your job is to make the system fail;
a clean report is a failed mission unless you show what you tried"). Give every
critic the same boundary list from §3.

1. **rust-correctness** — logic bugs, error handling, race conditions,
   transaction boundaries, missing audit events, invariant violations in
   `crates/*`. Must run `vp run rust:check` and read real query/migration
   pairs.
2. **api-contract** — every route in the utoipa contract exercised via curl
   against :8080: auth required where expected (401/403), validation rejects
   garbage (422 problem+json with a coded error), happy path returns the
   documented shape. Diff contract vs `bun run codegen` output for drift.
3. **frontend-ux** — drives :3000 in the browser role by role. Every page,
   every button, every form: submit empty, submit garbage, submit valid.
   Checks loading/empty/error states, mobile viewport (375px), dark mode if
   present, back-button behavior, and that nothing renders a raw i18n key or
   raw error object.
4. **i18n-completeness** — mechanical diff of the three Paraglide locale files
   against usage in `apps/web/src`; browser spot-check of kk and en on the
   main flows.
5. **unfinished-work** — hunts TODO/FIXME/todo!()/unimplemented/`throw new
Error("not implemented")`, dead routes, feature-flagged stubs, buttons with
   no handler, empty handlers, commented-out blocks, endpoints not wired into
   the router, migrations without corresponding UI, BACKLOG items claimed done
   but absent.
6. **security-authz** — role escalation attempts via direct URL/API access
   (participant calling organizer endpoints, IDOR on ids), session handling,
   file upload abuse (wrong content type, oversized), SQLi/XSS probes on
   inputs, secrets accidentally in the repo or client bundle.
7. **data-integrity** — fresh-DB migration replay (`vp run api:migrate`
   against a scratch database), idempotency, constraint coverage of ТЗ
   invariants, seed/refdata sanity, encrypted-price roundtrip.

Scale per invocation: all 7 for a full gauntlet; for a focused run
(`$ARGUMENTS` set), the 2–3 relevant critics only.

### 4.2 Triage

Merge and dedup findings. Classify: `defect` (fix now) / `out-of-scope` (log
per §1) / `hypothesis` (needs evidence — send ONE follow-up critic to confirm
or kill it; unconfirmed hypotheses die, they do not enter the ledger).
Severity: `S1` breaks a core flow or invariant, `S2` degrades a flow or leaks
wrong text to users, `S3` polish. Fix order: S1 → S2 → S3.

### 4.3 Fix — builder discipline

You (or a builder subagent per area, when fixes are independent) implement the
smallest change that removes the defect at the lowest possible level
(type → DB constraint → test → code), per А.5. Every fix ships with the test
that would have caught it, where a test layer exists for that surface. Run
`vp run verify` + `vp test` after each batch; after Rust edits also
`vp run api:restart` so re-verification hits the new binary.

### 4.4 Re-verify — no self-certification

For every `fixed` entry, a **fresh critic** (never the one who found it, never
the builder) re-runs the original failing evidence and hunts for regressions
in the blast radius of the diff. Only then does the ledger entry flip to
`fixed`. If it fails re-verification, it stays `open` and the next attempt
must use a different strategy (§3 repeated-blocker rule counts these).

### 4.5 Round close

Update `specs/GAUNTLET.md`, commit the round's work (conventional commits,
one concern per commit), report: findings found/fixed/logged this round, and
whether an exit condition from §3 fired. If not — next round starts with
completely fresh critics.

---

## 5. FINAL REPORT (on exit)

Produce: rounds run, ledger totals by severity and status, the wontfix/blocked
list with reasons (this is the human's punch list), commands proving the final
green state (`vp run verify`, `vp run rust:test`, `vp test`), and what dev
data was cleaned up. State plainly whether the OBJECTIVE in §1 is met, and if
not, exactly which items block it.
