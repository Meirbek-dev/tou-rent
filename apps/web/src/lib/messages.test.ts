import { readFileSync, readdirSync } from "node:fs"
import { join } from "node:path"

import { describe, expect, it } from "vite-plus/test"

import en from "#/../messages/en.json" with { type: "json" }
import kk from "#/../messages/kk.json" with { type: "json" }
import ru from "#/../messages/ru.json" with { type: "json" }

/**
 * Полнота переводов (T15, гейт G14). ru - эталон контура 1 (NFR-01):
 * каждый его ключ обязан существовать в kk и en с теми же параметрами
 * подстановки, а строки публичного портала - быть переведенными, а не
 * скопированными из ru.
 */

type Messages = Record<string, string>

const messages: Record<"ru" | "kk" | "en", Messages> = {
  ru: ru,
  kk: kk,
  en: en,
}

/** `$schema` - служебное поле файла сообщений, не строка интерфейса. */
const keys = (locale: Messages): string[] =>
  Object.keys(locale).filter((key) => !key.startsWith("$"))

/**
 * Совпадение с ru допустимо там, где перевода не существует: бренд, символ
 * нумерации, заимствованный термин «тендер» (одинаков в ru и kk), единица
 * измерения «{area} м²» и обозначения множителей Прил. 4 (Кт, Кк, ... —
 * предметные символы формулы FR-201, кириллица общая для ru и kk).
 */
const SAME_AS_RU_ALLOWED = new Set([
  "app_name",
  "lot_seq",
  "tender_card_title",
  "tender_id_label",
  "object_area_value",
  "rate_coef_kt",
  "rate_coef_kk",
  "rate_coef_ksk",
  "rate_coef_kr",
  "rate_coef_kvd",
  "rate_coef_kopf",
  "rate_coef_kfu",
  "rate_coef_ksots",
  "rate_coef_k",
  "rate_coef_kn",
  "rate_coef_kv",
])

/** Файлы публичного портала: страницы без авторизации и их компоненты. */
const PUBLIC_SOURCES = [
  "src/routes/index.tsx",
  "src/routes/how-to.tsx",
  "src/routes/land-plots.tsx",
  "src/routes/special-orders.tsx",
  "src/routes/objects/index.tsx",
  "src/components/object-status-badge.tsx",
  "src/routes/tenders/index.tsx",
  "src/routes/tenders/$tenderId.tsx",
  "src/routes/auth/login.tsx",
  "src/routes/auth/register.tsx",
  "src/components/site-header.tsx",
  "src/components/site-footer.tsx",
  "src/components/theme-toggle.tsx",
  "src/components/tender-list-item.tsx",
  "src/components/tender-status-badge.tsx",
  "src/components/deadline-block.tsx",
  "src/components/registry-skeleton.tsx",
  "src/components/locale-switcher.tsx",
  "src/lib/how-to-steps.ts",
]

/** Ключи, которые реально использует публичный портал. */
function publicKeys(): string[] {
  const root = join(import.meta.dirname, "..", "..")
  const used = new Set<string>()
  for (const file of PUBLIC_SOURCES) {
    const source = readFileSync(join(root, file), "utf8")
    for (const match of source.matchAll(/\bm\.([a-z0-9_]+)/gi)) {
      const key = match[1]
      if (key !== undefined && key in messages.ru) used.add(key)
    }
  }
  return [...used].toSorted()
}

/** Параметры сообщения: `{count}`, `{months}` и т.д. */
const params = (value: string): string[] =>
  [...value.matchAll(/\{(\w+)\}/g)].map((match) => match[1] ?? "").toSorted()

describe("i18n: полнота переводов (NFR-01, G14)", () => {
  it("файлы локалей лежат рядом с проектом inlang", () => {
    const dir = join(import.meta.dirname, "..", "..", "messages")
    expect(readdirSync(dir).toSorted()).toEqual([
      "en.json",
      "kk.json",
      "ru.json",
    ])
  })

  for (const locale of ["kk", "en"] as const) {
    it(`${locale}: нет пропущенных ключей относительно ru`, () => {
      const missing = keys(messages.ru).filter(
        (key) => !(key in messages[locale])
      )
      expect(missing).toEqual([])
    })

    it(`${locale}: нет ключей сверх ru (мертвые переводы)`, () => {
      const extra = keys(messages[locale]).filter(
        (key) => !(key in messages.ru)
      )
      expect(extra).toEqual([])
    })

    it(`${locale}: параметры подстановки совпадают с ru`, () => {
      const mismatched = keys(messages.ru).filter((key) => {
        const source = messages.ru[key]
        const target = messages[locale][key]
        if (source === undefined || target === undefined) return false
        return params(source).join() !== params(target).join()
      })
      expect(mismatched).toEqual([])
    })

    it(`${locale}: строки публичного портала переведены`, () => {
      const untranslated = publicKeys().filter(
        (key) =>
          !SAME_AS_RU_ALLOWED.has(key) &&
          messages[locale][key] === messages.ru[key]
      )
      expect(untranslated).toEqual([])
    })

    it(`${locale}: нет пустых строк`, () => {
      const empty = keys(messages[locale]).filter(
        (key) => (messages[locale][key] ?? "").trim() === ""
      )
      expect(empty).toEqual([])
    })
  }

  it("публичный портал вообще пользуется ключами", () => {
    expect(publicKeys().length).toBeGreaterThan(50)
  })
})
