#!/bin/sh
# Пересчет контрольных сумм примененных миграций (sqlx, SHA-384 по сырым
# байтам файла).
#
# Зачем это вообще нужно. sqlx сверяет каждую примененную миграцию с файлом
# и отказывается работать, если файл изменился: `migration NNN was previously
# applied but has been modified`. Правило простое - примененные миграции
# не переписываются (A-063). Но правка комментария схему не меняет, а стенд
# после нее не поднимается, и выбор между «оставить неверный комментарий
# навсегда» и «сломать стенд» - ложный: третий вариант - пересчитать сумму.
#
# ЭТОТ СКРИПТ НЕ ДЛЯ ИЗМЕНЕНИЯ SQL. Если в миграции поменялся хоть один
# оператор, пересчет суммы прячет расхождение схемы между стендами - то есть
# ровно то, от чего сверка и защищает. Изменение схемы - только новой
# миграцией.
#
#   ./resync-migration-checksums.sh                      # покажет, что сделает
#   ./resync-migration-checksums.sh --apply              # применит
#   PSQL="podman exec -i tou-rent-dev-postgres-1 psql -U tou_rent -d tou_rent" \
#     ./resync-migration-checksums.sh --apply            # дев-стенд
set -eu

MIGRATIONS="${MIGRATIONS:-$(dirname "$0")/../../crates/db/migrations}"
PSQL="${PSQL:-psql}"

sql=$(
  echo "BEGIN;"
  for file in "$MIGRATIONS"/*.sql; do
    version=$(basename "$file" | cut -d_ -f1)
    sum=$(sha384sum "$file" | cut -d' ' -f1)
    echo "UPDATE _sqlx_migrations SET checksum = decode('$sum', 'hex')"
    echo " WHERE version = $version AND checksum <> decode('$sum', 'hex');"
  done
  echo "COMMIT;"
)

if [ "${1:-}" = "--apply" ]; then
  printf '%s\n' "$sql" | $PSQL -v ON_ERROR_STOP=1 -q
  echo "контрольные суммы пересчитаны"
else
  printf '%s\n' "$sql"
  echo "# сухой прогон; чтобы применить - $0 --apply" >&2
fi
