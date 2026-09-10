import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { ConfirmAction } from "@/components/confirm-action"
import { Panel } from "@/components/panel"
import { Button } from "@/components/ui/button"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { api } from "@/lib/api"
import { problemMessage } from "@/lib/auth"
import { activeCommissionQuery, memberRoleLabel } from "@/lib/commission"
import type { MemberRole } from "@/lib/commission"
import type { UserDto } from "@/lib/admin"

const roles: MemberRole[] = ["chairman", "deputy", "member", "reserve"]

export function AdminCommissionPanel() {
  const client = useQueryClient()
  const commission = useQuery(activeCommissionQuery)
  const [userId, setUserId] = useState("")
  const [role, setRole] = useState<MemberRole>("member")
  const users = useQuery({
    queryKey: ["admin", "commission-candidates"],
    queryFn: async () => {
      const items: UserDto[] = []
      let after: string | undefined
      do {
        const { data, error } = await api.GET("/api/v1/admin/users", {
          params: { query: { limit: 100, ...(after ? { after } : {}) } },
        })
        if (error || !data) throw error ?? new Error("users unavailable")
        items.push(...data.items)
        after = data.next_after ?? undefined
      } while (after)
      return items.filter(
        (u) =>
          u.is_active &&
          u.roles.includes("commission") &&
          !u.roles.includes("secretary")
      )
    },
  })
  const refresh = () => client.invalidateQueries({ queryKey: ["commission"] })
  const save = useMutation({
    mutationFn: async (body: {
      user_id: string
      member_role: MemberRole | null
    }) => {
      if (!commission.data) return
      const { error } = await api.POST(
        "/api/v1/admin/commissions/{id}/members",
        {
          params: { path: { id: commission.data.id } },
          body,
        }
      )
      if (error) throw error
    },
    onSuccess: refresh,
  })
  const approve = useMutation({
    mutationFn: async () => {
      if (!commission.data) return
      const { error } = await api.POST("/api/v1/commissions/{id}/approve", {
        params: { path: { id: commission.data.id } },
      })
      if (error) throw error
    },
    onSuccess: refresh,
  })
  const busy = save.isPending || approve.isPending
  return (
    <Panel title={m.admin_commission_title()}>
      <p>{m.admin_commission_hint()}</p>
      {[commission.error, users.error, save.error, approve.error]
        .filter(Boolean)
        .map((error, i) => (
          <p key={i} role="alert" className="text-sm text-destructive">
            {problemMessage(error)}
          </p>
        ))}
      {commission.isPending && <p>{m.admin_commission_loading()}</p>}
      {commission.data === null && <p>{m.admin_commission_missing()}</p>}
      {commission.data && (
        <>
          <h3 className="font-medium">{commission.data.name}</h3>
          <p role="status">
            {commission.data.approved
              ? m.admin_commission_approved()
              : m.admin_commission_unapproved()}
          </p>
          {roles.map((group) => (
            <section key={group} className="flex flex-col gap-2">
              <h4 className="font-medium">{memberRoleLabel(group)}</h4>
              <ol className="list-decimal pl-6">
                {commission.data?.members
                  .filter((member) => member.member_role === group)
                  .map((member) => (
                    <li key={member.member_id} className="py-1">
                      <span>{member.full_name} </span>
                      <ConfirmAction
                        title={m.admin_commission_remove()}
                        description={m.admin_commission_remove_hint()}
                        confirmLabel={m.admin_commission_remove()}
                        disabled={busy}
                        onConfirm={() =>
                          save.mutate({
                            user_id: member.user_id,
                            member_role: null,
                          })
                        }
                        trigger={
                          <Button variant="outline" size="sm">
                            {m.admin_commission_remove()}
                          </Button>
                        }
                      />
                    </li>
                  ))}
              </ol>
            </section>
          ))}
          <form
            className="flex flex-col gap-3"
            onSubmit={(e) => {
              e.preventDefault()
              if (userId) save.mutate({ user_id: userId, member_role: role })
            }}
          >
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor="commission-user">
                  {m.admin_commission_user()}
                </FieldLabel>
                <NativeSelect
                  id="commission-user"
                  value={userId}
                  onChange={(e) => setUserId(e.target.value)}
                  required
                  disabled={busy}
                >
                  <NativeSelectOption value="">—</NativeSelectOption>
                  {users.data?.map((u) => (
                    <NativeSelectOption key={u.id} value={u.id}>
                      {u.full_name} ({u.email})
                    </NativeSelectOption>
                  ))}
                </NativeSelect>
              </Field>
              <Field>
                <FieldLabel htmlFor="commission-role">
                  {m.admin_commission_role()}
                </FieldLabel>
                <NativeSelect
                  id="commission-role"
                  value={role}
                  onChange={(e) => setRole(e.target.value as MemberRole)}
                  disabled={busy}
                >
                  {roles.map((r) => (
                    <NativeSelectOption key={r} value={r}>
                      {memberRoleLabel(r)}
                    </NativeSelectOption>
                  ))}
                </NativeSelect>
              </Field>
            </FieldGroup>
            <Button type="submit" disabled={busy || !userId}>
              {m.admin_commission_save()}
            </Button>
          </form>
          <Button
            disabled={
              busy ||
              commission.data.approved ||
              !!commission.data.composition_error
            }
            onClick={() => approve.mutate()}
          >
            {m.admin_commission_approve()}
          </Button>
        </>
      )}
    </Panel>
  )
}
