// Протокол о победителе № 2 при уклонении победителя (FR-903, п. 116–118).
// Данные - data.json, значения предформатированы сервером (ru - печатные
// формы контура 1, NFR-01).
// TODO-ENGINEER: сверить состав полей и формулировки с утвержденной формой
// (первоисточник-PDF недоступен, Q-006).
#set page(paper: "a4", margin: (x: 1.8cm, y: 1.6cm))
#set text(font: "Libertinus Serif", size: 10.5pt, lang: "ru")
#let d = json("data.json")

#align(center)[
  #text(weight: "bold", size: 13pt)[ПРОТОКОЛ №~#d.number] \
  об уклонении победителя тендера и определении участника № 2 \
  НАО «Торайгыров университет»
]

#v(0.9em)
#grid(
  columns: (16em, 1fr),
  row-gutter: 0.5em,
  [Тендер:], [#d.tender_number - #d.tender_title],
  [Дата и время заседания:], [#d.held_at],
  [Место проведения:], [#d.place],
  [Комиссия:], [#d.commission],
  [Срок оформления протокола:], [#d.deadline],
)

#v(0.9em)
#text(weight: "bold")[Уклонение от подписания договора (п. 116)]
#v(0.3em)
#table(
  columns: (auto, 1fr, auto, auto, auto),
  inset: 5pt,
  align: (center, left, left, left, left),
  table.header([№], [Уклонившийся], [Место], [Основание], [Дата]),
  ..d
    .evasions
    .enumerate()
    .map(((i, e)) => (
      [#(i + 1)],
      [#e.name],
      [#e.place],
      [#e.ground],
      [#e.declared_at],
    ))
    .flatten(),
)

#v(0.9em)
#text(weight: "bold")[Последствия (п. 116–118)]
#v(0.3em)
- Гарантийный взнос уклонившегося удерживается (п. 116).
- Право на заключение договора переходит к участнику № 2 с его ставкой (п. 117).
- Участник № 2 уведомляется не позднее следующего рабочего дня (п. 118);
  сроки заключения договора для него - те же (п. 110–115).

#v(0.9em)
#text(weight: "bold")[Участник № 2 по лотам]
#v(0.3em)
#if d.lots.len() == 0 [
  Участник № 2 по лотам отсутствует: тендер признается несостоявшимся (п. 81.4).
] else [
  #table(
    columns: (auto, 1fr, 1fr, auto),
    inset: 5pt,
    align: (center, left, left, right),
    table.header([Лот], [Объект], [Участник № 2], [Ставка, ₸]),
    ..d
      .lots
      .enumerate()
      .map(((i, l)) => ([#l.seq], [#l.object], [#l.runner_up], [#l.amount]))
      .flatten(),
  )
]

#v(0.9em)
#text(weight: "bold")[Состав комиссии на заседании]
#v(0.3em)
#table(
  columns: (auto, 1fr, auto),
  inset: 5pt,
  align: (center, left, left),
  table.header([№], [ФИО], [Роль в комиссии]),
  ..d
    .members
    .enumerate()
    .map(((i, m)) => ([#(i + 1)], [#m.name], [#m.role]))
    .flatten(),
)

#v(1.6em)
#grid(
  columns: (1fr, 1fr),
  [Председательствующий: #box(width: 1fr, repeat[.])],
  [Секретарь комиссии: #box(width: 1fr, repeat[.])],
)

#v(0.6em)
#text(size: 9pt)[Протокол сформирован системой TOU.Rent #d.generated_at]
