import { useEffect, useState } from "react"
import { useNavigate } from "@tanstack/react-router"

import { m } from "#/paraglide/messages"
import { Button } from "@/components/ui/button"
import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import { Kbd, KbdGroup } from "@/components/ui/kbd"
import { cabinetLabel, userCabinets } from "@/lib/auth"
import { REPORTS_NAV, WORKSPACE_NAV, canSeeReports, roleNav } from "@/lib/nav"
import { SearchIcon } from "lucide-react"

import type { NavEntry } from "@/lib/nav"

/**
 * Быстрый переход по разделам (Ctrl/⌘ + K).
 *
 * У человека с несколькими ролями боковая навигация - это четыре-пять групп;
 * дойти мышью до «Депозитной книги» из кабинета комиссии стоит трех движений.
 * Палитра сокращает это до имени раздела. Поиск здесь только по разделам:
 * искать по тендерам и договорам значило бы ходить в сеть на каждое нажатие,
 * а такой маршрут поиска в контракте один и он постраничный.
 */
export function CommandPalette({ roles }: { roles: readonly string[] }) {
  const [open, setOpen] = useState(false)
  const navigate = useNavigate()

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key.toLowerCase() !== "k") return
      if (!event.metaKey && !event.ctrlKey) return
      event.preventDefault()
      setOpen((previous) => !previous)
    }
    window.addEventListener("keydown", onKeyDown)
    return () => window.removeEventListener("keydown", onKeyDown)
  }, [])

  const workspace: NavEntry[] = canSeeReports(roles)
    ? [...WORKSPACE_NAV, REPORTS_NAV]
    : WORKSPACE_NAV

  const groups: { key: string; label: string; entries: NavEntry[] }[] = [
    { key: "workspace", label: m.nav_workspace(), entries: workspace },
    ...userCabinets(roles).map(({ role }) => ({
      key: role,
      label: cabinetLabel(role),
      entries: roleNav(role),
    })),
  ]

  const go = (to: string) => {
    setOpen(false)
    void navigate({ to })
  }

  return (
    <>
      <Button
        variant="outline"
        size="sm"
        aria-label={m.command_open()}
        className="hidden gap-2 text-muted-foreground md:inline-flex"
        onClick={() => setOpen(true)}
      >
        <SearchIcon aria-hidden="true" />
        <KbdGroup aria-hidden="true">
          <Kbd>Ctrl</Kbd>
          <Kbd>K</Kbd>
        </KbdGroup>
      </Button>

      <CommandDialog
        open={open}
        onOpenChange={setOpen}
        title={m.command_open()}
        description={m.command_placeholder()}
      >
        {/* Корень cmdk обязателен: CommandDialog дает только окно, а список,
            поле и пункты читают состояние поиска из контекста Command -
            без него палитра падает на первом же открытии */}
        <Command>
          <CommandInput placeholder={m.command_placeholder()} />
          <CommandList>
            <CommandEmpty>{m.command_empty()}</CommandEmpty>
            {groups.map((group) =>
              group.entries.length === 0 ? null : (
                <CommandGroup key={group.key} heading={group.label}>
                  {group.entries.map((entry) => {
                    const Icon = entry.icon
                    const label = entry.label()
                    return (
                      <CommandItem
                        key={entry.to}
                        // Значение ищется по подписи раздела вместе с именем
                        // группы: «книга» находит депозитную книгу финансов,
                        // «финанс» - все разделы этого кабинета
                        value={`${group.label} ${label}`}
                        onSelect={() => go(entry.to)}
                      >
                        <Icon aria-hidden="true" />
                        <span>{label}</span>
                      </CommandItem>
                    )
                  })}
                </CommandGroup>
              )
            )}
          </CommandList>
        </Command>
      </CommandDialog>
    </>
  )
}
