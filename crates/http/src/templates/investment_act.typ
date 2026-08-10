// Акт приемки инвестиций комиссией (FR-1204, п. 92). Данные - data.json,
// предформатированы сервером (ru, NFR-01).
// TODO-ENGINEER: форма акта приемки в Правилах не приведена - состав полей
// выведен из п. 92 и подлежит сверке (Q-014).
#set page(paper: "a4", margin: (x: 1.8cm, y: 1.6cm))
#set text(font: "Libertinus Serif", size: 10.5pt, lang: "ru")
#let d = json("data.json")

#align(center)[
  #text(weight: "bold", size: 13pt)[АКТ ПРИЕМКИ ИНВЕСТИЦИЙ №~#d.number]
]

#v(0.6em)
#grid(
  columns: (1fr, 1fr),
  [г. Павлодар],
  align(right)[#d.act_date],
)

#v(0.9em)
Комиссия НАО «Торайгыров университет» приняла инвестиции по инвестиционному
договору с #d.tenant.

#v(0.9em)
#grid(
  columns: (16em, 1fr),
  row-gutter: 0.5em,
  [Объект:], [#d.object],
  [Объем по договору:], [#d.promised ₸],
  [Принято настоящим актом:], [#d.accepted ₸],
  [Принято всего:], [#d.accepted_total ₸],
  [Исполнение:], [#if d.complete [обязательства исполнены полностью] else [частичное]],
)

#if d.note != "" [
  #v(0.9em)
  #text(weight: "bold")[Примечание]
  #v(0.3em)
  #d.note
]

#v(1.6em)
#grid(
  columns: (1fr, 1fr),
  [Комиссия: #d.accepted_by],
  [Подпись: #box(width: 1fr, repeat[.])],
)

#v(0.6em)
#text(size: 9pt)[
  Акт публикуется в порядке п. 92 Правил. Печатная форма подготовлена системой TOU.Rent.
]
