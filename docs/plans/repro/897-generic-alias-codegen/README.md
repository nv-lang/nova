# Пробы к №897 — обобщённый alias с частичным применением не эмитируется кодогеном

Заведено исследовательским окном (owner-research) 2026-09-04 по замечанию интегратора: строки реестра ссылались на пробы `examples/_pidxN/`, которых в дереве нет (правило плана 278 Ф.5/Ф.6 — находка без файла-пробы не доказана; `examples/` — поставляемый корпус, не место для улик). Пробы воспроизведены заново под именами предложения `docs/dev/research/2026-09-04-typed-index-vec.md` (`Ordinal` / `@ordinal()` / `.from_ordinal()`); цитаты в строках реестра были сняты под рабочими именами `Idx` / `@idx()` / `.from_idx()` — существо то же.

`spec_example` — дословный пример спеки (`02-types.md:224`). Контроль `plain_alias` — необобщённый alias через аннотацию типа собирается.

## Как запускать

Суффикс `.nv.txt` — по правилу этого каталога (README, п. 2): улика вне раннера и стражей. Для прогона скопировать без `.txt` в каталог внутри репозитория (например, сюда же) и вызвать:

```sh
cp docs/plans/repro/897-generic-alias-codegen/<файл>.nv.txt docs/plans/repro/897-generic-alias-codegen/<файл>.nv
nova-cli/target/release/nova.exe check docs/plans/repro/897-generic-alias-codegen/<файл>.nv
nova-cli/target/release/nova.exe build docs/plans/repro/897-generic-alias-codegen/<файл>.nv -o <куда-нибудь>.exe
```

## Что замерено 2026-09-04 (owner-research, `nova-cli/target/release/nova.exe`)

| проба | ожидание | `nova check` | `nova build` |
|---|---|---|---|
| `spec_example.nv.txt` | build: undeclared identifier Nova_StringMap____nova_int | `ok: docs/plans/repro/897-generic-alias-codegen/spec_example.nv` | `compiler error: error: use of undeclared identifier 'Nova_StringMap____nova_int'` |
| `partial.nv.txt` | build: unknown type name Nova_IntTable____nova_str | `ok: docs/plans/repro/897-generic-alias-codegen/partial.nv` | `compiler error: error: unknown type name 'Nova_IntTable____nova_str'; did you mean 'Nova_Vec____nova_str'?` |
| `plain_alias.nv.txt` | build ok, prints 1 | `ok: docs/plans/repro/897-generic-alias-codegen/plain_alias.nv` | `built; stdout: 1` |

Попутно в `spec_example`: строка C `Nova_StringMap____nova_int* m = Nova_EmbeddedDir_static_new();` — конструктор `StringMap[int].new()` резолвлен в `EmbeddedDir.new` (single-key fallback по имени `new`, семья №898/№254). Если бы typedef существовал, это была бы ТИХАЯ подмена конструктора, а не CC-FAIL.
