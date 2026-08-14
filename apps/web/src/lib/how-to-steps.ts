import { m } from "#/paraglide/messages"

/**
 * Порядок участия в тендере (п. 5–6) - один список на страницу «как
 * участвовать» и на полосу главной. Подписи берутся при вызове, а не при
 * загрузке модуля: локаль на сервере своя у каждого запроса.
 */
export type HowToStep = { title: string; text: string }

export function howToSteps(): HowToStep[] {
  return [
    {
      title: m.howto_step_register_title(),
      text: m.howto_step_register_text(),
    },
    {
      title: m.howto_step_prepare_title(),
      text: m.howto_step_prepare_text(),
    },
    {
      title: m.howto_step_submit_title(),
      text: m.howto_step_submit_text(),
    },
    {
      title: m.howto_step_trade_title(),
      text: m.howto_step_trade_text(),
    },
  ]
}
