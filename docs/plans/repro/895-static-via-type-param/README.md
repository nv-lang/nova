# Пробы к №895 — статический метод протокола через параметр типа и статический set-blanket не резолвятся в кодогене

Заведено исследовательским окном (owner-research) 2026-09-04 по замечанию интегратора: строки реестра ссылались на пробы `examples/_pidxN/`, которых в дереве нет (правило плана 278 Ф.5/Ф.6 — находка без файла-пробы не доказана; `examples/` — поставляемый корпус, не место для улик). Пробы воспроизведены заново под именами предложения `docs/dev/research/2026-09-04-typed-index-vec.md` (`Ordinal` / `@ordinal()` / `.from_ordinal()`); цитаты в строках реестра были сняты под рабочими именами `Idx` / `@idx()` / `.from_idx()` — существо то же.

Чекер зелёный, падает `nova build`. Контроль `direct` — тот же статический вызов без параметра типа собирается и печатает.

## Как запускать

Суффикс `.nv.txt` — по правилу этого каталога (README, п. 2): улика вне раннера и стражей. Для прогона скопировать без `.txt` в каталог внутри репозитория (например, сюда же) и вызвать:

```sh
cp docs/plans/repro/895-static-via-type-param/<файл>.nv.txt docs/plans/repro/895-static-via-type-param/<файл>.nv
nova-cli/target/release/nova.exe check docs/plans/repro/895-static-via-type-param/<файл>.nv
nova-cli/target/release/nova.exe build docs/plans/repro/895-static-via-type-param/<файл>.nv -o <куда-нибудь>.exe
```

## Что замерено 2026-09-04 (owner-research, `nova-cli/target/release/nova.exe`)

| проба | ожидание | `nova check` | `nova build` |
|---|---|---|---|
| `newtype_bound.nv.txt` | build: E_UNKNOWN_STATIC_METHOD int.from_ordinal | `ok: docs/plans/repro/895-static-via-type-param/newtype_bound.nv` | `error: codegen error: [E_UNKNOWN_STATIC_METHOD] `int.from_ordinal(...)` — у примитива `int` нет статического метода `from_ordinal`. Валидные static-ме` |
| `set_blanket_static.nv.txt` | build: INTERNAL-PANIC E_CODEGEN_TYPE_UNKNOWN | `ok: docs/plans/repro/895-static-via-type-param/set_blanket_static.nv` | `error: codegen error: [INTERNAL-PANIC] [E_CODEGEN_TYPE_UNKNOWN] Path call return type unknown for method=from_ordinal` |
| `direct.nv.txt` | build ok, prints 42 | `ok: docs/plans/repro/895-static-via-type-param/direct.nv` | `built; stdout: 42` |
