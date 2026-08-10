#!/bin/sh
# Хук Stop: полная проверка перед тем, как ход считается законченным.
#
# Это последний рубеж до пуша и до стенда. PostToolUse смотрит только фронт
# и только после правки файла; здесь проверяется все разом - гейты по дереву,
# типы и линт, а если правился Rust, то еще fmt и clippy в контейнере.
#
# Rust-часть условная не из экономии, а по существу: гонять контейнер, когда
# ни один .rs не менялся, нечего. Условие - «менялся ли Rust относительно
# HEAD», а не «есть ли podman»: гейт, который отключает сам себя при погашенной
# машине, ровно тем и плох, что молчит именно тогда, когда проверить некому.
# Машина погашена и Rust правился - ход не заканчивается, и в тексте сказано,
# что поднять.
set -eu

cd "$(dirname "$0")/../.."

payload=$(cat 2>/dev/null || printf '{}')

# Повторный вызов после того, как хук уже один раз остановил ход, пропускаем:
# иначе непроходимая проверка держала бы модель в петле.
active=$(printf '%s' "$payload" | node -e "
let s = ''
process.stdin.on('data', (d) => (s += d)).on('end', () => {
  try {
    process.stdout.write(JSON.parse(s).stop_hook_active ? '1' : '0')
  } catch {
    process.stdout.write('0')
  }
})
" 2>/dev/null || printf '0')
[ "$active" = "1" ] && exit 0

problems=""

if ! gates_out=$(sh infra/scripts/gates.sh 2>&1); then
  problems="$problems
--- гейты дерева (infra/scripts/gates.sh) ---
$gates_out"
fi

if ! check_out=$(vp check --no-fmt 2>&1); then
  problems="$problems
--- типы и линт (vp check) ---
$check_out"
fi

# Правился ли Rust: и закоммиченное относительно HEAD, и еще не закоммиченное.
rust_touched=$(
  {
    git diff --name-only HEAD -- '*.rs' 'Cargo.toml' 'Cargo.lock' 2>/dev/null || true
    git ls-files --others --exclude-standard -- '*.rs' 2>/dev/null || true
  } | head -1
)

if [ -n "$rust_touched" ]; then
  if ! rust_out=$(sh infra/scripts/rust-gate.sh lint 2>&1); then
    problems="$problems
--- Rust: fmt и clippy (vp run rust:lint) ---
$rust_out"
  fi
fi

[ -z "$problems" ] && exit 0

printf 'Проверки проекта не пройдены - это нужно починить до конца хода.%s\n' \
  "$problems" >&2
exit 2
