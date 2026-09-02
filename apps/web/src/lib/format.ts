import { getLocale } from "#/paraglide/runtime"

// NFR-03: юридически значимое время - серверное (UTC), отображение - Астана.
//
// Зона задана постоянным смещением, а не именем `Asia/Almaty`, и вот почему.
// Казахстан с 01.03.2024 живет в одном поясе UTC+5, но база часовых поясов
// в рантайме сервера от этого отстает: SSR идет под bun (`oven/bun:1.3` и в
// прод-образе, `infra/docker/web.Dockerfile`), а он для `Asia/Almaty` дает
// UTC+6 - и на 2026 год, и на 2023-й, то есть таблицу зон до 2024a. Node на
// том же вводе дает верные UTC+5.
//
// Цена ошибки видна на самой странице: срок приема заявок печатался на час
// позже настоящего, а после гидратации браузер (со свежей таблицей) рисовал
// уже другое время - то есть юридически значимый срок менялся на глазах.
// Рядом, в `lib/organizer.ts`, ввод дат кабинета всегда считался по
// фиксированному `+05:00` - две половины одного экрана расходились на час.
//
// `Etc/GMT-5` - это именно UTC+5 (знак в POSIX-именах инвертирован); такие
// зоны не зависят от версии таблицы и в bun, и в node дают одно и то же.
// Постоянное смещение здесь безопасно: перевода часов в стране нет.
const DISPLAY_TZ = "Etc/GMT-5"

/**
 * Подпись зоны. Смещение постоянно - постоянна и подпись: короткое имя
 * `Etc/GMT-5` одинаково во всех трех локалях, и спрашивать его у Intl
 * значило бы добавить к выводу еще одну зависимость от таблиц браузера.
 */
const TZ_LABEL = "GMT+5"

/**
 * Числовые части момента в зоне отображения.
 *
 * Локаль форматтера фиксирована и к языку интерфейса отношения не имеет:
 * у Intl спрашиваются только числа, а порядок и разделители собираются здесь
 * же вручную. Причина - не в Intl, а в данных ICU браузера: сборки Chrome и
 * Edge везут `kk` по корневому шаблону, и «2 қыркүйек 2026 ж.» после
 * гидратации превращалось в «2026 M09 2» (в Edge - в английский месяц),
 * тогда как node с полным ICU печатал то же время верно. То есть один и тот
 * же юридически значимый срок читался на сервере и в браузере по-разному:
 * сперва React ловил несовпадение разметки, а затем дата менялась на глазах.
 * Числовая запись `dd.MM.yyyy` от состава ICU не зависит вовсе и одинакова
 * в ru, kk и en.
 *
 * `hourCycle: "h23"` - чтобы полночь печаталась «00:00», а не «24:00»
 * (`hour12: false` в части сборок дает именно h24).
 */
const ZONE_PARTS = new Intl.DateTimeFormat("en-US", {
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit",
  hourCycle: "h23",
  timeZone: DISPLAY_TZ,
})

function zoneParts(iso: string): { date: string; time: string } {
  const parts = new Map(
    ZONE_PARTS.formatToParts(new Date(iso)).map((part) => [
      part.type,
      part.value,
    ])
  )
  const at = (type: Intl.DateTimeFormatPartTypes) => parts.get(type) ?? ""
  return {
    date: `${at("day")}.${at("month")}.${at("year")}`,
    time: `${at("hour")}:${at("minute")}`,
  }
}

/**
 * Момент времени с подписью зоны.
 *
 * Зона обязана быть видна: сроки подачи заявок и торгов юридически значимы,
 * а печатались в Asia/Almaty без единого признака - читатель из другой зоны
 * (и даже из Актобе, UTC+5 против UTC+6 до 2024 г.) принимал их за местные.
 */
export function formatDateTime(iso: string | null | undefined): string | null {
  if (!iso) return null
  const { date, time } = zoneParts(iso)
  return `${date} ${time} ${TZ_LABEL}`
}

/** Дата без времени: сроки хранения и прочие «до такого-то числа». */
export function formatDate(iso: string | null | undefined): string | null {
  if (!iso) return null
  return zoneParts(iso).date
}

/** Убирает незначащие нули десятичной строки Decimal ("0.500000" → "0.5"). */
export function trimZeros(value: string): string {
  if (!value.includes(".")) return value
  return value.replace(/0+$/, "").replace(/\.$/, "")
}

/**
 * Денежные суммы контракта приходят строками ("21000").
 *
 * `currencyDisplay: "narrowSymbol"` - иначе на локали ru Intl печатает код
 * «KZT» вместо «₸»: тот же лот выглядел как «52 495,00 KZT» по-русски и
 * «52 495,00 ₸» по-казахски.
 */
export function formatTenge(amount: string): string {
  const value = Number(amount)
  if (!Number.isFinite(value)) return amount
  return new Intl.NumberFormat(getLocale(), {
    style: "currency",
    currency: "KZT",
    currencyDisplay: "narrowSymbol",
    maximumFractionDigits: 2,
  }).format(value)
}
