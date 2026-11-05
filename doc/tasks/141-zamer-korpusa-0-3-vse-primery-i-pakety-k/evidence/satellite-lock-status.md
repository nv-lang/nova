# Состояние спутников после замера 141

Замер 2026-11-05. Файлы не откатывались; изменения не включаются в коммит задачи.

**Итог: состояние всех 9 спутников совпало с записанным в 135
(`../../135-proverit-obedinennye-ispravleniya-0-3-na/evidence/satellite-lock-status.md`)
— новых расхождений замер 141 не внёс.**

## nova-polaris @ 24f9d1757868

12 изменённых `examples/*/nova.lock.toml` — дрейф прежний (от замера 135,
`lockfile::sync` переписал pin `http` `>=0.1.0, <0.2.0` → `>=0.2.0, <0.3.0`).
Проверено: `git diff --stat` — ровно 12 файлов по 1 строке; diff
`examples/01-hello/nova.lock.toml` байт-в-байт тот же, что записан в 135
(index `6e4c960..4d5a6eb`).

## nova-duckdb @ a59a07328030

```text
?? .vscode/                                        (2026-09-16, до замеров 135/141)
?? examples/01-hello/nova.lock.toml
?? examples/02-appender-history/nova.lock.toml
?? examples/03-with-tls/nova.lock.toml
```

Три untracked lock-файла — то же состояние, что записано в 135; содержимое
`examples/01-hello/nova.lock.toml` совпало с записанным там дословно
(pin duckdb `>=0.1.0, <0.2.0`, version 0.1.1, commit
`ba7b8a1b39cbf05a9c8de03aac1d5f3f06653d97`). mtime файлов — 2026-11-05,
время прогона пакетов 141: `lockfile::sync` драйвера пересоздал их
идентичными (в 135 они были созданы тем же механизмом и, видимо, позже
удалены из рабочей копии).

## Остальные спутники — чисто

```text
nova-http     @ 5505f31c4ef8 : clean
nova-tls      @ 01d323b6423d : clean
nova-socks    @ 9d44e5d5353e : clean
nova-compress @ f11c523ecc68 : clean
nova-bignum   @ 3d865a055369 : clean
nova-postgres @ 8a46520269cf : clean
nova-sdl      @ e636ac9286df : clean
```

HEAD'ы всех 9 спутников совпали с таблицей 135 (`packages.tsv`, колонка head)
до начала прогонов — замер шёл на тех же ревизиях.
