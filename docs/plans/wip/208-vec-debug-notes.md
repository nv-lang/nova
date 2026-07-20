# [M-208-vec-chained-debug-display-red] — расследование + фикс

Рабочий чекпоинт (worktree `nova-vecdbg`, ветка `p-fix-208-vec-chained-debug`).

## Симптом

`spec_tests/conformance/vec_f32_chained_debug.nv` — 5 тестов, все используют
`consume a = StringBuilder.new(); Vec[...].debug(a)` / `.display(a)` затем
`assert(a.into_str() == "Vec[...]")`. RUN-FAIL (assert fails), не CC-FAIL.

## Изолированный репро

`examples/_vecdbg_repro/main.nv` (throwaway, вне conformance-CU) — те же 5
вызовов + println фактического значения. Фактический вывод: **пустая строка
для ВСЕХ 5** (включая `Vec[int]`-control, который якобы "already works" по
комментарию файла — на самом деле НЕ работает после 208).

## Гипотеза — по коду (НЕ гадание)

Формат Vec-Display НЕ изменился: `git diff ab2340973 c53ed9ba7 --
std/src/collections/vec/protocols.nv` показывает миграцию `(mut w Write)` →
`(mut f Fmt)`, тело `f.write("Vec[")...f.write("]")` — байт-в-байт та же
форма `Vec[e0, e1, ...]`. Канон = `Vec[...]` (подтверждено).

Что РЕАЛЬНО изменилось (D422, Plan 208 Ф.2, `std/src/prelude/protocols.nv`):
`Display.@display`/`Debug.@debug` теперь принимают `Fmt` (богатый протокол:
`use Write` + width/precision/align/fill/sign/alternate/kind/`@pad`), а НЕ
голый `Write`. `StringBuilder` реализует только `Write` (`@write(bytes []u8)`),
НЕ `Fmt` целиком. Раньше (`Write`-подпись) `.debug(a)` с `a: StringBuilder`
работал напрямую; теперь `a` не удовлетворяет `Fmt` структурно — прямой вызов
`.debug(a)` больше НЕ валиден без обёртки.

**Это ДОКУМЕНТИРОВАННОЕ, УЖЕ ПРИМЕНЁННОЕ в той же волне решение**, не
придумано мной:
- `spec_tests/conformance/d374_write_sink_decouple.nv:41-58` — тест
  прямым текстом: "`StringBuilder` alone satisfies `Write` but not the full
  `Fmt` interface, so it can no longer be passed AS `Fmt` directly (pre-D422
  it could...)" — и использует канонический вызов
  `p.display(FmtCtx.bare(sb, 0, false))`.
- `std/src/collections/vec/protocols_test.nv:24-29` — тот же канон:
  `v.display(FmtCtx.bare(sb, 0, false))`.
- `std/src/time/duration/core.nv:1156` — тот же канон для `.debug`:
  `2.to_seconds().debug(FmtCtx.bare(sb1, 0, true))`.

`vec_f32_chained_debug.nv` — единственный НЕ мигрированный fixture (написан
до 208, для СТАРОЙ `Write`-подписи, заголовок ссылается на старый маркер
`[M-154.1-chained-vec-f32-method-misdispatch]`) — пропущен волной 208 Ф.3,
т.к. её grep-аудит был scoped на `std/**`-импленты `@display`/`@debug`, а
не на call-сайты в `spec_tests/`.

**Вывод: гипотеза A' — API/calling-convention СОЗНАТЕЛЬНО изменена (D422),
формат-строка НЕ изменилась. Фикс = обновить тест на канон `FmtCtx.bare(...)`,
НЕ ослабление — синхронизация со УЖЕ применённым в той же волне каноном.**

## Побочная находка (НЕ фикшу в этой волне — вне объёма)

Почему `.debug(a)` вообще СКОМПИЛИРОВАЛСЯ с `a: StringBuilder` вместо
`Fmt`-ошибки? `compiler-codegen/src/types/mod.rs::resolved_cat_of_depth`
(~17231) мапит ЛЮБОЙ `TypeDeclKind::Protocol` expected-тип → `ResolvedType::Any`
(permissive, как generic-параметр). `assignable_direct` (~14064) сразу
возвращает `Compat::Ok` для `exp_rt == Any` — т.е. ЛЮБОЙ аргумент проходит
проверку для ЛЮБОГО protocol-typed параметра, БЕЗ структурной проверки
удовлетворения протокола. Codegen тоже не вставляет никакой auto-wrap/coercion
— просто прокидывает сырой указатель. Итог в С: `Nova_StringBuilder*`
передаётся туда, где ожидается `Nova_FmtCtx*` (оба типа начинаются с
указательного поля — `StringBuilder.buf` vs `FmtCtx.sink` — отсюда НЕ крэш, а
двойная type confusion через смещение 0, итог — пустая строка вместо
"Vec[...]"). Это ОБЩИЙ (не Vec-специфичный) пробел чекера: любой
protocol-typed параметр принимает произвольный аргумент без проверки. Риск
реален (тихий memory-unsafe вместо ошибки компиляции), но: (1) это НЕ
регрессия 208 конкретно под Vec — тот же permissive-путь существовал и
раньше, просто раньше все реальные call-сайты СЛУЧАЙНО совпадали структурно;
(2) фикс этого пробела — отдельная, рискованная, широкая задача (полная
protocol-conformance проверка аргументов для ВСЕХ protocol-параметров
кодовой базы, а не только Fmt/Write) — вне объёма и "зоны" этого P1-маркера
(тест-фикс). Задокументировано здесь + в отчёте владельцу; возможный будущий
маркер, НЕ создаю новый файл-маркер без указания владельца (see
feedback-no-external-memory-for-project-state — plan-state правки не мои).

## Фикс

`spec_tests/conformance/vec_f32_chained_debug.nv` — все 5 вызовов
`.debug(a)`/`.display(a)` → `.debug(FmtCtx.bare(a, 0, true))` /
`.display(FmtCtx.bare(a, 0, false))`, заголовочный комментарий обновлён
(старый маркер 154.1 остаётся как история, + примечание про 208/D422
миграцию вызова).

## Гейты

- Узкий репро (`examples/_vecdbg_repro/main.nv`) с `FmtCtx.bare(...)` —
  фактический вывод сверен с ожиданием ДО правки самого conformance-файла.
- Полный `spec_tests/conformance` (standalone single-CU) — см. итог в отчёте.
