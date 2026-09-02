import { describe, expect, it } from "vite-plus/test"

import { applicationPackageState } from "./participant"

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
