// Протокол об итогах тендера (FR-701, п. 73–74). Данные - data.json,
// значения предформатированы сервером (ru - печатные формы контура 1, NFR-01).
// TODO-ENGINEER: сверить состав полей, нумерацию и формулировки обязательств
// сторон с п. 73–74 Правил и утвержденной формой (первоисточник-PDF недоступен).
#set page(paper: "a4", margin: (x: 1.8cm, y: 1.6cm))
#set text(font: "Libertinus Serif", size: 10.5pt, lang: "ru")
#let d = json("data.json")

#align(center)[
  #text(weight: "bold", size: 13pt)[ПРОТОКОЛ №~#d.number] \
  об итогах тендера по предоставлению в имущественный наем (аренду) \
  объектов НАО «Торайгыров университет»
]

#v(0.9em)
#grid(
  columns: (16em, 1fr),
  row-gutter: 0.5em,
  [Тендер:], [#d.tender_number - #d.tender_title],
  [Дата и время заседания:], [#d.held_at],
  [Место проведения:], [#d.place],
  [Комиссия:], [#d.commission],
  [Срок оформления итогов:], [#d.deadline],
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
#text(weight: "bold")[Объекты тендера]
#v(0.3em)
#set text(size: 9pt)
#table(
  columns: (auto, 1fr, 1fr, auto, auto, auto),
  inset: 5pt,
  align: (center, left, left, right, right, right),
  table.header(
    [Лот],
    [Объект, адрес],
    [Целевое назначение],
    [Площадь, м²],
    [Срок найма, мес.],
    [Ставка, тг/мес.],
  ),
  ..d
    .lots
    .map(l => (
      [#l.seq],
      [#l.object],
      [#l.purpose],
      [#l.area],
      [#l.lease_months],
      [#l.base_rate],
    ))
    .flatten(),
)
#set text(size: 10.5pt)

#v(0.9em)
#text(weight: "bold")[Заявки и первоначальные ценовые предложения]
#v(0.3em)
#set text(size: 9pt)
#table(
  columns: (auto, 1fr, auto, auto, 1fr),
  inset: 5pt,
  align: (center, left, center, right, left),
  table.header([№], [Участник], [Лот], [Первоначальная цена, тг], [Решение комиссии]),
  ..d
    .applications
    .enumerate()
    .map(((i, a)) => (
      [#(i + 1)],
      [#a.applicant],
      [#a.lot],
      [#a.price],
      [#a.decision],
    ))
    .flatten(),
)
#set text(size: 10.5pt)

#v(0.9em)
#text(weight: "bold")[Итоги торгов по лотам (п. 69, 74)]
#v(0.3em)
#set text(size: 9pt)
#table(
  columns: (auto, auto, auto, 1fr, auto, 1fr, auto),
  inset: 5pt,
  align: (center, right, right, left, right, left, right),
  table.header(
    [Лот],
    [Стартовая ставка, тг],
    [Шаг, тг],
    [Победитель],
    [Ставка, тг],
    [Второе место],
    [Ставка, тг],
  ),
  ..d
    .results
    .map(r => (
      [#r.seq],
      [#r.starting_bid],
      [#r.step],
      [#r.winner],
      [#r.winner_amount],
      [#r.runner_up],
      [#r.runner_up_amount],
    ))
    .flatten(),
)
#set text(size: 10.5pt)

#v(0.9em)
#text(weight: "bold")[Обязательства сторон]
#v(0.3em)
#for line in d.obligations [
  - #line
]

#v(1.4em)
#grid(
  columns: (1fr, 1fr),
  row-gutter: 2.2em,
  ..d.members.map(m => [#m.name #h(1em) \_\_\_\_\_\_\_\_\_\_\_\_\_\_]),
  [Секретарь комиссии #h(1em) \_\_\_\_\_\_\_\_\_\_\_\_\_\_],
)

#v(1.2em)
#text(
  fill: luma(80),
  size: 9pt,
)[Сформировано системой TOU.Rent #d.generated_at. Форма протокола и формулировки обязательств уточняются юридической службой (п. 73–74, TODO-ENGINEER).]
