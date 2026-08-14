#!/bin/sh
# Хук PostToolUse (Write|Edit): типы и линт сразу после правки файла фронта.
#
# Зачем: ошибка типа, найденная через полминуты после правки, стоит одну
# правку; та же ошибка, найденная в конце длинной цепочки правок, стоит
# разбора, что из написанного было верно. `vp check --no-fmt` на этом дереве
# идет около двух секунд - дешевле, чем перечитывать свой же диф.
#
# Формат не трогаем: его правит `vp staged` на коммите, и переформатирование
# файла прямо под агентом сбивало бы ему представление о содержимом.
#
# Вход - JSON на stdin (см. документацию хуков), выход - код 2, чтобы вернуть
# ошибку модели.
set -eu

cd "$(dirname "$0")/../.."

payload=$(cat)

# Путь берется разбором JSON, а не grep-ом по строке: на Windows он приходит
# с экранированными обратными слэшами, и регулярное выражение их не разберет.
file=$(printf '%s' "$payload" | node -e "
let s = ''
process.stdin.on('data', (d) => (s += d)).on('end', () => {
  try {
    const j = JSON.parse(s)
    process.stdout.write(j.tool_response?.filePath ?? j.tool_input?.file_path ?? '')
  } catch {
    process.stdout.write('')
  }
})
" 2>/dev/null || true)

case "$file" in
*.ts | *.tsx) ;;
*) exit 0 ;;
esac

# Выход кодогена и генерируемые файлы под правило не попадают: их пишет
# генератор, а не агент.
case "$file" in
*/routeTree.gen.ts | */paraglide/* | */schema.d.ts) exit 0 ;;
esac

if out=$(vp check --no-fmt 2>&1); then
  exit 0
fi

printf 'vp check не проходит после правки %s:\n\n%s\n' "$file" "$out" >&2
exit 2
