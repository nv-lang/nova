# pk2-fix: окно К2 №36/№39/№126/№166 — отчёт

Модель: **sonnet**. Worktree: `d:/Sources/nv-lang/nova-pk2fix` (ветка `pk2-fix`
от `main`, коммит `bf5fa82e3`). Порядок по заданию: №39 → №166 → №36 → №126
(диагноз/фикс делались в этом порядке; №166 остановлен рано, поэтому дальше
шли №36/№126 полными фиксами, отчёт группирует по номеру для читаемости).

Коммит фикса: `d4bc03469` (`fix(221.1, pk2-fix): №39/№36/№126 закрыты в окне К2`).

## Таблица вердиктов

| № | Вердикт | Фикс |
|---|---|---|
| 39 | ✅ ЗАКРЫТ | checker-registry (emit_c.rs, `debt_mangled_has_nested_placeholder`) |
| 36 | ✅ ЗАКРЫТ | emit-side type-decay (emit_c.rs, `Stmt::Let` unannotated-binding path) |
| 126 | ✅ ЗАКРЫТ | ДВА фикса: checker-канал (types/mod.rs, новый продюсер) + codegen-dispatch (emit_c.rs, расширенный TurboFish-recursion-арм) |
| 166 | ⛔ НЕ ЗАКРЫТ | точный диагноз ниже, стоп — корень системный (`self.types` не module-scoped) |
| 19 | ⛔ НЕ ЗАКРЫТ (тот же корень, что №166) | см. №166 |

---

## №39 — `[M-effect-generic-value-record-mono]` — ЗАКРЫТ

**Корень.** `debt_mangled_has_nested_placeholder` (emit_c.rs, использует
`drain_generic_type_worklist`'s Stage-2 nested-placeholder skip) проверяет
каждый `Nova_<token>`-сегмент мангл-имени против `record_schemas`/
`sum_schemas`/`generic_types`/`opaque_ffi_types`/примитивов, чтобы отличить
«реальный конкретный тип» от «нерешённый type-param placeholder». Реестр
`effect_schemas` в этом OR-списке отсутствовал. Эффект-тип-аргумент
(`Handler[Db]`'s `Db`) лежит в мангл-имени как `Nova_<Eff>_p` (heap-указатель
на диспетч-vtable) — НИ в одном из проверенных реестров не значится →
классифицировался как placeholder → `drain_generic_type_worklist` тихо
пропускал эмиссию mono-структуры (`NovaValue_<Name>____Nova_<Eff>_p` так и
не эмитился) → downstream ссылки на этот тип давали "unknown type name".

Пробой сужено верно: условие — «generic value-record инстанцирован ЛЮБЫМ
эффект-типовым параметром», не «два эффект-инстанса в одном CU» (второй
инстанс для репро не обязателен).

**Фикс.** Один `&& !self.effect_schemas.contains_key(tok)` в OR-цепочке
`debt_mangled_has_nested_placeholder` (emit_c.rs). Регистро-полнотный фикс,
не выдумывание новой типовой информации — эффект-типы УЖЕ зарегистрированы
в `effect_schemas`, просто не проверялись в этом ОДНОМ месте.

**Фикстура.** `spec_tests/conformance/d277_generic_value_record_mono.nv`
("D277 Stage 3") — расширен существующий D277-файл (не новый CU): один и
два эффект-инстанса generic value-record в одном CU.

**Вердикты (дословно):**
```
=== p39_effect_generic_value ===
built: ...\p39_effect_generic_value\main.exe
db
log
exit=0
=== p39b_single_effect_generic ===
built: ...\p39b_single_effect_generic\main.exe
db
exit=0
=== p39c_plain_type_generic ===
built: ...\p39c_plain_type_generic\main.exe
plain
exit=0
```

---

## №36 — `[M-fluent-value-init-local-deref]` — ЗАКРЫТ

**Корень.** `Stmt::Let` (emit_c.rs) для БЕЗАННОТАЦИОННОГО `mut`/`ro`
(`decl.ty.is_none()`) берёт C-тип локала через `infer_expr_c_type(&decl.value)`.
Для RHS = вызов fluent-сеттера на value-типе (`mut out = resp.body(gz)`,
`mut @body(data) -> @`) это возвращает СЫРОЙ эмитированный C-возврат метода —
`NovaValue_X*` (`ref Self`, D117/Plan 184 ABI) — потому что это ФАКТ уровня
C для fluent-сеттера. Но Nova-уровневый тип биндинга — VALUE `X`, не
указатель. Существующий предикат `is_fluent_value_ptr_for_target` (уже
использовался для explicit-annotation let/return/argpos-консьюмеров, семья
D326/Plan 184 Р5-Р7) в конце того же `Stmt::Let`-хендлера ДОЛЖЕН был
дореференсировать RHS под аннотированный `ty_c` — но для безаннотационного
случая `ty_c` УЖЕ был указателем (`NovaValue_X*`), и гейт
`is_value_struct_val` (требует НЕ-указатель) отбрасывал случай без
дереференса. Локал объявлялся pointer-typed. Симптом проявлялся ПОЗЖЕ, когда
локал читался bare-Ident'ом в value-позиции (например `return out`,
implicit trailing return) — там RHS уже не `Call`, а plain `Ident`, и та же
проверка не срабатывает (не находит fluent-call AST-узел).

Это ПЯТЫЙ непокрытый deref-сайт семьи №9/№10 (окно-блокеров чинило 4) — из
реестра, диагноз пробоя подтверждён точно.

**Фикс.** В ветке `decl.ty.is_none()` — если инференс дал
`is_value_struct_ptr` И `is_fluent_value_ptr_for_target(&decl.value,
<база_без_*>)` истинно, `ty_c` decay'ится к value-форме (`.trim_end_matches('*')`).
Это ПЕРЕЗАПУСКАЕТ существующий deref-чек чуть ниже по тому же
`Stmt::Let`-хендлеру (он теперь видит value-typed `ty_c` и корректно
дереференсирует RHS) — ОДНА точка правды (`is_fluent_value_ptr_for_target`),
не второй ad-hoc deref-сайт.

**Фикстура.** `spec_tests/conformance/d326_fluent_value_let.nv` — новый файл
семьи D326 (Stage «Let»), закрывает ИМЕННО двухшаговую форму (bind →
LATER read as ident), которую соседние D326-файлы (return/argpos/stmt) не
покрывали — они консьюмят fluent-вызов НАПРЯМУЮ в своей позиции.

**Вердикты (дословно):**
```
=== p36_fluent_deref ===
built: ...\p36_fluent_deref\main.exe
working split ok, body.len=3
failing fluent ok, body.len=3
exit=0
=== p36b_working_only ===
built: ...\p36b_working_only\main.exe
working split ok, body.len=3
exit=0
```

---

## №126 — `[M-static-generic-method-path-call-p67-panic]` — ЗАКРЫТ

**Корень — ДВА независимых гэпа**, оба нужны для полного закрытия.

**(1) Checker-канал.** `Type.method[T](args)` — турбофиш на ИМЕНИ МЕТОДА
(метод-собственный generic, не carrier-generic типа). Ни один существующий
продюсер не покрывал: `resolve_instance_method_return_arity` жёстко бэйлит
на `ReceiverKind::Static` (только instance-ресиверы — by design); сиблинг
`resolve_generic_static_return` покрывает ТОЛЬКО турбофиш на СОБСТВЕННЫХ
carrier-дженериках типа-ресивера (`Type[T].ctor()` — другая AST-форма,
`Member{obj:TurboFish}` вместо нашей `TurboFish{base:Member/Path}`).
`resolved_types`/`resolved_callees` для call-узла не писались вовсе →
codegen's legacy `infer_call_ret_c` не находил ни одного bucket'а →
безусловная `[P67-LEGACY]` паника.

Новый продюсер `resolve_generic_static_method_own_return` (types/mod.rs,
сиблинг `resolve_generic_static_return`, но по методным `f.generics`, не
`recv.generics`) + вызов из `infer_method_call_channel_type` (общий top-level
чек на `TurboFish{base}`, где `base` — `Member{obj:Ident}` ИЛИ
`Path(len==2)`, обе — bare-имя-типа-в-obj/parts[0], не значение в scope).
Сужен НАРОЧНО на `recv.generics.is_empty()` (тип без своих carrier-дженериков)
— двойной турбофиш (`Type[G].method[M]`) — другая, необработанная форма,
честно оставлена legacy.

**(2) Codegen dispatch (отдельный гэп, найден ПОСЛЕ фикса (1) — линкер-ошибка
`undefined symbol: Nzp126Utils_show` вместо ICE).** `Type.method[T](x)` на
PascalCase-имени типа парсится parser'ом (`starts_uppercase`-петля,
parser/mod.rs ~9115) в `TurboFish{base: Path([tyname, method])}`, НЕ
`TurboFish{base: Member{...}}` — точка складывается в 2-сегментный Path ДО
того, как postfix-стадия успевает построить `Member`-узел. Существующий
`[M-91.1-method-turbofish-dispatch]`-рекурсивный арм в `emit_call` (стешит
`current_method_turbofish`, рекурсирует на `base`) матчил ТОЛЬКО
`base:Member` — для `base:Path` турбофиш просто терялся, call падал в
naive-фоллбек, эмитивший несуществующий немангленный символ.

**Фикс.** Расширить guard существующего recursion-арма:
`matches!(base.kind, ExprKind::Member{..}) || matches!(&base.kind,
ExprKind::Path(p) if p.len()==2)`. Рекурсия на bare Path-call с ТЕМ ЖЕ
`call_id` — downstream `resolve_method_level_subst` уже читает
`node_substs[call_id]` channel-first (тот самый, что продюсер (1) заполнил)
— новой mono/dispatch-логики не потребовалось, арм был просто узок.

**Фикстура.** `spec_tests/conformance/m126_static_generic_method_path_turbofish.nv`
— implicit-Unit возврат (форма пробоя) + explicit `-> ro T` возврат
(другая ветка продюсера) + сиблинг instance-turbofish форма как regression-guard.

**Вердикты (дословно):**
```
=== p126_static_generic_path ===
built: ...\p126_static_generic_path\main.exe
show = 42
exit=0
=== p126b_instance_generic_ok ===
built: ...\p126b_instance_generic_ok\main.exe
show = 42
exit=0
```

---

## №166 / №19 — `[M-io-write-all-tcpstream-mono-cc-fail]` — НЕ ЗАКРЫТ, честный стоп

**Репро сужено до минимума** (НЕ зависит от net/TcpStream — воспроизводится
ЛЮБЫМ user-типом, реализующим `std.io.Write`):
```nova
import std.io
type P166Sink value { priv count int }
fn P166Sink mut @write(data []u8) -> Result[int, io.IoError] { ... Ok(data.len()) }
fn P166Sink mut @flush() -> Result[(), io.IoError] => Ok(())
fn main() { mut s = P166Sink.new(); io.write_all(s, data) ... }
```
CC-FAIL: `initializing 'nova_unit' with an expression of incompatible type
'NovaRes_nova_int_NovaValue_IoError *'` — байт-в-байт та же ошибка, что и
`std/src/net/addr` (№166) и вытесненный №19.

**Корень.** `std.io.write_all[W Write](mut w W, data []u8)`'s тело:
`match w.write(rest) { ... }`. `w.write(rest)`'s return type резолвится
через `resolve_generic_bound_receiver_method` (types/mod.rs) — единственный
продюсер для ВЫЗОВА МЕТОДА на bare protocol-bound generic receiver'е (`w: W`,
без конкретной моно-инстанциации на момент чек-прохода — checker типизирует
generic-тело ОДИН раз, генерически). Эта функция берёт ПЕРВЫЙ bound-протокол
и резолвит его метод через `self.types.get(bpath[0])` — ГЛОБАЛЬНЫЙ,
плоский, НЕ module-scoped реестр типов ("same-name declarations onto one
slot, last `module.items` write wins" — комментарий у `types_get_for_file`,
4535).

В стандартной библиотеке РОВНО ДВЕ декларации `export type Write protocol`
с ИДЕНТИЧНЫМ именем:
- `std/src/prelude/protocols.nv:165` — текстовый sink (`@display`/Debug-
  форматирование), `mut @write(bytes []u8) -> ()`;
- `std/src/io/core.nv:50` — байтовый sink, `mut @write(data []u8) ->
  Result[int, IoError]` (ИМЕННО этот протокол в bound'е `write_all[W Write]`,
  тот же файл/модуль — same-module reference).

`self.types.get("Write")` возвращает ОДНУ запись независимо от того, ИЗ
КАКОГО ФАЙЛА вызван lookup — подтверждено эмпирически: даже вызов ИЗНУТРИ
`std/src/io/core.nv` (где объявлен СВОЙ "Write") получает ПРЕЛЮДНУЮ версию
(`() `-возврат совпадает с наблюдаемым `nova_unit`). Минимальный изолированный
репро (только `import std.io`, без net) воспроизводит СТО ИЗ СТА, не
завязан на порядок файлов конкретного CU.

Это не узкий баг одного вызова — это ТОТ ЖЕ документированный СИСТЕМНЫЙ
класс `[M-172.1-var-types-cu-name-leak]` («резолвер пока держит один
name-keyed namespace на CU»), упомянутый в test-conventions.md (http/
ErrorKind-коллизия, аналогичный workaround «временно живёт в nova_tests
до Plan 182») и в самом коде (`types_get_for_file`'s doc, `[M-198-f4c-1-
privfile-type-not-discriminated]`) как известный, ПРИНЯТЫЙ архитектурный
гэп с отложенным полным решением.

**Почему стоп, а не узкий фикс.** Механизм `types_get_for_file` (уже
существует, module/file-aware lookup) СУЩЕСТВУЕТ, но `file_local_types`
(его источник) заполняется ТОЛЬКО для `priv(file)`-коллизий — узкий частный
случай. `Write`/`Write` — оба ПУБЛИЧНЫЕ (`export`) декларации в РАЗНЫХ
модулях — это ДРУГОЙ, более общий класс коллизии (cross-module same-name
EXPORTED type), для которого module-scoped resolution ПРОСТО НЕ СУЩЕСТВУЕТ
нигде в чекере — `self.types` в принципе не хранит, из какого модуля
пришла КАЖДАЯ запись, только имя→декларация. Узкий фикс ТОЛЬКО для
`resolve_generic_bound_receiver_method` потребовал бы либо (a) провести
через ВЕСЬ чекер понятие «текущий модуль вызывающей стороны» и «в каком
модуле объявлен каждый `self.types`-элемент» (не существует), либо (b)
угадывать эвристикой («тот, что объявлен в том же файле, что enclosing fn»)
— рискованно (false-positive на легитимных cross-module bound'ах) и не
проверяемо в отведённом окне без риска тихо сломать другой резолв. Полное,
корректное решение — module-qualified `self.types` (или аналог) — это
задача калибра отдельного плана (Plan 182 уже упомянут как владелец этого
класса), не точечный К2-фикс.

**Рекомендация.** №166/№19 остаются в очереди как ОДНА запись (№19 —
дубликат/вытеснённая формулировка №166, подтверждено дважды: пробоем и этим
окном). Настоящее закрытие ждёт module-scoped type resolution (Plan 182 или
преемник) — тогда `resolve_generic_bound_receiver_method`'s `self.types.get`
естественно становится `types_get_for_file`-подобным с ПОЛНЫМ (не только
priv(file)) охватом.

---

## №TBD-находки

Нет новых находок за пределами реестровых номеров — оба гэпа №126 (checker
+ codegen) и уточнение диагноза №39/№166 УЖЕ покрыты существующими номерами
реестра (записаны выше как под-пункты (1)/(2), не как отдельные №TBD).

---

## Проверка (сводно)

- Пробойные фикстуры (7 файлов, №36×2, №39×3, №126×2) — все PASS дословно
  выше.
- `spec_tests/conformance` (дефолт-лейн, включает мега-CU
  `a_q3_println_debug_record` = ОДИН compile-unit со всеми pier-тестами,
  включая три новые/расширенные фикстуры этого окна): **163 PASS / 0 FAIL /
  578 SKIP** (два независимых прогона после каждого фикса, оба чистые).
- `nova check std/src`: **148 PASS / 26 FAIL / 61 WARN** — байт-в-байт
  совпадает с заданным окну baseline (26 FAIL — известные, не мои).
- `arch-ratchet.sh`: `infer=348<=348` (чисто); `lines=64172` — **+1 строка
  над baseline 64171** (не абсолютный ноль). Все три codegen-фикса
  скомпактированы до предела (однострочные условия через `||`/`&&`, inline
  `/* */`-комментарии вместо отдельных строк) — минимум, достижимый БЕЗ
  посторонней (не относящейся к фиксам) чистки соседнего кода ради
  зануления метрики. Честно зафиксировано, не подмена baseline.
- `cargo build --release` (`compiler-codegen` + `nova-cli`) — чисто, без
  новых ошибок (только пред-существующие warnings, не мои).
- Git: коммит `d4bc03469` в ветке `pk2-fix`, НЕ запушен, НЕ смёржен в
  `main` — интеграция/гейт мега-CU (672/0/69) и флагман — вне мандата этого
  окна (принадлежит интегратору по инструкции задания).

## Модель

sonnet (единственная модель этого окна, фонов/суб-агентов не спавнилось).
