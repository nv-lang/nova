# №435 — вариант в паттерне сравнивается с тегом ЧУЖОГО enum'а

Источник — окно пакета nova-postgres, `repro-local/record_enum_tag` (каталог в `.gitignore` пакета), отчёт `integrator-report-d8c.md` §9; перемерено приёмкой интегратора 2026-09-30 (агент sonnet). Файлы — улики `.nv.txt` (правило №695): переименуй в `.nv`, положи `nova.toml` рядом и собери `nova build src/main.nv` вне дерева репозитория с переменными окружения рантайма.

Два модуля: `lib.Kind.Protocol/Malformed` и `lib2.Outer.Protocol/Malformed` в одном CU. Сборка молчит; рантайм: `o=''`, `o2=''`, `s='other'` вместо ожидаемых. В C: `Nova_Outer_method_to_str` сравнивает `_nv_scr_20->tag == NOVA_TAG_Kind_Protocol` (константа `Kind`, скрутини — `Outer`).

**Контроль:** варианты `Outer` переименованы в уникальные (`OProtocol`/`OMalformed`) — все значения верны.
