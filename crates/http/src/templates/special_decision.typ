// Протокол решения Правления по заявке особого порядка (FR-1202, п. 90, 97).
// Данные - data.json, предформатированы сервером (ru, NFR-01).
// TODO-ENGINEER: форма протокола решения Правления в Правилах не приведена -
// состав полей выведен из п. 89–90 и подлежит сверке (Q-009).
#set page(paper: "a4", margin: (x: 1.8cm, y: 1.6cm))
#set text(font: "Libertinus Serif", size: 10.5pt, lang: "ru")
#let d = json("data.json")

#align(center)[
  #text(weight: "bold", size: 13pt)[РЕШЕНИЕ ПРАВЛЕНИЯ №~#d.number] \
  по заявке о предоставлении имущества в особом порядке (раздел 12 Правил)
]

#v(0.6em)
#grid(
  columns: (1fr, 1fr),
  [НАО «Торайгыров университет»],
  align(right)[#d.decided_at],
)

#v(0.9em)
#grid(
  columns: (16em, 1fr),
  row-gutter: 0.5em,
  [Заявитель:], [#d.applicant],
  [Категория:], [#d.category],
  [Объект имущества:], [#d.object],
  [Заявка подана:], [#d.submitted_at],
)

#v(0.9em)
#text(weight: "bold")[Существо заявки]
#v(0.3em)
#d.purpose

#v(0.9em)
#text(weight: "bold")[Заключение уполномоченного подразделения (п. 89)]
#v(0.3em)
#d.conclusion
#v(0.3em)
#emph[Вывод подразделения: #d.recommendation]

#v(0.9em)
#text(weight: "bold")[Решение Правления (п. 90)]
#v(0.3em)
#text(weight: "bold")[#d.decision]

#v(0.6em)
#text(weight: "bold")[Обоснование]
#v(0.3em)
#d.rationale

#v(1.6em)
#grid(
  columns: (1fr, 1fr),
  [Председательствующий: #d.decided_by],
  [Подпись: #box(width: 1fr, repeat[.])],
)

#v(0.6em)
#text(size: 9pt)[
  Решение и его обоснование публикуются в порядке п. 97 Правил.
  Печатная форма подготовлена системой TOU.Rent.
]
