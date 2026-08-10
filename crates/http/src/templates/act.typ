// Акт приема-передачи / возврата объекта (Прил. 7–8, FR-904, п. 122, 128–129).
// Один шаблон на оба вида: различаются заголовок, направление передачи
// и следствие. Данные - data.json, предформатированы сервером (ru, NFR-01).
// TODO-ENGINEER: сверить состав полей с Прил. 7 и Прил. 8 (Q-005).
#set page(paper: "a4", margin: (x: 1.8cm, y: 1.6cm))
#set text(font: "Libertinus Serif", size: 10.5pt, lang: "ru")
#let d = json("data.json")

#align(center)[
  #text(weight: "bold", size: 13pt)[#upper(d.title) №~#d.number] \
  к договору имущественного найма (аренды) #d.contract_number
]

#v(0.6em)
#grid(
  columns: (1fr, 1fr),
  [г. Павлодар],
  align(right)[#d.act_date],
)

#v(0.9em)
НАО «Торайгыров университет», именуемое «Наймодатель», и #d.tenant_name,
именуемый «Наниматель», составили настоящий акт о том, что #d.transfer_text

#v(0.9em)
#text(weight: "bold")[Объект]
#v(0.3em)
#grid(
  columns: (16em, 1fr),
  row-gutter: 0.5em,
  [Наименование:], [#d.object_name],
  [Адрес:], [#d.object_address],
  [Площадь:], [#d.object_area м²],
  [Целевое назначение:], [#d.purpose],
)

#v(0.9em)
#text(weight: "bold")[Состояние объекта]
#v(0.3em)
#if d.note == "" [
  Претензий к состоянию объекта у сторон не имеется.
] else [
  #d.note
]

#v(0.9em)
#text(weight: "bold")[Следствие]
#v(0.3em)
#d.effect_text

#v(1.6em)
#grid(
  columns: (1fr, 1fr),
  [Наймодатель: #box(width: 1fr, repeat[.])],
  [Наниматель: #box(width: 1fr, repeat[.])],
)

#v(0.6em)
#text(size: 9pt)[
  Форма #d.appendix Правил. Печатная форма подготовлена системой TOU.Rent #d.generated_at
]
