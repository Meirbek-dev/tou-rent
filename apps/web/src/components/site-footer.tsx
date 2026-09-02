import { Link } from "@tanstack/react-router"

import { m } from "#/paraglide/messages"

const UNIVERSITY_URL = "https://tou.edu.kz"

/**
 * Подвал публичного портала: карта разделов, вход для участника, документы
 * и принадлежность портала. Ссылки и текст, без единой строки JS - подвал
 * обязан работать там же, где и остальной портал (NFR-04).
 *
 * Подвал же несет фирменную полосу университета: темно-синее поле с белым
 * локапом и оранжевой чертой по нижней кромке - тот же порядок, что на
 * обложке для соцсетей. Синий одинаков в обеих темах, поэтому цвета текста
 * здесь заданы от `brand-foreground`, а не от темы: `muted-foreground` на
 * этом фоне не дает и половины требуемого контраста (гейт G17).
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

/** Белое на #1D3D66 - 11:1, приглушенное до 80% - 7,7:1, обе выше AA. */
const linkClass =
  "text-sm text-brand-foreground/80 underline-offset-4 transition-colors hover:text-brand-foreground hover:underline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand-foreground"

export function SiteFooter() {
  return (
    <footer className="mt-16 bg-brand text-brand-foreground">
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
            {/* Локап и адрес - одна ссылка, а не две подряд в одно место:
                имя ссылки складывается из `alt` (название университета на
                языке интерфейса) и видимого адреса. Белый локап существует
                в единственном варианте, и это ровно тот фон, для которого
                он сделан */}
            <a
              href={UNIVERSITY_URL}
              rel="noreferrer"
              className="group flex w-fit flex-col items-start gap-3 underline-offset-4 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand-foreground"
            >
              <img
                src="/brand/tou-lockup-white.png"
                alt={m.footer_university()}
                width={484}
                height={160}
                className="h-11 w-auto"
              />
              <span className="text-sm underline group-hover:no-underline">
                tou.edu.kz
              </span>
            </a>
          </FooterColumn>
        </div>

        <div className="mt-10 flex flex-col gap-2 border-t border-brand-foreground/25 pt-6 sm:flex-row sm:items-start sm:justify-between sm:gap-8">
          <p className="max-w-[68ch] text-sm text-brand-foreground/80">
            {m.footer_official_notice()}
          </p>
          <p className="shrink-0 text-sm text-brand-foreground/80">
            {m.footer_copyright()}
          </p>
        </div>
      </div>

      {/* Оранжевая черта по нижней кромке - как на обложке og-cover */}
      <div aria-hidden="true" className="h-1 w-full bg-brand-accent" />
    </footer>
  )
}
