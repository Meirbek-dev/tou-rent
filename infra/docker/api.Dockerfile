# Образ api (NFR-10): сборка в rust:1.97, запуск на debian-slim.
# Миграции едут в образе - их накатывает сервис api-migrate той же версией.
FROM rust:1.97 AS build
WORKDIR /w
# Запросы проверяются макросами sqlx по схеме. В сборке образа БД нет,
# поэтому проверка идет по слепку `.sqlx` (в репозитории, гейт G3):
# без этой переменной сборка попыталась бы сходить в живую базу.
ENV SQLX_OFFLINE=true

# Слой зависимостей: манифесты меняются реже кода
COPY Cargo.toml Cargo.lock rust-toolchain.toml clippy.toml ./
COPY apps/api/Cargo.toml apps/api/Cargo.toml
COPY apps/jobs/Cargo.toml apps/jobs/Cargo.toml
COPY crates/application/Cargo.toml crates/application/Cargo.toml
COPY crates/db/Cargo.toml crates/db/Cargo.toml
COPY crates/domain/Cargo.toml crates/domain/Cargo.toml
COPY crates/http/Cargo.toml crates/http/Cargo.toml
COPY crates/ports/Cargo.toml crates/ports/Cargo.toml
COPY crates/testkit/Cargo.toml crates/testkit/Cargo.toml
RUN mkdir -p apps/api/src apps/jobs/src crates/application/src crates/db/src \
      crates/domain/src crates/http/src crates/ports/src crates/testkit/src \
 && echo 'fn main() {}' > apps/api/src/main.rs \
 && echo 'fn main() {}' > apps/jobs/src/main.rs \
 && for lib in application db domain http ports testkit; do touch "crates/$lib/src/lib.rs"; done \
 && cargo build --release -p api \
 && rm -rf apps crates

COPY . .
# Штампы времени у скопированных файлов старше сборки заглушек - без touch
# cargo считает пустые крейты-заглушки свежими и собирает api против них
RUN find apps crates -name '*.rs' -exec touch {} + \
 && cargo build --release --locked -p api -p jobs

FROM debian:trixie-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /w/target/release/api /usr/local/bin/api
# Двигатель обязательств (FR-1702) живет в том же образе - другая команда
COPY --from=build /w/target/release/jobs /usr/local/bin/jobs
COPY --from=build /w/crates/db/migrations /app/crates/db/migrations

# Непривилегированный пользователь (NFR-07)
RUN useradd --system --uid 10001 tou
USER tou

ENV API_ADDR=0.0.0.0:8080 MIGRATIONS_DIR=/app/crates/db/migrations
EXPOSE 8080
HEALTHCHECK --interval=15s --timeout=3s --retries=5 \
  CMD curl -fsS http://127.0.0.1:8080/api/v1/healthz || exit 1
ENTRYPOINT ["api"]
