// Форма объявления о тендере (Прил. 1, FR-303). Данные - из data.json,
// все значения предварительно отформатированы сервером (даты - Asia/Almaty).
// TODO-ENGINEER: сверить шапку, формулировки и реквизиты с Прил. 1 Правил
// (первоисточник-PDF агенту недоступен).
#set page(paper: "a4", margin: (x: 1.8cm, y: 1.6cm))
#set text(font: "Libertinus Serif", size: 10.5pt, lang: "ru")
#let d = json("data.json")

#if d.is_draft {
  align(
    center,
    text(fill: rgb("#b91c1c"), weight: "bold", size: 12pt)[ЧЕРНОВИК - объявление не опубликовано],
  )
  v(0.5em)
}

#align(center)[
  #text(weight: "bold", size: 13pt)[ОБЪЯВЛЕНИЕ] \
  о проведении тендера по предоставлению в имущественный наем (аренду) \
  объектов НАО «Торайгыров университет»
]

#v(0.9em)
#text(weight: "bold", size: 11.5pt)[#d.title]
#v(0.5em)

#grid(
  columns: (16em, 1fr),
  row-gutter: 0.5em,
  [Номер тендера:], [#d.number],
  [Дата публикации объявления:], [#d.published_at],
  [Прием заявок до:], [#d.submission_deadline],
  [Вскрытие конвертов с заявками:], [#d.opening_at],
  [Дата проведения торгов:], [#d.trading_at],
)

#v(0.9em)
#text(weight: "bold")[Перечень лотов]
#v(0.3em)
#set text(size: 9pt)
#table(
  columns: (auto, 1fr, auto, 1fr, auto, auto, auto),
  inset: 5pt,
  align: (center, left, center, left, center, right, right),
  table.header(
    [№],
    [Объект (адрес)],
    [Площадь, м²],
    [Целевое назначение],
    [Срок найма, мес.],
    [Стартовая ставка, тг],
    [Гарантийный взнос, тг],
  ),
  ..d
    .lots
    .map(l => (
      [#l.seq],
      [#l.object],
      [#l.area],
      [#l.purpose],
      [#l.lease_months],
      [#l.monthly #l.rate_unit],
      [#l.fee],
    ))
    .flatten(),
)
#set text(size: 10.5pt)

#v(0.6em)
- Ставки указаны без НДС (п. 140–143 Правил).
- Гарантийный взнос равен месячной базовой ставке аренды лота (п. 25 Правил);
  для почасовых лотов - стоимости разыгрываемого объема часов (п. 97, FR-205).

#if d.viewings.len() > 0 [
  #v(0.3em)
  Срок и условия осмотра объектов:
  #for v in d.viewings [
    - Лот №#v.seq: #v.text
  ]
]

#v(1.2em)
#text(
  fill: luma(80),
  size: 9pt,
)[Реквизиты и контакты организатора уточняются юридической службой (заполняется по Прил. 1). Документ сформирован системой TOU.Rent.]
