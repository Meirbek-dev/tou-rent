#!/usr/bin/env bash
# Деплой прод-стенда TOU.Rent (NFR-10): пулл образов из GitLab Registry,
# миграции, перезапуск, проверка здоровья. Запускается из GitLab CI по SSH
# на хосте rent.tou.edu.kz либо руками из каталога infra/compose.
#
# Окно заморозки: пока по любому лоту идет живая комната торгов
# (core.auctions.status = 'running' и время не истекло), деплой отклоняется -
# обрыв WS-комнаты посреди процедуры недопустим (п. 63–68). Обход -
# DEPLOY_FORCE=1, сознательное решение дежурного.
#
# Переменные (файл .env рядом, см. .env.prod.example). Перечень обязательных
# задан в самом compose подстановками `${VAR:?}` - без любой из них деплой
# падает на первом же вызове compose, до пулла и миграций:
#   TOU_DOMAIN, ACME_EMAIL, POSTGRES_PASSWORD, REDIS_PASSWORD,
#   S3_ACCESS_KEY, S3_SECRET_KEY (root-ключи RustFS - только провижининг),
#   S3_APP_ACCESS_KEY, S3_APP_SECRET_KEY (учетная запись приложения),
#   PRICE_ENCRYPTION_KEY (INV-040: без него цены не читаются и не пишутся).
# Необязательные: REGISTRY и TAG (у обеих есть подстановка по умолчанию,
# см. предупреждение про TAG ниже), реквизиты OIDC (пусто - вход локальный).
#
# Поведение самого скрипта настраивается окружением, а не .env:
#   DEPLOY_FORCE=1   - обойти окно заморозки (см. выше)
#   HEALTH_TIMEOUT   - сколько секунд ждать здоровья каждого контейнера (180)
#   COMPOSE          - чем звать compose (`podman compose`)
#   ENV_FILE         - путь к файлу переменных (.env)
#   STATE_FILE       - где хранится последний успешный тег (.deployed-tag)
set -euo pipefail

cd "$(dirname "$0")"

COMPOSE="${COMPOSE:-podman compose}"
COMPOSE_FILE="docker-compose.prod.yml"
ENV_FILE="${ENV_FILE:-.env}"
HEALTH_TIMEOUT="${HEALTH_TIMEOUT:-180}"
# Тег, который в последний раз прошел проверку здоровья. Пишется только после
# успеха, поэтому в файле всегда стоит версия, про которую известно, что она
# поднималась. Отсюда же берется цель отката
STATE_FILE="${STATE_FILE:-.deployed-tag}"

log() { printf '\033[1;94m==>\033[0m %s\n' "$*"; }
fail() { printf '\033[1;91mdeploy: %s\033[0m\n' "$*" >&2; exit 1; }

[ -f "$ENV_FILE" ] || fail "нет файла $ENV_FILE (скопируйте .env.prod.example и заполните)"
# shellcheck disable=SC1090
set -a && . "./$ENV_FILE" && set +a

compose() { $COMPOSE -f "$COMPOSE_FILE" "$@"; }

# --- Окно заморозки ---------------------------------------------------------
freeze_check() {
  if [ "${DEPLOY_FORCE:-0}" = "1" ]; then
    log "DEPLOY_FORCE=1 - проверка окна заморозки пропущена"
    return 0
  fi

  # Стенд может быть еще не поднят (первый деплой) - тогда проверять нечего
  if ! compose ps --status running --services 2>/dev/null | grep -qx postgres; then
    log "postgres не запущен - первый деплой, окно заморозки не проверяется"
    return 0
  fi

  # Заморозку задает живая комната, а не статус тендера: тендер может висеть
  # в trading с уже завершенными торгами (в т.ч. в демо-данных Прил. Б)
  local active
  active=$(compose exec -T postgres psql -qtAX -U tou_rent -d tou_rent -c "
    SELECT count(*) FROM core.auctions
    WHERE status = 'running' AND (ends_at IS NULL OR ends_at > now())") || \
    fail "не удалось опросить БД об активных торгах"

  active=$(printf '%s' "$active" | tr -d '[:space:]')
  if [ "${active:-0}" != "0" ]; then
    fail "идут торги (активных процедур: $active) - деплой заблокирован (NFR-10). Повторите после завершения или запустите с DEPLOY_FORCE=1"
  fi
  log "окно заморозки свободно: активных торгов нет"
}

# --- Здоровье ---------------------------------------------------------------
# Проверяются оба контейнера, которые обслуживают трафик. Один api недостаточен:
# Caddy поднимается только после того, как здоров и `web`, - при мертвой пробе
# web стенд стоит целиком, а деплой рапортует об успехе, потому что api жив.
wait_healthy() {
  local waited=0
  until compose exec -T api curl -fsS http://127.0.0.1:8080/api/v1/healthz >/dev/null 2>&1; do
    waited=$((waited + 5))
    if [ "$waited" -ge "$HEALTH_TIMEOUT" ]; then
      printf '\033[1;91mdeploy: api не отвечает на /healthz за %s с\033[0m\n' "$HEALTH_TIMEOUT" >&2
      return 1
    fi
    sleep 5
  done
  log "api здоров (/api/v1/healthz)"

  waited=0
  until compose exec -T web bun -e \
    "process.exit((await fetch('http://127.0.0.1:3000/healthz')).ok ? 0 : 1)" >/dev/null 2>&1; do
    waited=$((waited + 5))
    if [ "$waited" -ge "$HEALTH_TIMEOUT" ]; then
      printf '\033[1;91mdeploy: web не отвечает на /healthz за %s с\033[0m\n' "$HEALTH_TIMEOUT" >&2
      return 1
    fi
    sleep 5
  done
  log "web здоров (/healthz)"
}

# --- Откат ------------------------------------------------------------------
# Возврат на последний тег, про который известно, что он поднимался. Схему БД
# откат не трогает: миграции уже накачены, и отмены у них нет. Это осознанная
# граница - код возвращается автоматически, несовместимая миграция разбирается
# руками (см. README, § «Прод»).
rollback() {
  local prev="$1" current="${TAG:-latest}"

  [ -n "$prev" ] || fail "новая версия ($current) не поднялась, а откатываться некуда: $STATE_FILE пуст (первый деплой). Стенд остался на $current"
  [ "$prev" != "$current" ] || fail "новая версия ($current) не поднялась; предыдущий успешный тег тот же самый - откат ничего не изменит. Стенд остался на $current"

  log "откат на последний успешный тег: $prev"
  # Подстановка тега - в подоболочке: `compose` здесь функция, и присваивание
  # перед ее вызовом в разных оболочках живет по-разному
  (export TAG="$prev" && compose pull) >/dev/null 2>&1 || true
  (export TAG="$prev" && compose up -d --remove-orphans) \
    || fail "откат на $prev не выполнен - стенд требует ручного вмешательства"

  wait_healthy || fail "не поднялись ни $current, ни откат на $prev - стенд требует ручного вмешательства"

  fail "версия $current не прошла проверку здоровья, стенд возвращен на $prev. Схема БД осталась мигрированной вперед: если миграция несовместима со старым кодом, нужен разбор вручную"
}

# T70 снял `latest` с инфраструктурных образов ровно затем, чтобы стенд
# не переезжал на новую версию сам по себе; у своих образов подстановка
# по умолчанию делает то же самое молча. Отказывать поздно - деплой могут
# запускать и вручную, - но незамеченным это оставаться не должно
if [ -z "${TAG:-}" ]; then
  printf '\033[1;93mdeploy: TAG не задан - будет развернут «latest». Укажите тег сборки (T70)\033[0m\n' >&2
fi

log "тег образов: ${TAG:-latest}, реестр: ${REGISTRY:-registry.local/tou-rent}"
freeze_check

PREV_TAG=""
if [ -f "$STATE_FILE" ]; then
  PREV_TAG=$(tr -d '[:space:]' < "$STATE_FILE")
  log "последний успешный тег: ${PREV_TAG:-неизвестен}"
else
  log "$STATE_FILE отсутствует - откатываться будет некуда, если версия не поднимется"
fi

log "пулл образов"
compose pull

log "миграции БД"
compose run --rm api-migrate

log "перезапуск сервисов"
compose up -d --remove-orphans

wait_healthy || rollback "$PREV_TAG"

printf '%s\n' "${TAG:-latest}" > "$STATE_FILE"
log "деплой завершен: https://${TOU_DOMAIN}"
