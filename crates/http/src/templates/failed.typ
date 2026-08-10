// Протокол о признании тендера несостоявшимся (FR-802, п. 81–83).
// Данные - data.json, значения предформатированы сервером (ru - печатные
// формы контура 1, NFR-01).
// TODO-ENGINEER: сверить состав полей и формулировки с утвержденной формой
// и нумерацией подпунктов п. 81 (первоисточник-PDF недоступен, Q-004).
#set page(paper: "a4", margin: (x: 1.8cm, y: 1.6cm))
#set text(font: "Libertinus Serif", size: 10.5pt, lang: "ru")
#let d = json("data.json")

#align(center)[
  #text(weight: "bold", size: 13pt)[ПРОТОКОЛ №~#d.number] \
  о признании тендера несостоявшимся \
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
#text(weight: "bold")[Основание признания несостоявшимся]
#v(0.3em)
#grid(
  columns: (16em, 1fr),
  row-gutter: 0.5em,
  [Пункт Правил:], [#d.ground_rule],
  [Основание:], [#d.ground_label],
  [Подано заявок:], [#d.applications],
  [Допущено участников:], [#d.admitted],
)

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

#v(0.9em)
#text(weight: "bold")[Заявки]
#v(0.3em)
#if d.applications_list.len() == 0 [
  Заявки не подавались.
] else [
  #table(
    columns: (auto, 1fr, auto, auto),
    inset: 5pt,
    align: (center, left, right, left),
    table.header([№], [Заявитель], [Цена], [Решение]),
    ..d
      .applications_list
      .enumerate()
      .map(((i, a)) => ([#(i + 1)], [#a.applicant], [#a.price], [#a.decision]))
      .flatten(),
  )
]

#v(0.9em)
#text(weight: "bold")[Следствие (п. 82–83)]
#v(0.3em)
#d.consequence_text

#v(1.6em)
#grid(
  columns: (1fr, 1fr),
  [Председательствующий: #box(width: 1fr, repeat[.])],
  [Секретарь комиссии: #box(width: 1fr, repeat[.])],
)

#v(0.6em)
#text(size: 9pt)[Протокол сформирован системой TOU.Rent #d.generated_at]
