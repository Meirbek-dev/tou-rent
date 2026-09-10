import { m } from "#/paraglide/messages"
import { getLocale } from "#/paraglide/runtime"
import { Separator } from "./ui/separator"
import { documentKindLabel } from "../lib/application-documents"
import { formatDateTime, formatTenge } from "../lib/format"
import type { LotDto } from "@/lib/api"
import type { ApplicationDto } from "@/lib/participant"

export function CommissionApplicationMaterials({
  application,
  lot,
  opened,
}: {
  application: Pick<
    ApplicationDto,
    "id" | "submitted_at" | "qualification" | "files"
  >
  lot: LotDto | undefined
  opened: boolean
}) {
  return (
    <div className="flex min-w-0 flex-col gap-4">
      <Separator />
      {lot !== undefined && (
        <div className="flex flex-col gap-2">
          <h3 className="font-medium wrap-anywhere">
            {m.auction_lot({
              seq: lot.seq,
              purpose: getLocale() === "kk" ? lot.purpose_kk : lot.purpose,
            })}
          </h3>
          <dl className="grid gap-3 text-sm sm:grid-cols-3">
            <div>
              <dt className="text-muted-foreground">{m.lot_base_rate()}</dt>
              <dd>{formatTenge(lot.base_rate_monthly)}</dd>
            </div>
            <div>
              <dt className="text-muted-foreground">{m.lot_guarantee_fee()}</dt>
              <dd>{formatTenge(lot.guarantee_fee)}</dd>
            </div>
            <div>
              <dt className="text-muted-foreground">{m.lot_lease_months()}</dt>
              <dd>{m.lot_months({ months: lot.lease_months })}</dd>
            </div>
          </dl>
        </div>
      )}
      <dl className="flex flex-wrap gap-x-6 gap-y-2 text-sm">
        <div>
          <dt className="text-muted-foreground">
            {m.application_card_short()}
          </dt>
          <dd className="wrap-anywhere">{application.id}</dd>
        </div>
        <div>
          <dt className="text-muted-foreground">
            {m.application_submitted_at()}
          </dt>
          <dd suppressHydrationWarning>
            {formatDateTime(application.submitted_at) ?? "—"}
          </dd>
        </div>
      </dl>
      {opened ? (
        <>
          <div className="flex flex-col gap-1">
            <h3 className="font-medium">{m.apply_qualification_label()}</h3>
            <p className="text-sm wrap-anywhere whitespace-pre-wrap">
              {application.qualification?.trim() ||
                m.commission_qualification_empty()}
            </p>
          </div>
          <div className="flex flex-col gap-2">
            <h3 className="font-medium">{m.application_files_title()}</h3>
            {application.files.length === 0 ? (
              <p className="text-sm text-muted-foreground">
                {m.application_files_empty()}
              </p>
            ) : (
              <ul className="flex flex-col gap-2">
                {application.files.map((file) => (
                  <li key={file.id} className="min-w-0 text-sm wrap-anywhere">
                    <a
                      href={`/api/v1/applications/${application.id}/files/${file.id}`}
                      className="underline underline-offset-4"
                    >
                      {documentKindLabel(file.document_kind)} — {file.filename}
                    </a>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </>
      ) : (
        <p className="text-sm text-muted-foreground">
          {m.commission_materials_sealed()}
        </p>
      )}
      <Separator />
    </div>
  )
}
