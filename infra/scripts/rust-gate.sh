#!/bin/sh
# Rust-проверки на машине разработчика - до того, как поднимется стенд.
#
# Windows-хост нативные бинарники не линкует (A-003), поэтому cargo здесь
# запускается в контейнере. Контейнер берется не произвольным `podman run`,
# а сервисом `api` дев-стенда: у него уже описаны и монтирование workspace,
# и кеш-тома (tou-rent-cargo/-target/-rustup), и переменные окружения.
# Отдельная команда `podman run` их бы дублировала - и разошлась бы с compose
# при первой же правке одного из двух мест.
#
# --no-deps: базе тут взяться неоткуда и она не нужна. Запросы проверяются
# по offline-слепку `.sqlx` (SQLX_OFFLINE=true) - тому же, по которому
# собираются задания пайплайна без сервиса postgres. Свежесть слепка стережет
# `sqlx prepare --check` в CI.
#
# Кеш общий с сервисом api, поэтому после первого прогона проверка идет
# секунды, а не минуты: `cargo check --workspace --all-targets` на теплом
# кеше - около 15 с.
#
# Проверка называется словом, а не собирается из аргументов в package.json:
# составную команду («fmt, а следом clippy») там пришлось бы писать через
# `sh -c "... && ..."`, и `&&` съедала бы внешняя оболочка - вторая половина
# выполнялась бы на хосте, где cargo нет. Здесь склейка живет в одном месте.
#
# Использование: sh infra/scripts/rust-gate.sh <check|lint|fmt>
set -eu

cd "$(dirname "$0")/../.."

case "${1:-}" in
check) inner='cargo check --workspace --all-targets' ;;
lint) inner='cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings' ;;
fmt) inner='cargo fmt --all' ;;
*)
  echo "rust-gate: укажите проверку - check | lint | fmt" >&2
  exit 2
  ;;
esac

if ! command -v podman >/dev/null 2>&1; then
  echo "rust-gate: podman не найден - Rust на этом хосте не собирается (A-003)" >&2
  exit 1
fi

# Машина podman погашена - это не повод пропустить проверку молча: гейт,
# который не может отработать, обязан быть красным (регламент А.4).
if ! podman info >/dev/null 2>&1; then
  cat >&2 <<'EOF'
rust-gate: podman не отвечает - Rust-проверки не выполнены.
Поднимите машину: podman machine start
EOF
  exit 1
fi

exec podman compose -f infra/compose/docker-compose.dev.yml --profile api \
  run --rm --no-deps -e SQLX_OFFLINE=true api sh -c "$inner"
