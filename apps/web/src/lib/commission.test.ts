import { describe, expect, it } from "vite-plus/test"

import { applicantName, commissionSeat, ownVote } from "./commission"

import type { ApplicationVotes, CommissionDto } from "./commission"

/**
 * Гаунтлет G-113: резервный член комиссии получал полный комплект кнопок
 * голосования, а отказ 409 приходил только после подтверждения
 * «необратимого» голоса. Право голоса определяется составом (п. 9–11), и
 * знать его экран обязан до первого щелчка, а не после.
 */

const COMMISSION: CommissionDto = {
  id: "c1",
  name: "Тендерная комиссия",
  valid_from: "2026-01-01",
  valid_until: "2026-12-31",
  approved: true,
  voting_total: 7,
  quorum_required: 5,
  members: [
    {
      member_id: "m1",
      user_id: "u1",
      full_name: "Председатель",
      member_role: "chairman",
    },
    {
      member_id: "m8",
      user_id: "u8",
      full_name: "Резервный",
      member_role: "reserve",
    },
  ],
}

const VOTES: ApplicationVotes = {
  tally: { votes_for: 1, votes_against: 0, eligible: 7, outcome: null },
  votes: [
    {
      member_id: "m1",
      member_name: "Председатель",
      value: "for",
      dissent: null,
    },
  ],
}

describe("место пользователя в составе комиссии", () => {
  it("находится по user_id, а не по роли кабинета", () => {
    expect(commissionSeat(COMMISSION, "u8")?.member_role).toBe("reserve")
  })

  it("пользователь вне состава места не имеет", () => {
    expect(commissionSeat(COMMISSION, "u42")).toBeNull()
    expect(commissionSeat(COMMISSION, undefined)).toBeNull()
    expect(commissionSeat(null, "u1")).toBeNull()
  })
})

describe("свой голос по заявке", () => {
  it("узнается по member_id - повторно голосовать нельзя (п. 13–14)", () => {
    expect(ownVote(VOTES, "m1")?.value).toBe("for")
  })

  it("чужие голоса своими не считаются", () => {
    expect(ownVote(VOTES, "m8")).toBeNull()
    expect(ownVote(VOTES, undefined)).toBeNull()
    expect(ownVote(null, "m1")).toBeNull()
  })
})

/**
 * Гаунтлет G-116: таблица заявок и журнал регистрации опознавали заявки
 * восемью символами UUIDv7 - у всех строк одинаковыми, потому что заявки
 * одного тендера создаются в одну и ту же миллисекунду шкалы v7.
 */
describe("имя заявителя в строке реестра", () => {
  it("берется из данных заявки, а не из идентификатора", () => {
    expect(
      applicantName({
        id: "01a063ed-d8b2-74d6-9059-27e5f0783041",
        applicant_details: { name: "ТОО «Демо-Партнер»", id_number: "123" },
      })
    ).toBe("ТОО «Демо-Партнер»")
  })

  it("без имени остается короткий идентификатор - строка не бывает пустой", () => {
    expect(
      applicantName({ id: "01a063ed-d8b2", applicant_details: { name: "  " } })
    ).toBe("01a063ed")
    expect(applicantName({ id: "01a063ed-d8b2" })).toBe("01a063ed")
    expect(
      applicantName({ id: "01a063ed-d8b2", applicant_details: null })
    ).toBe("01a063ed")
  })
})
