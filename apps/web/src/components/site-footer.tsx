import { Link } from "@tanstack/react-router"

import { m } from "#/paraglide/messages"

const UNIVERSITY_URL = "https://tou.edu.kz"

/**
 * Подвал публичного портала: карта разделов, вход для участника, документы
 * и принадлежность портала. Ссылки и текст, без единой строки JS - подвал
 * обязан работать там же, где и остальной портал (NFR-04).
 *
 * TODO-ENGINEER: прямой ссылки на Правила университета в кодовой базе нет
 * (адрес PDF нигде не назван), поэтому «Правила проведения тендера» ведут
 * на /how-to - изложение процедуры по Правилам. Появится адрес документа -
 * менять здесь одну строку.
 */
function FooterColumn({
  id,
  title,
  children,
}: {
  id: string
  title: string
  children: React.ReactNode
}) {
  return (
    <div className="flex flex-col gap-3">
      <h2 id={id} className="text-sm font-semibold">
        {title}
      </h2>
      {children}
    </div>
  )
}

const linkClass =
  "text-sm text-muted-foreground underline-offset-4 transition-colors hover:text-foreground hover:underline"

export function SiteFooter() {
  return (
    <footer className="mt-16 border-t bg-muted">
      <div className="mx-auto w-full max-w-6xl px-4 py-12 sm:px-6">
        <div className="grid grid-cols-1 gap-8 sm:grid-cols-2 lg:grid-cols-4">
          <FooterColumn id="footer-sections" title={m.footer_sections_title()}>
            <nav aria-labelledby="footer-sections">
              <ul className="flex flex-col gap-2">
                <li>
                  <Link to="/tenders" className={linkClass}>
                    {m.nav_tenders()}
                  </Link>
                </li>
                <li>
                  <Link to="/objects" className={linkClass}>
                    {m.nav_objects()}
                  </Link>
                </li>
                <li>
                  <Link to="/land-plots" className={linkClass}>
                    {m.nav_land_plots()}
                  </Link>
                </li>
                <li>
                  <Link to="/special-orders" className={linkClass}>
                    {m.nav_special_orders()}
                  </Link>
                </li>
              </ul>
            </nav>
          </FooterColumn>

          <FooterColumn
            id="footer-participants"
            title={m.footer_participants_title()}
          >
            <nav aria-labelledby="footer-participants">
              <ul className="flex flex-col gap-2">
                <li>
                  <Link to="/how-to" className={linkClass}>
                    {m.nav_how_to()}
                  </Link>
                </li>
                <li>
                  <Link to="/auth/register" className={linkClass}>
                    {m.register()}
                  </Link>
                </li>
                <li>
                  <Link to="/auth/login" className={linkClass}>
                    {m.sign_in()}
                  </Link>
                </li>
              </ul>
            </nav>
          </FooterColumn>

          <FooterColumn
            id="footer-documents"
            title={m.footer_documents_title()}
          >
            <nav aria-labelledby="footer-documents">
              <ul className="flex flex-col gap-2">
                <li>
                  <Link to="/special-orders" className={linkClass}>
                    {m.public_records_title()}
                  </Link>
                </li>
                <li>
                  <Link to="/how-to" className={linkClass}>
                    {m.footer_rules_link()}
                  </Link>
                </li>
              </ul>
            </nav>
          </FooterColumn>

          <FooterColumn id="footer-contact" title={m.footer_contact_title()}>
            <p className="text-sm text-muted-foreground">
              {m.footer_university()}
            </p>
            <a
              href={UNIVERSITY_URL}
              rel="noreferrer"
              className="text-sm text-primary underline-offset-4 hover:underline"
            >
              tou.edu.kz
            </a>
          </FooterColumn>
        </div>

        <div className="mt-10 flex flex-col gap-2 border-t pt-6 sm:flex-row sm:items-start sm:justify-between sm:gap-8">
          <p className="max-w-[68ch] text-sm text-muted-foreground">
            {m.footer_official_notice()}
          </p>
          <p className="shrink-0 text-sm text-muted-foreground">
            {m.footer_copyright()}
          </p>
        </div>
      </div>
    </footer>
  )
}
