#!/bin/sh
# База сравнения для гейтов, работающих по диффу (G4 - squawk, G2/SQL - core.now(),
# G9 - мутации домена). Печатает в stdout один git-ref; все прочее уходит в stderr,
# чтобы вызывающий мог написать `BASE=$(sh infra/ci/diff-base.sh)`.
#
# Зачем отдельный файл: до T74 каждый такой гейт вычислял базу сам и при пустом
# CI_MERGE_REQUEST_TARGET_BRANCH_NAME молча уходил в ветку «новых миграций нет».
# Переменная эта заполняется только в пайплайнах merge request, а их в проекте
# не было вовсе (не было и workflow-правил) - то есть гейты годами отчитывались
# зеленым, не посмотрев ни на одну миграцию. Теперь база либо определена, либо
# скрипт падает: гейт, который не может вычислить свой вход, обязан быть красным,
# а не зеленым (регламент А.4).
set -eu

log() { printf 'diff-base: %s\n' "$*" >&2; }

# Глубина клона в CI по умолчанию мелкая - до базы можно не дотянуться
fetch_ref() {
  git fetch -q --depth=100 origin "$1" >/dev/null 2>&1 \
    || git fetch -q origin "$1" >/dev/null 2>&1 \
    || { log "не удалось получить ветку '$1' из origin"; exit 1; }
}

default_branch="${CI_DEFAULT_BRANCH:-main}"

if [ -n "${CI_MERGE_REQUEST_TARGET_BRANCH_NAME:-}" ]; then
  # Пайплайн merge request: сравниваем с целевой веткой
  fetch_ref "$CI_MERGE_REQUEST_TARGET_BRANCH_NAME"
  base="origin/$CI_MERGE_REQUEST_TARGET_BRANCH_NAME"
elif [ "${CI_COMMIT_BRANCH:-}" = "$default_branch" ]; then
  # Пайплайн самой main: сравниваем с ее предыдущим состоянием. Для влитого
  # MR это ровно его дифф, для прямого коммита - этот коммит
  base="${CI_COMMIT_BEFORE_SHA:-HEAD~1}"
  # Первый коммит в ветке и мелкий клон дают нулевой SHA либо оборванную историю
  case "$base" in
  0000000000000000000000000000000000000000) base="HEAD~1" ;;
  esac
  git fetch -q --deepen=100 >/dev/null 2>&1 || true
else
  # Ветка без открытого MR: сравниваем с веткой по умолчанию
  fetch_ref "$default_branch"
  base="origin/$default_branch"
fi

git rev-parse --verify -q "$base^{commit}" >/dev/null || {
  log "база '$base' не разрешается в коммит - дифф вычислить нельзя"
  log "гейты по диффу не должны молча пропускать проверку; исправьте окружение пайплайна"
  exit 1
}

log "база сравнения: $base"
printf '%s\n' "$base"
