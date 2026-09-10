import { m } from "#/paraglide/messages"

export function documentKindLabel(kind: string): string {
  switch (kind) {
    case "application_form":
      return m.application_document_application_form()
    case "registration_certificate":
      return m.application_document_registration_certificate()
    case "tax_clearance":
      return m.application_document_tax_clearance()
    case "guarantee_payment":
      return m.application_document_guarantee_payment()
    case "qualification_documents":
      return m.application_document_qualification_documents()
    case "price_proposal_form":
      return m.application_document_price_proposal_form()
    case "qualification_form":
      return m.application_document_qualification_form()
    default:
      return m.application_document_legacy()
  }
}
