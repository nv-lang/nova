# Пробы к №898 — статический вызов через имя alias'а теряет тип значения в кодогене

Заведено исследовательским окном (owner-research) 2026-09-04 по замечанию интегратора: строки реестра ссылались на пробы `examples/_pidxN/`, которых в дереве нет (правило плана 278 Ф.5/Ф.6 — находка без файла-пробы не доказана; `examples/` — поставляемый корпус, не место для улик). Пробы воспроизведены заново под именами предложения `docs/dev/research/2026-09-04-typed-index-vec.md` (`Ordinal` / `@ordinal()` / `.from_ordinal()`); цитаты в строках реестра были сняты под рабочими именами `Idx` / `@idx()` / `.from_idx()` — существо то же.

Ломается ровно значение из `Alias.new()`; через аннотацию (`annotated`) и через параметр (`param`) тот же alias работает.

## Как запускать

Суффикс `.nv.txt` — по правилу этого каталога (README, п. 2): улика вне раннера и стражей. Для прогона скопировать без `.txt` в каталог внутри репозитория (например, сюда же) и вызвать:

```sh
cp docs/plans/repro/898-alias-static-ctor/<файл>.nv.txt docs/plans/repro/898-alias-static-ctor/<файл>.nv
nova-cli/target/release/nova.exe check docs/plans/repro/898-alias-static-ctor/<файл>.nv
nova-cli/target/release/nova.exe build docs/plans/repro/898-alias-static-ctor/<файл>.nv -o <куда-нибудь>.exe
```

## Что замерено 2026-09-04 (owner-research, `nova-cli/target/release/nova.exe`)

| проба | ожидание | `nova check` | `nova build` |
|---|---|---|---|
| `alias_new.nv.txt` | build: E_RECV_METHOD_MISMATCH … StringBuilder | `ok: docs/plans/repro/898-alias-static-ctor/alias_new.nv` | `error: codegen error: [E_RECV_METHOD_MISMATCH] `.push(...)` на ресивере типа `StringBuilder` — у `StringBuilder` нет метода `push`, а single-key fallb` |
| `alias_hashmap.nv.txt` | build: E_RECV_METHOD_MISMATCH … StringBuilder | `ok: docs/plans/repro/898-alias-static-ctor/alias_hashmap.nv` | `error: codegen error: [E_RECV_METHOD_MISMATCH] `.insert(...)` на ресивере типа `StringBuilder` — у `StringBuilder` нет метода `insert`, а single-key f` |
| `annotated.nv.txt` | build ok, prints 1 | `ok: docs/plans/repro/898-alias-static-ctor/annotated.nv` | `built; stdout: 1` |
| `param.nv.txt` | build ok, prints 1 | `ok: docs/plans/repro/898-alias-static-ctor/param.nv` | `built; stdout: 1` |
