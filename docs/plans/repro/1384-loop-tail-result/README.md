# №1384 — хвост-`loop` функции с `Result` оборачивается в `Ok`

Источник — окно пакета nova-postgres, `repro-local/loop_tail` (каталог в `.gitignore` пакета), отчёт `integrator-report-d8c.md` §11; перемерено приёмкой интегратора 2026-09-30 (агент sonnet). Файлы — улики `.nv.txt` (правило №695): переименуй в `.nv`, положи `nova.toml` рядом и собери `nova build src/main.nv` вне дерева репозитория с переменными окружения рантайма.

`fn … -> Result[str, str]` с хвостом `loop { … return … }`: C — `passing 'NovaRes_nova_str_nova_str *' to parameter of incompatible type 'nova_str'` (эмиттер выпускает `Ok(_nv_loop_N)`).

**Контроль:** явный `Err("unreachable")` после цикла — зелёно. Второй носитель в корпусе — `spec_tests/conformance/standalone/d483_tail_infinite_loop_payloads.nv`.
