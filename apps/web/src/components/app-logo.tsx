import { m } from "#/paraglide/messages"
import blueLogoUrl from "../../res/logo-blue.webp?url"
import whiteLogoUrl from "../../res/logo-white.png?url"

/**
 * Знак портала.
 *
 * Кадрирование осталось, и вот почему: оба файла - квадрат 240×240, в котором
 * сам логотип занимает горизонтальную полосу посередине, а остальное - поля.
 * Без кадрирования знак в шапке высотой 40 px ужался бы до полутора десятков
 * пикселей. Прежние значения (`w-[135%] left-[61%]`) при этом срезали конец
 * слова «Rent» - здесь кадр симметричный и обрезает только поля.
 *
 * Отдельной текстовой подписи рядом нет намеренно: слово «Rent» впечатано
 * в сам файл, и вторая подпись читалась бы как «Rent ToU.Rent».
 *
 * TODO-ENGINEER: logo-white.webp - без альфа-канала, тёмно-синяя подложка
 * впечатана в файл и не совпадает с фоном тёмной темы. Нужен вариант знака
 * с прозрачностью; до тех пор скруглением сглажен стык.
 */
export function AppLogo() {
  return (
    <span className="relative block h-10 w-28 shrink-0 overflow-hidden rounded-md">
      <img
        src={blueLogoUrl}
        alt={m.app_name()}
        width={240}
        height={240}
        className="absolute top-1/2 left-1/2 w-[112%] max-w-none -translate-x-1/2 -translate-y-1/2 dark:hidden"
      />
      {/* Тему логотип меняет через display, а скрытая картинка выпадает
          и из дерева доступности: с `alt=""` и `aria-hidden` в темной теме
          у ссылки на главную не оставалось имени вовсе (гейт G17, link-name).
          Видимой всегда ровно одна картинка, поэтому подпись у обеих */}
      <img
        src={whiteLogoUrl}
        alt={m.app_name()}
        width={240}
        height={240}
        className="absolute top-1/2 left-1/2 hidden w-[112%] max-w-none -translate-x-1/2 -translate-y-1/2 dark:block"
      />
    </span>
  )
}
