# Пробы к №899 — default-значение generic-параметра (D88) на объявлении типа не реализовано

Заведено исследовательским окном (owner-research) 2026-09-04 по замечанию интегратора: строки реестра ссылались на пробы `examples/_pidxN/`, которых в дереве нет (правило плана 278 Ф.5/Ф.6 — находка без файла-пробы не доказана; `examples/` — поставляемый корпус, не место для улик). Пробы воспроизведены заново под именами предложения `docs/dev/research/2026-09-04-typed-index-vec.md` (`Ordinal` / `@ordinal()` / `.from_ordinal()`); цитаты в строках реестра были сняты под рабочими именами `Idx` / `@idx()` / `.from_idx()` — существо то же.

Обе формы из D88 (`type Complex[T = f64]`, «тип без скобок ≡ с default») отвергаются чекером; в std и spec_tests ни одного `= Default`.

## Как запускать

Суффикс `.nv.txt` — по правилу этого каталога (README, п. 2): улика вне раннера и стражей. Для прогона скопировать без `.txt` в каталог внутри репозитория (например, сюда же) и вызвать:

```sh
cp docs/plans/repro/899-d88-default-type-param/<файл>.nv.txt docs/plans/repro/899-d88-default-type-param/<файл>.nv
nova-cli/target/release/nova.exe check docs/plans/repro/899-d88-default-type-param/<файл>.nv
nova-cli/target/release/nova.exe build docs/plans/repro/899-d88-default-type-param/<файл>.nv -o <куда-нибудь>.exe
```

## Что замерено 2026-09-04 (owner-research, `nova-cli/target/release/nova.exe`)

| проба | ожидание | `nova check` | `nova build` |
|---|---|---|---|
| `two_params.nv.txt` | check: E7310 expects 2 type arguments | `docs/plans/repro/899-d88-default-type-param/two_params.nv:17:13: error: [E7310] type `Table` expects 2 type arguments, but 1 was provided` | `—` |
| `bare_name.nv.txt` | check: E_RECV_SHAPE_MISMATCH | `docs/plans/repro/899-d88-default-type-param/bare_name.nv:11:13: error: [E_RECV_SHAPE_MISMATCH] method `width` requires receiver shape `Span[I]`, got `` | `—` |
