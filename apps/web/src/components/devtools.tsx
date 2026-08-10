import { TanStackDevtools } from "@tanstack/react-devtools"
import { TanStackRouterDevtoolsPanel } from "@tanstack/react-router-devtools"

import TanStackQueryDevtools from "@/integrations/tanstack-query/devtools"

/**
 * Панель инструментов разработчика.
 *
 * Все три пакета `@tanstack/*-devtools` импортируются только отсюда, а сам
 * модуль подключается динамически и лишь под `import.meta.env.DEV`
 * (см. `routes/__root.tsx`). В прод-сборке ветка отсекается статически,
 * динамический импорт остается недостижимым, и пакеты в бандл не попадают -
 * поэтому они и переехали в devDependencies.
 */
export default function Devtools() {
  return (
    <TanStackDevtools
      config={{
        position: "bottom-right",
      }}
      plugins={[
        {
          name: "Tanstack Router",
          render: <TanStackRouterDevtoolsPanel />,
        },
        TanStackQueryDevtools,
      ]}
    />
  )
}
