// Протокол допуска к участию в тендере (FR-503, п. 54–55). Данные - data.json,
// значения предформатированы сервером (ru - печатные формы контура 1, NFR-01).
// TODO-ENGINEER: сверить состав полей, нумерацию и формулировки с п. 55 Правил
// и утвержденной формой протокола (первоисточник-PDF агенту недоступен).
#set page(paper: "a4", margin: (x: 1.8cm, y: 1.6cm))
#set text(font: "Libertinus Serif", size: 10.5pt, lang: "ru")
#let d = json("data.json")

#align(center)[
  #text(weight: "bold", size: 13pt)[ПРОТОКОЛ №~#d.number] \
  заседания тендерной комиссии о допуске к участию в тендере \
  по предоставлению в имущественный наем (аренду) объектов \ НАО «Торайгыров университет»
]

#v(0.9em)
#grid(
  columns: (16em, 1fr),
  row-gutter: 0.5em,
  [Тендер:], [#d.tender_number - #d.tender_title],
  [Дата и время заседания:], [#d.held_at],
  [Назначено на:], [#d.scheduled_at],
  [Место проведения:], [#d.place],
  [Комиссия:], [#d.commission],
  [Кворум (п. 12):], [#d.quorum],
)

#v(0.9em)
#text(weight: "bold")[Состав комиссии на заседании]
#v(0.3em)
#table(
  columns: (auto, 1fr, auto, auto),
  inset: 5pt,
  align: (center, left, left, left),
  table.header([№], [ФИО], [Роль в комиссии], [Явка]),
  ..d
    .members
    .enumerate()
    .map(((i, m)) => ([#(i + 1)], [#m.name], [#m.role], [#m.attendance]))
    .flatten(),
)

#if d.recusals.len() > 0 [
  #v(0.9em)
  #text(weight: "bold")[Отводы по конфликту интересов (п. 15)]
  #v(0.3em)
  #set text(size: 9pt)
  #table(
    columns: (1fr, auto, 1fr, 1fr),
    inset: 5pt,
    align: (left, left, left, left),
    table.header([Член комиссии], [Отвод по], [Основание], [Замена]),
    ..d
      .recusals
      .map(r => ([#r.member], [#r.scope], [#r.reason], [#r.replacement]))
      .flatten(),
  )
  #set text(size: 10.5pt)
]

#v(0.9em)
#text(weight: "bold")[Вскрытие конвертов и решения по заявкам]
#v(0.3em)
#set text(size: 9pt)
#table(
  columns: (auto, 1fr, 1fr, auto, auto, 1fr),
  inset: 5pt,
  align: (center, left, left, right, center, left),
  table.header(
    [№],
    [Участник],
    [Лот],
    [Первоначальная цена, тг],
    [Документов],
    [Решение],
  ),
  ..d
    .applications
    .enumerate()
    .map(((i, a)) => (
      [#(i + 1)],
      [#a.applicant],
      [#a.lot],
      [#a.price],
      [#a.files],
      [#a.decision],
    ))
    .flatten(),
)
#set text(size: 10.5pt)

#if d.votes.len() > 0 [
  #v(0.9em)
  #text(weight: "bold")[Мнения членов комиссии по заявкам (п. 55)]
  #v(0.3em)
  #set text(size: 9pt)
  #table(
    columns: (1fr, 1fr, auto, 1fr),
    inset: 5pt,
    align: (left, left, center, left),
    table.header([Заявка], [Член комиссии], [Голос], [Особое мнение]),
    ..d
      .votes
      .map(v => ([#v.application], [#v.member], [#v.value], [#v.dissent]))
      .flatten(),
  )
  #set text(size: 10.5pt)
]

#v(0.6em)
- Заседание открыто при кворуме ⅔ голосующего состава с участием председателя или заместителя (п. 12).
- Заявки вскрыты на заседании комиссии; состав документов и первоначальные цены оглашены (п. 50).
- Решения приняты большинством голосов присутствующих; при равенстве голос председательствующего решающий (п. 13–14).
- Основания отклонения - из закрытого перечня п. 52 Правил.

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
)[Сформировано системой TOU.Rent #d.generated_at. Форма протокола уточняется юридической службой (п. 55).]
