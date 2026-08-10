import { Outlet, createFileRoute, redirect } from "@tanstack/react-router"
import { m } from "#/paraglide/messages"

// Кабинет департамента цифрового развития (М15, T53): пользователи и роли
// (FR-1503, FR-1902), справочники расчета и календарь (FR-1901, FR-1701).
// Admin не участвует в тендерном процессе и не видит цены до вскрытия
// (INV-040) - это решает политика на сервере, здесь только гард маршрута.
export const Route = createFileRoute("/app/admin")({
  beforeLoad: ({ context }) => {
    if (!context.user.roles.includes("admin")) {
      throw redirect({ to: "/app" })
    }
  },
  component: AdminLayout,
})

function AdminLayout() {
  return (
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-6 px-6 py-8">
      <h1 className="font-heading text-2xl font-semibold">
        {m.cabinet_admin()}
      </h1>
      <Outlet />
    </div>
  )
}
