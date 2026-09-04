# Пробы к №896 — blanket по type-set не засчитывается в конформанс протокола

Заведено исследовательским окном (owner-research) 2026-09-04 по замечанию интегратора: строки реестра ссылались на пробы `examples/_pidxN/`, которых в дереве нет (правило плана 278 Ф.5/Ф.6 — находка без файла-пробы не доказана; `examples/` — поставляемый корпус, не место для улик). Пробы воспроизведены заново под именами предложения `docs/dev/research/2026-09-04-typed-index-vec.md` (`Ordinal` / `@ordinal()` / `.from_ordinal()`); цитаты в строках реестра были сняты под рабочими именами `Idx` / `@idx()` / `.from_idx()` — существо то же.

Контроль `dispatch` — тот же blanket-метод прямым вызовом чекер видит. Один вопрос «есть ли у `int` метод `ordinal`», два ответа.

## Как запускать

Суффикс `.nv.txt` — по правилу этого каталога (README, п. 2): улика вне раннера и стражей. Для прогона скопировать без `.txt` в каталог внутри репозитория (например, сюда же) и вызвать:

```sh
cp docs/plans/repro/896-set-blanket-conformance/<файл>.nv.txt docs/plans/repro/896-set-blanket-conformance/<файл>.nv
nova-cli/target/release/nova.exe check docs/plans/repro/896-set-blanket-conformance/<файл>.nv
nova-cli/target/release/nova.exe build docs/plans/repro/896-set-blanket-conformance/<файл>.nv -o <куда-нибудь>.exe
```

## Что замерено 2026-09-04 (owner-research, `nova-cli/target/release/nova.exe`)

| проба | ожидание | `nova check` | `nova build` |
|---|---|---|---|
| `conformance.nv.txt` | check: E_BOUND_NOT_SATISFIED | `docs/plans/repro/896-set-blanket-conformance/conformance.nv:11:31: error: [E_BOUND_NOT_SATISFIED] type `int` does not satisfy `Ordinal` bound (in call` | `—` |
| `dispatch.nv.txt` | check ok | `ok: docs/plans/repro/896-set-blanket-conformance/dispatch.nv` | `built; stdout: 7` |
