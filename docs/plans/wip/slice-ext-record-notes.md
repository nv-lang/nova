# Чекпоинт — [M-static-conv-array-record-mono-cc-fail] (2026-07-17)

Worktree: `d:/Sources/nv-lang/nova-slicemono` (branch `p-fix-slice-ext-record-mono`).
Бинарь: `cargo build --release --manifest-path nova-cli/Cargo.toml`.

**Статус: НЕ ЗАКРЫТ** — RED воспроизведён надёжно, корень локализован до узкого
множества кандидатов в некопируемой (frozen-adjacent) инфраструктуре чекера/
кодогена, но однострочный фикс не найден за отведённое время. Маркер
ОСТАЁТСЯ открытым; `nova:allow` подавления в std НЕ сняты; переименования
`.from()` → `to_*/into_*` НЕ возвращены.

## КРИТИЧЕСКАЯ находка методологии (для следующей волны)

`compiler-codegen/src/codegen/external_registry.rs` эмбеддит
`std/src/runtime/read_buffer.nv` / `write_buffer.nv` / `string_builder.nv` /
`sync.nv` и др. через `include_str!` (компилируются В БИНАРЬ RUSTC на этапе
сборки `nova-codegen`). **Cargo НЕ ВСЕГДА корректно инвалидирует инкрементальный
кэш при правке ТОЛЬКО `.nv`-файла** (сам `.rs`-файл с `include_str!` не менялся)
— на этой машине/Windows наблюдалось МНОГОКРАТНО: правка `read_buffer.nv` +
`cargo build --release` давала БИНАРЬ СО СТАРЫМ эмбеддедом содержимым (разное
между последовательными rebuild'ами БЕЗ ЛЮБЫХ .rs-правок — три разных билда
одного и того же `.nv`-diff дали ТРИ РАЗНЫХ симптома: `E_RECV_METHOD_MISMATCH`
на ресивере `HashMap`, тот же на `WriteBuffer`, и `[E7320] no field or method
ptr/len on []u8` в СОВЕРШЕННО не относящемся к `read_buffer.nv` файле
`string/core.nv`). Все три — ложные срабатывания STALE-эмбеддеда, НЕ
детерминированный баг компилятора.

**Обязательная процедура при следующей волне**: после КАЖДОЙ правки
`std/src/runtime/{read_buffer,write_buffer,string_builder,sync,...}.nv` —
`touch compiler-codegen/src/codegen/external_registry.rs` ПЕРЕД
`cargo build --release --manifest-path nova-cli/Cargo.toml`, иначе результаты
теста недостоверны. (Ещё надёжнее — `cargo build -p nova-codegen --release`
дважды подряд без правок между ними: если результат СТАБИЛЕН — кэш свежий.)

Также обнаружено: `nova test --positive --compile-error spec_tests/conformance`
(ДИРЕКТОРИЯ) **НЕ включает** корневые `.nv`-файлы папки (read_nav.nv,
write_constructors.nv, roundtrip.nv и ~980 др. с `module spec_tests.conformance`)
в `target/last-test-results.json` — они просто отсутствуют в выдаче (ни PASS,
ни FAIL). Похоже, директорийный режим обходит только под-папки
(`standalone/`, `neg/`, `consume_fixtures/`, `d78_*` и т.п.) как отдельные
пакеты, а корневой co-equal-модуль пропускает. **Рабочая форма** — явные пути
файлов: `nova test --strict-effects spec_tests/conformance/read_nav.nv
spec_tests/conformance/write_constructors.nv` (это подтягивает ВСЕ ~986
co-equal peer-файлов автоматически, т.к. они делят `module
spec_tests.conformance`) — именно так исторически была найдена RED (отчёт
задачи fix-runtime-lint-debt, коммит `25217fc88`).

## RED (дословный, ПОСЛЕ учёта stale-cache гочи)

Точная форма §1а восстановлена (static `.from` УБРАН, только extension-метод):

```nova
// read_buffer.nv
export fn []u8 @to_readbuffer() -> ReadBuffer => { data: @, pos: 0 }

// write_buffer.nv
export fn []u8 @to_writebuffer() -> WriteBuffer {
    mut wb WriteBuffer = { buf: []u8.new(cap: @len()) }
    wb.buf.append(@)
    wb
}
```

Call-сайты `read_nav.nv`/`write_constructors.nv` мигрированы на
`bytes.to_readbuffer()`/`bytes.to_writebuffer()`.

Команда (ПОСЛЕ `touch external_registry.rs` + rebuild):
```
nova test --strict-effects spec_tests/conformance/read_nav.nv spec_tests/conformance/write_constructors.nv --verbose
```

Дословный результат (стабилен, воспроизведён 3+ раз подряд с одним и тем же
свежим бинарём):
```
CODEGEN-FAIL   spec_tests/conformance/read_nav  # ...std/src/runtime/string/core.nv:192:20:
error: [E7320] no field or method `ptr` on type `[]u8`
192 |     str.alloc_copy(@ptr(), @len())
  |                    ^^^^
...string/core.nv:192:28: error: [E7320] no field or method `len` on type `[]u8`
...string/core.nv:201:12: error: [E7320] no field or method `len` on type `[]u8`
```

Т.е. добавление `[]u8 @to_readbuffer()`/`@to_writebuffer()` (конкретный,
НЕ-generic extension-метод на `[]u8`, тело строит user-record `{ field: @ }`)
ломает резолв `@ptr()`/`@len()` (штатные Vec-аксессоры) **внутри СОВЕРШЕННО
другого файла** (`string/core.nv`, метод `[]u8 @to_str_unchecked()`, который
НИКАК не связан с ReadBuffer/WriteBuffer). Это НЕ то же сообщение, что в
историческом отчёте (`lld-link: undefined symbol
Nova_NovaArray_nova_int_method_to_readbuffer`) — по-видимому один и тот же
корень манифестирует по-разному в зависимости от привходящих деталей билда/
состояния компилятора (см. ниже про нестабильность), но ПРИНАДЛЕЖИТ той же
категории: расширение `[]u8`-фасада с record-телом ломает НЕСВЯЗАННУЮ типизацию
где-то ещё в том же compile unit.

Baseline (`git checkout -- read_buffer.nv write_buffer.nv read_nav.nv
write_constructors.nv`, ЖЕ бинарь) — 100% чисто: `RUN-FAIL` только с
ИЗВЕСТНЫМ несвязанным дефектом chained `.debug()`/`.display()`
(`Vec[f32]/Vec[int]` — задокументирован ранее, не относится к этой задаче).
Т.е. baseline звучен, регрессия строго от `[]u8`-extension-метода с
record-телом.

## Нестабильность симптома (важно для интерпретации)

С разными (но НЕ намеренно разными — просто последовательные rebuild БЕЗ
`touch`) бинарями ОДИН И ТОТ ЖЕ `.nv`-diff давал:
1. `[E_RECV_METHOD_MISMATCH] .read_u32_le(...) на ресивере типа HashMap`
   (single-key fallback резолвит имя в `ReadBuffer`, last-wins).
2. То же, но ресивер `WriteBuffer` (при чуть другом сочетании файлов —
   write_buffer.nv откачен, read_buffer.nv — нет).
3. `[E7320] no field or method ptr/len on []u8` (string/core.nv, ПОСЛЕ
   правильного `touch`+rebuild — это похоже НАИБОЛЕЕ стабильный/воспроизводимый
   вариант, 3/3 повторов идентичны с одним и тем же свежим бинарём).

Учитывая (3) стабилен при повторных ЗАПУСКАХ одного бинаря (не про
`HashMap`-рандомизацию хэшей — то было артефактом STALE-эмбеддеда из п.
«методология» выше, НЕ рантайм-рандомизация), а (1)/(2) были на
STALE-бинарях — доверять стоит **только (3)** как истинному симптому текущего
HEAD. (1)/(2) — отброшены как артефакты методологии.

## Расследованные и НЕ подтверждённые кандидаты корня

1. **`emit_c.rs` `is_array_ext` single-key регистрация методов** (~6476-6650,
   ~14065-14119, ~23987-24028, ~38358-38553): множество `is_array_ext =
   recv.type_name.starts_with("[]")`-гейтов обрабатывает КОНКРЕТНЫЕ
   `[]u8`-ресиверы ТАК ЖЕ, как generic `fn[T] []T`-методы (единый механизм,
   разный семантический смысл). `E_RECV_METHOD_MISMATCH`-гард (~38538-38553)
   ЯВНО исключает `[]`-префиксные ресиверы из safety-проверки («they
   legitimately dispatch by name here») — подозрительно, но НЕ подтверждено
   прямой связью с E7320 (тот — checker-side, до codegen).
2. **`types/mod.rs::check_instance_overload` `array_elem_key`** (~10689-10722,
   Plan 196.7-фикс): резолвит `[]u8`-фасад-методы через ELEMENT-спеллинг,
   ГЕЙТИТСЯ на `method_table.get("Vec").{None|!contains_key(method)}`. Прочитан
   построчно — по логике ДОЛЖЕН резолвить `to_readbuffer` корректно. НЕ
   воспроизведена связь с E7320 напрямую.
3. **`emit_c.rs::infer_expr_c_type` Channel 1/1b** (~52497-52564): Channel 1
   (`resolved_callees → fn_ret_by_span`) и Channel 1b (Plan 196.7 facade
   return-type twin, гейтится на bare-T blanket collision — `to_readbuffer` НЕ
   blanket, эта ветка не должна срабатывать). Frozen-зона `infer_call_ret_c`
   (46293-48883) — НЕ тронута, НЕ вскрыта как источник (не удалось
   подтвердить, что корень СТРОГО там).
4. **`sig_registry.rs::merge_module_fns`** (~198-227) — гейт «уже известен»
   был PER-TYPE (`known_types: HashSet<String>` снэпшот), а НЕ per-(type,
   method); `"[]u8"` — общий bucket между `string/core.nv` (to_str,
   to_str_unchecked — регистрируются штатно) И `read_buffer.nv`/
   `write_buffer.nv` (to_readbuffer/to_writebuffer — приходят ТОЛЬКО через
   `builtin_sig_modules()`, embedded `include_str!`, т.к. эти 3 типа
   «ABSENT from checker's module-built registry», см. комментарий в
   external_registry.rs:20-30 и types/mod.rs:3605). Гипотеза: per-type skip
   ронял ВСЮ embedded-регистрацию `[]u8` из read_buffer.nv (включая
   to_readbuffer), т.к. `"[]u8"` уже «known» от string/core.nv. **Применён
   точечный фикс** (per-(type,method) гейт вместо per-type) — **ПЕРЕПРОВЕРЕН,
   НЕ ПОМОГ** (тот же E7320 3/3, байт-в-байт). Фикс ОТКАЧЕН (0 diff к HEAD).
   Т.е. эта гипотеза ОПРОВЕРГНУТА эмпирически — either корень не здесь, либо
   есть ВТОРОЙ такой же per-type гейт где-то ещё (не найден).

## Рекомендация для следующей волны

- НЕ повторять чистую догадку-и-rebuild без `touch external_registry.rs` —
  тратит часы на ложные сигналы.
- Начать с #4 (per-type gate) шире: проверить ВСЕ 3 call-сайта
  `builtin_sig_modules()` (types/mod.rs:1716, :3612, :20168) и
  `merge_from_module`/`from_module` в `external_registry.rs` на тот же класс
  «per-type, не per-(type,method)» гейта — возможно, есть СИММЕТРИЧНЫЙ
  дубль-механизм (напр. `ExternalRegistry::merge_from_module`,
  external_registry.rs:266-286, `seen_types`/`receiver_types` — тоже
  ПО-ТИПОВОЙ, не по-методный список) который ТОЖЕ теряет `to_readbuffer`.
- Альтернативный маршрут: добавить eprintln-инструментацию (env-гейт, как
  `NOVA_DEBUG_STATIC`/`NOVA_CH2_TRACE` прецеденты) НЕПОСРЕДСТВЕННО в
  `check_instance_overload`'s `array_elem_key`-ветку (types/mod.rs~10689) —
  распечатать, находит ли она `to_readbuffer` в `method_table.get("[]u8")`
  ВООБЩЕ (до моего revert я НЕ успел добавить трассировку именно сюда —
  добавлял только в `emit_c.rs`, codegen-сторону, которая, возможно, никогда
  не достигается, если чекер уже падает с E7320 раньше).
- Рассмотреть: может ли `[]u8`-регистрация `to_readbuffer`/`to_writebuffer`
  вообще НЕ доходить до чекера НИКАК (ни через module.items, ни через
  builtin_sig_modules) — т.е. `read_buffer.nv`/`write_buffer.nv` ДЕЙСТВИТЕЛЬНО
  «ABSENT from module-built registry» ПОЛНОСТЬЮ (не только ТИПЫ, но и МЕТОДЫ)
  для ЭТОГО конкретного compile-unit (spec_tests.conformance), и
  `builtin_sig_modules()` — ЕДИНСТВЕННЫЙ канал, а мой per-method фикс просто
  НЕ ДОШЁЛ до реального прогона (напр. если MODULES OnceLock уже
  проинициализирован ДО правки — но `touch`+полный rebuild должен был это
  исключить; перепроверить отдельно unit-тестом на `SigRegistry` напрямую,
  без полного nova test).

## Не изменено (осталось в исходном виде)

- `std/src/runtime/read_buffer.nv:54` / `write_buffer.nv:60` —
  `nova:allow W_STATIC_CONVERSION` подавления НЕ сняты.
- Переименования `.from()` → `to_*()`/`into_*()` (владелец-директива:
  `to_readbuffer`/`to_writebuffer` — клонирующие, `into_readbuffer`/
  `into_writebuffer` — consume-захватывающие; см. запись директивы ниже) —
  НЕ применены к боевым файлам (только опробованы в этом чекпоинте, все
  правки откачены).
- Маркер `[M-static-conv-array-record-mono-cc-fail]` в
  `docs/plans/backlog-followups.md` — статус НЕ изменён (остаётся открытым,
  P2).

## Директива владельца (применить ПОСЛЕ фикса корня, зафиксирована для памяти)

Получена в середине волны, ДО обнаружения root-cause; относится к ФИНАЛЬНОЙ
форме API, когда фикс будет найден:

1. Клонирующая: `export fn []u8 @to_writebuffer() -> WriteBuffer => { buf:
   @.clone() }` (однострочник; симметрично `@to_readbuffer() -> ReadBuffer =>
   { data: @.clone(), pos: 0 }`). `Vec[T Clone] @clone()` уже существует
   (std/src/collections/vec/protocols.nv:117), `u8: Clone` — тривиально.
2. Захватывающая (consume, БЕЗ копии): `export fn []u8 consume
   @into_writebuffer() -> WriteBuffer => { buf: @ }` /
   `[]u8 consume @into_readbuffer() -> ReadBuffer => { data: @, pos: 0 }`.
   Прецедент живого `consume`-конверсии на `[]u8`: `[]u8 consume
   @into_str_unchecked() -> str` (std/src/runtime/string/core.nv:265) —
   ТОЧНО такой же синтаксис/семантика.
3. Аудит call-сайтов (сделан в рамках этой волны, до отката): ПОЧТИ ВСЕ
   реальные вызовы `ReadBuffer.from(bytes)`/`WriteBuffer.from(bytes)` в
   `spec_tests/conformance/*.nv` НЕ используют исходный `bytes` повторно
   после конструирования — то есть по факту все они кандидаты на
   `into_readbuffer()`/`into_writebuffer()` (consume), НЕ `to_*()` (clone).
   Проверено скриптом (awk-подсчёт упоминаний переменной `bytes` на тест-блок)
   по всем файлам: `read_nav.nv`, `read_char_str.nv`, `read_floats.nv`,
   `read_integers.nv`, `read_oob.nv`, `roundtrip.nv`, `write_constructors.nv`,
   `write_floats.nv`, `neg/d325_retracted_read_try_prefix_neg.nv`,
   `neg/neg_read_oob.nv`, `d325_result_everywhere.nv` — 0 повторных
   использований найдено. При миграции (после фикса корня) — использовать
   `into_*()` почти везде; `to_*()` оставить для симметрии API +
   гипотетических будущих call-сайтов, повторно использующих буфер (пин-
   фикстура должна покрыть ОБЕ формы явно).
4. Докстроки при миграции — явно «клонирует» / «захватывает без копии».

## Файлы (актуальный на конец волны, все ревёрчены к HEAD)

- `std/src/runtime/read_buffer.nv` — 0 diff (nova:allow остаётся).
- `std/src/runtime/write_buffer.nv` — 0 diff (nova:allow остаётся).
- `spec_tests/conformance/read_nav.nv` / `write_constructors.nv` — 0 diff.
- `compiler-codegen/src/sig_registry.rs` — 0 diff (пробный фикс откачен).
- `compiler-codegen/src/codegen/emit_c.rs` — 0 diff (debug-трассировка
  откачена).

## Коммиты этой волны

Нет — все правки экспериментальные, откачены до 0 diff перед завершением
(RED воспроизведён только временно, для диагностики; ни одного зелёного
состояния не достигнуто).
