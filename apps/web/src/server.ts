import handler from "@tanstack/react-start/server-entry"
import { paraglideMiddleware } from "./paraglide/server"

// SSR-детекция локали из URL (/kk, /en) - paraglide оборачивает обработку запроса
// (https://github.com/TanStack/router/tree/main/examples/react/i18n-paraglide)
export default {
  fetch(request: Request): Promise<Response> {
    return paraglideMiddleware(request, () => handler.fetch(request))
  },
}
