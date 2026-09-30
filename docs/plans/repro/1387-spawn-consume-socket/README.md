# №1387 — `spawn consume` сокета не собирается

Источник — окно пакета nova-postgres, `repro-local/spawn_consume_socket` (каталог в `.gitignore` пакета), отчёт `integrator-report-d8c.md` §10; перемерено приёмкой интегратора 2026-09-30 (агент sonnet). Файлы — улики `.nv.txt` (правило №695): переименуй в `.nv`, положи `nova.toml` рядом и собери `nova build src/main.nv` вне дерева репозитория с переменными окружения рантайма.

`spawn consume s = <std TcpStream>` с пустым телом файбера: C — `initializing 'nova_int' with an expression of incompatible type 'nova_unit'` и `returning 'nova_int' from a function with incompatible result type 'nova_unit'`.

**Контроль:** `spawn consume x = 5` в изоляции — зелёно; тот же `spawn consume x = 5` рядом с непотреблённым `consume s = <TcpStream>` в той же области — КРАСНО тем же отказом (наблюдение, не установлено как причина).
