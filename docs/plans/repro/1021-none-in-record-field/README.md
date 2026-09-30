# №1021 — голый `None` в инициализаторе поля записи

Источник — окно пакета nova-postgres, `repro-local/enum_opt_slice` (каталог в `.gitignore` пакета), отчёт `integrator-report-d8c.md` §12; перемерено приёмкой интегратора 2026-09-30 (агент sonnet). Файлы — улики `.nv.txt` (правило №695): переименуй в `.nv`, положи `nova.toml` рядом и собери `nova build src/main.nv` вне дерева репозитория с переменными окружения рантайма.

`Ev.Data { payload: None }` при поле `payload Option[[]u8]`: C — `passing 'NovaOpt_nova_int' to parameter of incompatible type 'NovaOpt_Nova_Vec____nova_byte_p'`.

**Контроль:** аннотированный локал `ro p Option[[]u8] = None` и `payload: p` — зелёно.
