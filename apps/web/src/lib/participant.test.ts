import { describe, expect, it } from "vite-plus/test"

import {
  REQUIRED_DOCUMENT_KINDS,
  applicationPackageProgress,
  applicationPackageState,
  applicationSubject,
} from "./participant"

import { overwriteGetLocale } from "#/paraglide/runtime"

/**
 * Что карточка заявки говорит о пакете документов (FR-401, п. 46).
 *
 * Признак `package_complete` отвечает «требование пакета выполнено», а не
 * «пять PDF на месте»: у заявки без обязательного пакета он поднят при нуле
 * вложений. Карточка читала его как «собран» и уверяла участника, что
 * обязательный пакет из пяти PDF собран, строкой выше «Файлы не приложены».
 */

const file = { id: "01a0634c", filename: "prilozhenie-2.pdf" }

describe("состояние пакета заявки", () => {
  it("признак поднят и вложения есть - пакет собран", () => {
    expect(
      applicationPackageState({ package_complete: true, files: [file] })
    ).toBe("complete")
  })

  it("признак поднят при нуле вложений - пакет не требовался", () => {
    expect(applicationPackageState({ package_complete: true, files: [] })).toBe(
      "not_required"
    )
  })

  it("признак не поднят - пакет неполон, сколько бы файлов ни было", () => {
    expect(
      applicationPackageState({ package_complete: false, files: [] })
    ).toBe("incomplete")
    expect(
      applicationPackageState({ package_complete: false, files: [file] })
    ).toBe("incomplete")
  })
})

describe("собранность пакета", () => {
  const of = (...kinds: string[]) => ({
    files: kinds.map((document_kind, index) => ({
      ...file,
      id: `0${index}`,
      document_kind,
    })),
  })

  it("считает закрытые позиции, а не файлы", () => {
    expect(applicationPackageProgress(of())).toStrictEqual({
      done: 0,
      total: 5,
    })
    expect(
      applicationPackageProgress(of("application_form", "tax_clearance"))
    ).toStrictEqual({ done: 2, total: 5 })
  })

  it("пять копий одной справки пакета не собирают", () => {
    expect(
      applicationPackageProgress(
        of(
          "application_form",
          "application_form",
          "application_form",
          "application_form",
          "application_form"
        )
      )
    ).toStrictEqual({ done: 1, total: 5 })
  })

  it("вложения старых заявок (legacy) в счет не идут", () => {
    expect(applicationPackageProgress(of("legacy", "legacy"))).toStrictEqual({
      done: 0,
      total: 5,
    })
  })

  it("все пять типов - пакет собран", () => {
    expect(
      applicationPackageProgress(of(...REQUIRED_DOCUMENT_KINDS))
    ).toStrictEqual({ done: 5, total: 5 })
  })
})

/**
 * Название заявки в списках участника (G-083).
 *
 * «Мои заявки» показывали восемь шестнадцатеричных цифр идентификатора:
 * заявки по разным тендерам выглядели строками-двойниками, и какая из них
 * какая - узнать было негде. Строка собирается из полей самой заявки, а
 * казахское название берется сохраненным, не машинным переводом.
 */
function withLocale<T>(locale: "ru" | "kk" | "en", body: () => T): T {
  overwriteGetLocale(() => locale)
  try {
    return body()
  } finally {
    overwriteGetLocale(() => "ru")
  }
}

describe("предмет заявки", () => {
  const application = {
    tender_title: "Аренда аудитории 42 м²",
    tender_title_kk: "42 м² аудиторияны жалға алу",
    lot_seq: 3,
    object_name: "Аудитория 42 м², корпус А",
  }

  it("русская локаль: название, лот и объект в одной строке", () => {
    const subject = withLocale("ru", () => applicationSubject(application))
    expect(subject).toContain("Аренда аудитории 42 м²")
    expect(subject).toContain("3")
    expect(subject).toContain("Аудитория 42 м², корпус А")
  })

  it("на kk берется сохраненное казахское название, а не русское", () => {
    const subject = withLocale("kk", () => applicationSubject(application))
    expect(subject).toContain("42 м² аудиторияны жалға алу")
    expect(subject).not.toContain("Аренда аудитории")
  })

  it("идентификатора заявки в строке нет вовсе", () => {
    const subject = withLocale("ru", () => applicationSubject(application))
    expect(subject).not.toMatch(/[0-9a-f]{8}/u)
  })
})
