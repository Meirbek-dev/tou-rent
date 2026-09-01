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
 * Момент времени с подписью зоны.
 *
 * Зона обязана быть видна: сроки подачи заявок и торгов юридически значимы,
 * а печатались в Asia/Almaty без единого признака - читатель из другой зоны
 * (и даже из Актобе, UTC+5 против UTC+6 до 2024 г.) принимал их за местные.
 * `timeZoneName` не сочетается с `dateStyle`/`timeStyle` - Intl бросает
 * TypeError, - поэтому части даты перечислены поэлементно; вывод при этом
 * совпадает с прежним во всех трех локалях, к нему лишь добавляется «GMT+5».
 */
export function formatDateTime(iso: string | null | undefined): string | null {
  if (!iso) return null
  return new Intl.DateTimeFormat(getLocale(), {
    year: "numeric",
    month: "long",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
    timeZone: DISPLAY_TZ,
    timeZoneName: "short",
  }).format(new Date(iso))
}

/** Дата без времени: сроки хранения и прочие «до такого-то числа». */
export function formatDate(iso: string | null | undefined): string | null {
  if (!iso) return null
  return new Intl.DateTimeFormat(getLocale(), {
    dateStyle: "long",
    timeZone: DISPLAY_TZ,
  }).format(new Date(iso))
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
