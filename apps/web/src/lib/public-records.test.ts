import { readFileSync } from "node:fs"
import { join } from "node:path"

import { describe, expect, it } from "vite-plus/test"

import { publicationSubject, publicRecordKindLabel } from "./public-records"

/**
 * Полнота подписей видов публикаций (FR-1403, NFR-01).
 *
 * Вид закрыт enum'ом `domain::publication::PublicRecordKind`, а до портала
 * доезжает строкой контракта вместе с русской подписью `kind_title_ru` -
 * компилятор такую пару не сверит. Пока сверки не было, портал рисовал
 * серверную подпись напрямую, и бейдж карточки оставался русским на /en/
 * и /kk/.
 *
 * Тест читает сам каталог домена, не поднимая ни Rust, ни React.
 */

const CATALOG = join(
  import.meta.dirname,
  "..",
  "..",
  "..",
  "..",
  "crates",
  "domain",
  "src",
  "publication.rs"
)

/** Коды видов из `PublicRecordKind::as_str`. */
function catalogKinds(): string[] {
  const source = readFileSync(CATALOG, "utf8")
  const body = source.slice(source.indexOf("pub fn as_str"))
  const arm = /PublicRecordKind::\w+ => "(\w+)"/g
  return [...body.matchAll(arm)].map((match) => match[1] as string)
}

/** Русская подпись сервера - то, от чего вид должен уйти. */
const SERVER_TITLE = "результаты особого порядка"

/**
 * Чем очередь публикаций различает соседние материалы (G-134).
 *
 * У обоснований ставки заголовок один на все договоры особого порядка, и
 * очередь из трех строк была неразличима. Реквизиты приходят по одному и
 * могут отсутствовать: номер до регистрации договора не присвоен, объекта
 * у заявки может не быть вовсе - подпись собирается из того, что есть.
 */
describe("подпись материала в очереди публикаций", () => {
  it("собирает номер, объект и сторону через разделитель", () => {
    expect(
      publicationSubject({
        reference: "ОП-2026/14",
        object_name: "Аудитория 214",
        party_name: "ТОО «Астана Строй»",
      })
    ).toBe("ОП-2026/14 · Аудитория 214 · ТОО «Астана Строй»")
  })

  it("пропускает то, чего у материала нет", () => {
    expect(
      publicationSubject({
        reference: null,
        object_name: "Аудитория 214",
        party_name: "  ",
      })
    ).toBe("Аудитория 214")
    expect(publicationSubject({})).toBe("")
  })
})

describe("виды публикаций особого порядка", () => {
  it("каталог видов читается и не пуст", () => {
    const kinds = catalogKinds()
    expect(kinds.length).toBeGreaterThanOrEqual(3)
    expect(new Set(kinds).size).toBe(kinds.length)
  })

  it("у каждого вида каталога есть своя подпись", () => {
    const untranslated = catalogKinds().filter(
      (kind) =>
        publicRecordKindLabel({ kind, kind_title_ru: SERVER_TITLE }) ===
        SERVER_TITLE
    )
    expect(untranslated).toEqual([])
  })

  it("подписи видов не повторяются", () => {
    const labels = catalogKinds().map((kind) =>
      publicRecordKindLabel({ kind, kind_title_ru: SERVER_TITLE })
    )
    expect(new Set(labels).size).toBe(labels.length)
  })

  it("вид вне каталога откатывается на серверную подпись", () => {
    expect(
      publicRecordKindLabel({
        kind: "kind_from_the_future",
        kind_title_ru: SERVER_TITLE,
      })
    ).toBe(SERVER_TITLE)
  })
})
