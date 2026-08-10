import { m } from "#/paraglide/messages"
import blueLogoUrl from "../../res/logo-blue.webp?url"
import whiteLogoUrl from "../../res/logo-white.webp?url"

export function AppLogo() {
  return (
    <span className="relative block h-10 w-24 shrink-0 overflow-hidden">
      <img
        src={blueLogoUrl}
        alt={m.app_name()}
        className="absolute top-1/2 left-[61%] w-[135%] max-w-none -translate-x-1/2 -translate-y-1/2 dark:hidden"
      />
      {/* Тему логотип меняет через display, а скрытая картинка выпадает
          и из дерева доступности: с `alt=""` и `aria-hidden` в темной теме
          у ссылки на главную не оставалось имени вовсе (гейт G17, link-name).
          Видимой всегда ровно одна картинка, поэтому подпись у обеих */}
      <img
        src={whiteLogoUrl}
        alt={m.app_name()}
        className="absolute top-1/2 left-1/2 hidden w-[132%] max-w-none -translate-x-1/2 -translate-y-1/2 dark:block"
      />
    </span>
  )
}
