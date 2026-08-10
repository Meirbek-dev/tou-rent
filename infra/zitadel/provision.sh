#!/bin/sh
# Провижининг Zitadel под TOU.Rent (FR-1502, ADR-0003): проект, роли системы,
# OIDC-клиент api и демо-сотрудник. Идемпотентен: повторный запуск с готовым
# /out/oidc.env ничего не меняет.
#
# Запускается сервисом zitadel-init дев-стенда (`vp run zitadel:up`).
# В проде те же шаги выполняет администратор Zitadel один раз; реквизиты
# клиента приходят в api переменными окружения (NFR-09).
set -eu

OUT_FILE=/out/oidc.env
API="${ZITADEL_INTERNAL_URL:?ZITADEL_INTERNAL_URL}"

if [ -f "$OUT_FILE" ]; then
  echo "provision: $OUT_FILE уже есть - стенд настроен, выхожу"
  exit 0
fi

apk add --no-cache curl jq >/dev/null

echo "provision: жду Zitadel на $API"
i=0
until curl -fsS "$API/debug/healthz" >/dev/null 2>&1; do
  i=$((i + 1))
  if [ "$i" -gt 120 ]; then
    echo "provision: Zitadel не поднялся за 240 с" >&2
    exit 1
  fi
  sleep 2
done

# PAT машинного пользователя создается на первом старте (FIRSTINSTANCE_PATPATH)
i=0
until [ -s /pat/pat.txt ]; do
  i=$((i + 1))
  if [ "$i" -gt 60 ]; then
    echo "provision: PAT не появился в /pat/pat.txt" >&2
    exit 1
  fi
  sleep 2
done
PAT="$(tr -d '\r\n' </pat/pat.txt)"

auth() {
  # $1 - метод, $2 - путь, $3 - тело (может отсутствовать)
  if [ "$#" -ge 3 ]; then
    curl -fsS -X "$1" "$API$2" \
      -H "Authorization: Bearer $PAT" \
      -H "Content-Type: application/json" \
      -d "$3"
  else
    curl -fsS -X "$1" "$API$2" -H "Authorization: Bearer $PAT"
  fi
}

echo "provision: проект TOU.Rent"
# projectRoleAssertion - роли системы попадают в id_token (их читает api)
PROJECT_ID="$(
  auth POST /management/v1/projects \
    '{"name":"TOU.Rent","projectRoleAssertion":true,"projectRoleCheck":false,"hasProjectCheck":false}' |
    jq -r '.id'
)"
[ -n "$PROJECT_ID" ] && [ "$PROJECT_ID" != "null" ] || {
  echo "provision: проект не создан" >&2
  exit 1
}

echo "provision: роли системы (ТЗ § 3)"
# guest - аноним, роли в провайдере не имеет; participant регистрируется сам
auth POST "/management/v1/projects/$PROJECT_ID/roles/_bulk" '{"roles":[
  {"key":"organizer","displayName":"Организатор тендера","group":"tou-rent"},
  {"key":"secretary","displayName":"Секретарь комиссии","group":"tou-rent"},
  {"key":"commission","displayName":"Член комиссии","group":"tou-rent"},
  {"key":"board","displayName":"Член Правления","group":"tou-rent"},
  {"key":"finance","displayName":"Финансы","group":"tou-rent"},
  {"key":"admin","displayName":"Администратор","group":"tou-rent"},
  {"key":"participant","displayName":"Участник","group":"tou-rent"}
]}' >/dev/null

echo "provision: OIDC-клиент api"
APP="$(
  auth POST "/management/v1/projects/$PROJECT_ID/apps/oidc" "$(
    jq -n \
      --arg redirect "${OIDC_REDIRECT_URL:?OIDC_REDIRECT_URL}" \
      --arg redirect2 "${OIDC_EXTRA_REDIRECT_URL:-}" \
      --arg logout "${OIDC_POST_LOGOUT_URL:?OIDC_POST_LOGOUT_URL}" \
      '{
        name: "TOU.Rent api",
        redirectUris: ([$redirect, $redirect2] | map(select(. != "")) ),
        postLogoutRedirectUris: [$logout],
        responseTypes: ["OIDC_RESPONSE_TYPE_CODE"],
        grantTypes: ["OIDC_GRANT_TYPE_AUTHORIZATION_CODE"],
        appType: "OIDC_APP_TYPE_WEB",
        authMethodType: "OIDC_AUTH_METHOD_TYPE_BASIC",
        accessTokenType: "OIDC_TOKEN_TYPE_BEARER",
        idTokenRoleAssertion: true,
        idTokenUserinfoAssertion: true,
        devMode: true
      }'
  )"
)"
CLIENT_ID="$(printf '%s' "$APP" | jq -r '.clientId')"
CLIENT_SECRET="$(printf '%s' "$APP" | jq -r '.clientSecret')"
[ -n "$CLIENT_ID" ] && [ "$CLIENT_ID" != "null" ] || {
  echo "provision: клиент не создан" >&2
  exit 1
}

# Демо-сотрудник: вход через провайдера видно без настройки AD.
# Пароль - переменная стенда, в репозиторий не попадает (NFR-09).
echo "provision: демо-сотрудник secretary@tou.local"
USER_ID="$(
  auth POST /v2/users/human "$(
    jq -n --arg password "${DEMO_USER_PASSWORD:?DEMO_USER_PASSWORD}" '{
      username: "secretary@tou.local",
      profile: { givenName: "Демо", familyName: "Секретарь", preferredLanguage: "ru" },
      email: { email: "secretary@tou.demo", isVerified: true },
      password: { password: $password, changeRequired: false }
    }'
  )" | jq -r '.userId'
)"

if [ -n "$USER_ID" ] && [ "$USER_ID" != "null" ]; then
  auth POST "/management/v1/users/$USER_ID/grants" "$(
    jq -n --arg project "$PROJECT_ID" \
      '{projectId: $project, roleKeys: ["secretary", "commission"]}'
  )" >/dev/null
fi

# Zitadel кладет в аудиторию id_token не только client_id, но и id проекта -
# он перечисляется в OIDC_TRUSTED_AUDIENCES явным списком (api не доверяет
# незнакомой аудитории)
cat >"$OUT_FILE" <<EOF
# Сгенерировано infra/zitadel/provision.sh - правки перетрутся при пересоздании
# стенда. Файл читает сервис api дев-стенда (env_file), в git не попадает.
OIDC_ISSUER_URL=${ZITADEL_ISSUER:?ZITADEL_ISSUER}
OIDC_CLIENT_ID=$CLIENT_ID
OIDC_CLIENT_SECRET=$CLIENT_SECRET
OIDC_REDIRECT_URL=$OIDC_REDIRECT_URL
OIDC_POST_LOGOUT_URL=$OIDC_POST_LOGOUT_URL
OIDC_SCOPES=openid profile email
OIDC_TRUSTED_AUDIENCES=$PROJECT_ID
OIDC_LABEL=учетную запись университета
EOF

echo "provision: готово - $OUT_FILE (проект $PROJECT_ID, клиент $CLIENT_ID)"
echo "provision: перезапусти api, чтобы он подхватил реквизиты (vp run api:restart)"
