# Пробы к №628 (второй носитель) — `[]` на обобщённой пользовательской записи не маршрутизируется в `@index` кодогеном

Заведено исследовательским окном (owner-research) 2026-09-04 по замечанию интегратора: строки реестра ссылались на пробы `examples/_pidxN/`, которых в дереве нет (правило плана 278 Ф.5/Ф.6 — находка без файла-пробы не доказана; `examples/` — поставляемый корпус, не место для улик). Пробы воспроизведены заново под именами предложения `docs/dev/research/2026-09-04-typed-index-vec.md` (`Ordinal` / `@ordinal()` / `.from_ordinal()`); цитаты в строках реестра были сняты под рабочими именами `Idx` / `@idx()` / `.from_idx()` — существо то же.

Чтение и запись через `[]` падают в C; контроль `explicit` — те же `@index` явными вызовами собираются. Вместе с №894 (чекер не проверяет ключ) D238/D240 для user-типов не реализованы ни с одной стороны.

## Как запускать

Суффикс `.nv.txt` — по правилу этого каталога (README, п. 2): улика вне раннера и стражей. Для прогона скопировать без `.txt` в каталог внутри репозитория (например, сюда же) и вызвать:

```sh
cp docs/plans/repro/628-user-index-codegen-generic/<файл>.nv.txt docs/plans/repro/628-user-index-codegen-generic/<файл>.nv
nova-cli/target/release/nova.exe check docs/plans/repro/628-user-index-codegen-generic/<файл>.nv
nova-cli/target/release/nova.exe build docs/plans/repro/628-user-index-codegen-generic/<файл>.nv -o <куда-нибудь>.exe
```

## Что замерено 2026-09-04 (owner-research, `nova-cli/target/release/nova.exe`)

| проба | ожидание | `nova check` | `nova build` |
|---|---|---|---|
| `read.nv.txt` | build: C error passing struct to nova_int | `ok: docs/plans/repro/628-user-index-codegen-generic/read.nv` | `compiler error: error: passing 'Nova_Table____nova_int__nova_str' (aka 'struct Nova_Table____nova_int__nova_str') to parameter of incompatible type 'nova_int' (aka 'l` |
| `write.nv.txt` | build: C error assigning str to struct | `ok: docs/plans/repro/628-user-index-codegen-generic/write.nv` | `compiler error: error: assigning to 'Nova_Table____nova_int__nova_str' (aka 'struct Nova_Table____nova_int__nova_str') from incompatible type 'const nova_str'` |
| `explicit.nv.txt` | build ok, prints B | `ok: docs/plans/repro/628-user-index-codegen-generic/explicit.nv` | `built; stdout: B` |
