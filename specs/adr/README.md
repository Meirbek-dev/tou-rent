# ADR - architecture decision records

Формат: один файл `NNNN-slug.md` на решение; статусы `accepted / superseded by NNNN`.
Записываются решения, отклоняющиеся от Архитектуры v2 или дополняющие ее;
краткие допущения без выбора альтернатив - в [ASSUMPTIONS.md](../ASSUMPTIONS.md).

| №    | Решение                                                                                     | Статус   |
| ---- | ------------------------------------------------------------------------------------------- | -------- |
| 0001 | [Нативные git-хуки Vite+ вместо lefthook](0001-vite-plus-hooks.md)                          | accepted |
| 0002 | [PostgreSQL 19 beta в dev, 18 - прод](0002-postgres-19beta-dev.md)                          | accepted |
| 0003 | [Zitadel вместо Keycloak](0003-zitadel-instead-of-keycloak.md)                              | accepted |
| 0004 | [WORM-хранение досье: Object Lock в режиме compliance](0004-worm-object-lock-compliance.md) | accepted |
| 0005 | [Управляемое время стенда](0005-controllable-server-time.md)                                | accepted |
| 0006 | [RustFS вместо MinIO](0006-rustfs-instead-of-minio.md)                                      | accepted |
