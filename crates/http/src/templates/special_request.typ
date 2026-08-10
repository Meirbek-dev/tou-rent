// Заявка на предоставление имущества в особом порядке (Прил. 3, FR-1201,
// п. 87–88). Данные - data.json, предформатированы сервером (ru, NFR-01).
// TODO-ENGINEER: точные формулировки шапки, состав полей Прил. 3 и наименование
// категории агенту недоступны (Q-009) - сверить по Правилам.
#set page(paper: "a4", margin: (x: 1.8cm, y: 1.6cm))
#set text(font: "Libertinus Serif", size: 10.5pt, lang: "ru")
#let d = json("data.json")

#align(center)[
  #text(weight: "bold", size: 13pt)[ЗАЯВКА №~#d.number] \
  о предоставлении имущества в особом порядке (раздел 12 Правил)
]

#v(0.6em)
#grid(
  columns: (1fr, 1fr),
  [НАО «Торайгыров университет»],
  align(right)[#d.submitted_at],
)

#v(0.9em)
#text(weight: "bold")[Категория особого порядка]
#v(0.3em)
#d.category

#v(0.9em)
#text(weight: "bold")[Заявитель]
#v(0.3em)
#grid(
  columns: (16em, 1fr),
  row-gutter: 0.5em,
  [Наименование / ФИО:], [#d.applicant_name],
  [БИН / ИИН:], [#d.applicant_id_number],
  [Адрес:], [#d.applicant_address],
  [Телефон:], [#d.applicant_phone],
  [Электронная почта:], [#d.applicant_email],
)

#v(0.9em)
#text(weight: "bold")[Предмет заявки]
#v(0.3em)
#grid(
  columns: (16em, 1fr),
  row-gutter: 0.5em,
  [Объект имущества:], [#d.object],
  [Испрашиваемый срок, мес.:], [#d.requested_months],
  [Срок рассмотрения:], [#d.review_term],
  [Состояние заявки:], [#d.status],
)

#v(0.6em)
#text(weight: "bold")[Цель использования]
#v(0.3em)
#d.purpose

#v(0.9em)
#text(weight: "bold")[Приложенные документы]
#v(0.3em)
#if d.documents.len() == 0 [
  Документы не приложены.
] else [
  #table(
    columns: (2em, 1fr, 14em),
    inset: 5pt,
    align: (center, left, left),
    table.header([№], [Документ], [Позиция перечня категории]),
    ..d.documents.enumerate().map(((i, doc)) => (
      [#(i + 1)],
      [#doc.filename],
      [#doc.document_code],
    )).flatten()
  )
]

#v(1.6em)
#grid(
  columns: (1fr, 1fr),
  [Заявитель: #box(width: 1fr, repeat[.])],
  [Дата: #box(width: 1fr, repeat[.])],
)

#v(0.6em)
#text(size: 9pt)[
  Форма Прил. 3 Правил. Печатная форма подготовлена системой TOU.Rent.
]
