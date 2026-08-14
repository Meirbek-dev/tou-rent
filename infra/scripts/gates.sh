#!/bin/sh
# Гейты, которым не нужны ни контейнер, ни база: grep по дереву и по диффу.
#
# Зачем копия того, что и так есть в .gitlab-ci.yml: до этого скрипта все они
# жили только в пайплайне, то есть срабатывали через несколько минут после
# пуша, а на машине разработчика (и у агента) не срабатывали вовсе. Стоимость
# ошибки от этого не менялась, а вот срок ее обнаружения - да. Здесь те же
# проверки, но за доли секунды и до контейнера.
#
# Пайплайн остается источником истины: он гоняет эти же гейты на чистом
# дереве, плюс те, что требуют базы и Rust. Расхождение правил здесь и там -
# ошибка; при правке одного места правьте оба.
#
# Использование: sh infra/scripts/gates.sh   (либо `vp run gates`)
set -eu

cd "$(dirname "$0")/../.."

failed=0
fail() {
  printf '\n\033[31mFAIL\033[0m %s\n' "$1"
  shift
  [ $# -gt 0 ] && printf '%s\n' "$@"
  failed=1
}
ok() { printf '\033[32mok\033[0m   %s\n' "$1"; }

# --- База сравнения для гейтов по диффу ------------------------------------
# В пайплайне ее считает infra/ci/diff-base.sh (он умеет ветки merge request
# и умеет падать, когда базу вычислить нельзя). Здесь база локальная и
# сеть не дергается: `git fetch` на каждый прогон превратил бы гейт из
# «доли секунды» в «как повезет с сетью». Правило то же, что и там:
# база либо названа вслух, либо гейт красный, - молча проверить ноль файлов
# нельзя.
base=""
for candidate in origin/main main; do
  if git rev-parse --verify -q "$candidate^{commit}" >/dev/null 2>&1; then
    base=$(git merge-base HEAD "$candidate" 2>/dev/null) || base=""
    [ -n "$base" ] && break
  fi
done
if [ -z "$base" ]; then
  base=$(git rev-parse --verify -q 'HEAD~1^{commit}' 2>/dev/null || true)
fi
if [ -z "$base" ]; then
  fail "база сравнения не вычисляется - гейты по диффу проверять нечем" \
    "нет ни origin/main, ни main, ни HEAD~1"
  exit 1
fi
printf 'база сравнения: %s (%s)\n\n' "$(git rev-parse --short "$base")" \
  "$(git log -1 --format=%s "$base" | cut -c1-60)"

# Файлы, добавленные относительно базы, включая еще не закоммиченные:
# агент правит рабочее дерево, и гейт обязан смотреть туда же, а не только
# в историю.
added_files() {
  pattern="$1"
  {
    git diff --diff-filter=A --name-only "$base" -- "$pattern" 2>/dev/null || true
    git ls-files --others --exclude-standard -- "$pattern" 2>/dev/null || true
  } | sort -u
}

# --- G2: глушители линтов и пропуски тестов без токена ----------------------
# Шаблоны - те же, что в job `suppressions`. `.skip(` ищется только в тестовых
# формах JS: в Rust `.skip(` - это Iterator::skip.
#
# Отличие от пайплайна одно, и оно намеренное: там проверяется все дерево,
# здесь - только файлы, тронутые относительно базы. Локальный гейт отвечает
# на вопрос «не я ли это сейчас принес», а пайплайн - на вопрос «чисто ли
# дерево». Проверяй здесь все дерево, любой унаследованный глушитель держал бы
# красным каждый коммит и каждый ход агента, пока его кто-нибудь не разберет, -
# и гейт первым делом отключили бы. Дерево целиком проверяется в CI и по
# `vp run gates --all`.
scope_files=""
if [ "${1:-}" = "--all" ]; then
  scope_files=$(git ls-files -- 'crates/*.rs' 'apps/*.rs' 'apps/*.ts' 'apps/*.tsx' \
    'packages/*.ts' 'packages/*.tsx' 2>/dev/null || true)
  scope_label="все дерево"
else
  scope_files=$(
    {
      git diff --name-only "$base" -- '*.rs' '*.ts' '*.tsx' 2>/dev/null || true
      git ls-files --others --exclude-standard -- '*.rs' '*.ts' '*.tsx' 2>/dev/null || true
    } | sort -u
  )
  scope_label="файлы диффа"
fi

found=""
for f in $scope_files; do
  [ -f "$f" ] || continue
  case "$f" in
  *.rs) pattern='#\[allow\(|#\[ignore\]' ;;
  *.ts | *.tsx) pattern='oxlint-disable|(it|test|describe)\.skip\(' ;;
  *) continue ;;
  esac
  hits=$(grep -nE "$pattern" "$f" 2>/dev/null | grep -v 'ALLOWED-BY-ENGINEER' || true)
  [ -n "$hits" ] && found="$found$f:$hits
"
done
if [ -n "$found" ]; then
  fail "G2: глушители без токена ALLOWED-BY-ENGINEER ($scope_label)" "$found"
else
  ok "G2: глушителей без токена нет ($scope_label)"
fi

# Запрещенных вызовов Rust (unwrap/expect/panic!/println! в не-тестовом коде,
# регламент А.4) здесь намеренно нет. Grep их не отличает от тех же вызовов
# в модулях #[cfg(test)], где они разрешены: на этом дереве такая проверка
# дает полсотни ложных срабатываний и приучает проходить мимо гейта. Ловит их
# clippy - точно, с разбором контекста и по правилам clippy.toml, - и стоит
# это секунды: `vp run rust:lint`.

# --- G2/SQL: время в новых миграциях - только из core.now() -----------------
# Проверяются только добавленные миграции: примененные - защищенный путь,
# и bare now() в них остался с тех пор, когда правила ADR-0005 еще не было.
new_migrations=$(added_files 'crates/db/migrations/*.sql')
bare=""
for file in $new_migrations; do
  [ -f "$file" ] || continue
  # Комментарии вырезаются до поиска: в них now() упоминается по делу,
  # а нумерация строк от этого не сбивается.
  hits=$(sed 's/--.*$//' "$file" |
    grep -nE '(^|[^.[:alnum:]_])now\(\)' |
    grep -v 'core\.now()' || true)
  [ -n "$hits" ] && bare="$bare$file:$hits
"
done
if [ -n "$bare" ]; then
  fail "G2/SQL: время мимо core.now() (NFR-03, ADR-0005)" "$bare"
elif [ -n "$new_migrations" ]; then
  ok "G2/SQL: время в новых миграциях берется из core.now()"
else
  ok "G2/SQL: новых миграций в диффе нет"
fi

# --- Перевод строки в миграциях --------------------------------------------
# Контрольную сумму миграции sqlx считает по сырым байтам файла. Файл,
# переписанный на Windows с CRLF, дает «migration NNN was previously applied
# but has been modified» при чистом `git status` - и опознается это тяжело.
# Гейт дешевый, а диагноз дорогой.
# Перевод строки берется у самого git (`--eol` печатает вид в рабочем дереве
# и в индексе), а не поиском CR регулярным выражением: `$'\r'` в POSIX sh
# не разворачивается, и шаблон вырождается в букву `r` - такая проверка
# «находит» все файлы подряд.
crlf=$(git ls-files --eol -- 'crates/db/migrations/*.sql' |
  awk '$1 != "i/lf" || $2 != "w/lf" { print $NF }' || true)
if [ -n "$crlf" ]; then
  fail "миграции с CRLF - sqlx сверяет контрольную сумму по сырым байтам" \
    "$crlf" "починка: git show HEAD:<файл> > <файл>"
else
  ok "миграции: перевод строки LF"
fi

# --- G3: есть query!-макросы - обязан быть слепок .sqlx ---------------------
if grep -rqn --include='*.rs' -E 'sqlx::query(_as|_scalar)?!' crates apps 2>/dev/null; then
  if [ -d .sqlx ]; then
    ok "G3: слепок .sqlx на месте"
  else
    fail "G3: есть query!-макросы, но нет .sqlx" "почините: cargo sqlx prepare"
  fi
else
  ok "G3: query!-макросов нет"
fi

# --- Кодоген без диффа (G5, второе звено) ----------------------------------
# openapi.json → schema.d.ts. Первое звено (код → openapi.json) проверяет
# тест openapi_sync.rs в Rust-гейтах: для него нужен собранный api.
if git diff --quiet -- packages/api-client/src/schema.d.ts 2>/dev/null &&
  git diff --quiet -- packages/api-client/openapi.json 2>/dev/null; then
  ok "G5: выход кодогена совпадает с закоммиченным"
else
  fail "G5: выход кодогена разошелся с закоммиченным" \
    "$(git diff --stat -- packages/api-client)" \
    "перегенерируйте (bun run codegen) и закоммитьте результат"
fi

printf '\n'
if [ "$failed" -ne 0 ]; then
  printf '\033[31mгейты не пройдены\033[0m\n'
  exit 1
fi
printf '\033[32mгейты пройдены\033[0m\n'
