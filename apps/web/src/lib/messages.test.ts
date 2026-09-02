import { readFileSync, readdirSync } from "node:fs"
import { join } from "node:path"

import { describe, expect, it } from "vite-plus/test"

import en from "#/../messages/en.json" with { type: "json" }
import kk from "#/../messages/kk.json" with { type: "json" }
import ru from "#/../messages/ru.json" with { type: "json" }

/**
 * Полнота переводов (T15, гейт G14). ru - эталон контура 1 (NFR-01):
 * каждый его ключ обязан существовать в kk и en с теми же параметрами
 * подстановки и быть переведенным, а не скопированным из ru.
 *
 * Проверка сперва читала только девятнадцать файлов публичного портала -
 * кабинеты в нее не входили, и четырнадцать kk-значений там байт-в-байт
 * совпадали с русскими. Теперь охвачен весь `src`, а совпадение с ru
 * приходится объявлять поименно и с причиной.
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
 * нумерации, единица измерения, обозначения множителей Прил. 4 и термины,
 * заимствованные казахским как есть. Причина - в значении: каждая строчка
 * здесь обязана объясняться, иначе список превращается в свалку непереведенного.
 */
const SAME_AS_RU_ALLOWED = new Map<string, string>([
  ["app_name", "бренд - не переводится"],
  ["lot_seq", "«№» - символ нумерации"],
  ["journal_seq", "«№» - символ нумерации"],
  ["object_area_value", "«{area} м²» - единица измерения"],
  ["tender_card_title", "«тендер» - заимствование, kk то же слово"],
  ["tender_id_label", "«тендер» - заимствование"],
  ["admin_schedule_tender", "«тендер» - заимствование"],
  ["tender_lot_legend", "«лот» - заимствование (ср. tenders_lots_count)"],
  ["acts_pdf", "«акт» - заимствование (ср. contract_acts_title «Актілер»)"],
  ["meeting_commission", "«комиссия» - заимствование (ср. meeting_title)"],
  ["meeting_quorum", "«кворум» - заимствование (ср. meeting_open_first)"],
  ["commission_quorum_required", "«кворум» - заимствование"],
  ["calc_option", "«опция» - заимствование (ср. admin_pick_option)"],
  ["admin_option", "«опция» - заимствование (ср. admin_pick_option)"],
  [
    "org_nav_calculator",
    "«калькулятор» - заимствование (ср. land_contract_hint)",
  ],
  ["file_upload_label", "«файл» - заимствование (ср. application_files_empty)"],
  ["benefit_scheme_label", "«схема» - заимствование (ср. benefit_title)"],
  ["rate_coef_kt", "обозначение множителя Прил. 4 - символ формулы"],
  ["rate_coef_kk", "обозначение множителя Прил. 4 - символ формулы"],
  ["rate_coef_ksk", "обозначение множителя Прил. 4 - символ формулы"],
  ["rate_coef_kr", "обозначение множителя Прил. 4 - символ формулы"],
  ["rate_coef_kvd", "обозначение множителя Прил. 4 - символ формулы"],
  ["rate_coef_kopf", "обозначение множителя Прил. 4 - символ формулы"],
  ["rate_coef_kfu", "обозначение множителя Прил. 4 - символ формулы"],
  ["rate_coef_ksots", "обозначение множителя Прил. 4 - символ формулы"],
  ["rate_coef_k", "обозначение множителя Прил. 4 - символ формулы"],
  ["rate_coef_kn", "обозначение множителя Прил. 4 - символ формулы"],
  ["rate_coef_kv", "обозначение множителя Прил. 4 - символ формулы"],
])

const SRC = join(import.meta.dirname, "..")

/** Исходники интерфейса: весь `src`, кроме сгенерированного Paraglide. */
function sources(dir: string = SRC): string[] {
  const found: string[] = []
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name)
    if (entry.isDirectory()) {
      if (entry.name === "paraglide") continue
      found.push(...sources(path))
    } else if (/\.tsx?$/.test(entry.name)) {
      found.push(path)
    }
  }
  return found
}

/** Ключи, которые интерфейс действительно вызывает (`m.<key>`). */
function usedKeys(): Set<string> {
  const used = new Set<string>()
  for (const file of sources()) {
    const source = readFileSync(file, "utf8")
    for (const match of source.matchAll(/\bm\.([a-z0-9_]+)/gi)) {
      const key = match[1]
      if (key !== undefined && key in messages.ru) used.add(key)
    }
  }
  return used
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

    it(`${locale}: строки интерфейса переведены, а не скопированы из ru`, () => {
      const untranslated = [...usedKeys()].filter(
        (key) =>
          !SAME_AS_RU_ALLOWED.has(key) &&
          messages[locale][key] === messages.ru[key]
      )
      expect(untranslated.toSorted()).toEqual([])
    })

    it(`${locale}: нет пустых строк`, () => {
      const empty = keys(messages[locale]).filter(
        (key) => (messages[locale][key] ?? "").trim() === ""
      )
      expect(empty).toEqual([])
    })
  }

  it("интерфейс вообще пользуется ключами", () => {
    expect(usedKeys().size).toBeGreaterThan(500)
  })

  it("мертвых ключей нет: каждый ключ ru кто-то вызывает", () => {
    const used = usedKeys()
    const dead = keys(messages.ru).filter((key) => !used.has(key))
    expect(dead.toSorted()).toEqual([])
  })

  it("список допущенных совпадений не протух", () => {
    const used = usedKeys()
    const stale = [...SAME_AS_RU_ALLOWED.keys()].filter(
      (key) => !used.has(key) || messages.kk[key] !== messages.ru[key]
    )
    expect(stale).toEqual([])
  })
})
