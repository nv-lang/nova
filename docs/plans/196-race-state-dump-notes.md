<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# 196 — гуляющий рантайм-рейс (Plan 198 блокер): state-dump расследование (2026-07-13)

Worktree: `d:/Sources/nv-lang/nova-p196`, branch `race-state-dump`, base `09008b0fd`.
Model: sonnet. Метод: state-dump (см. `docs/cases/mn-race-stale-slot-2026-05.md`) +
`docs/debugging-races.md` playbook (`NOVA_DIAG_SEGV`/`segv_diag.c`).

## Статус: ЧАСТИЧНО ЗАКРЫТО — 3 реальных бага найдены и исправлены; ГЛАВНЫЙ
блокер (плавающий AV) ТОЧНО ЛОКАЛИЗОВАН (полная символизированная трасса), но
НЕ исправлен — фикс требует находки места, где резолвится метод `write_str` на
receiver'е типа `Write`.

## 1. Репро

Corpus: снимок `spec_tests/conformance` из `nova-198`, ветка `triage-198`,
коммит `f9622fb38` (1472 файла), скопирован в этот worktree коммитом
`957be00c9` (не мёржить в main как есть — это ЧУЖОЙ корпус, снятый для
локального репро; финальный корпус живёт в nova-198).

```sh
cd /d/Sources/nv-lang/nova-p196
export NOVA_GC_LIB_DIR=/d/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/lib
export NOVA_GC_INCLUDE_DIR=/d/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/include
export NOVA_INCLUDE_DIR=/d/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/include
nova-cli/target/release/nova.exe test --positive --timeout 300 ./spec_tests/conformance
```

**Частота: 100% (3/3 прогонов до фиксов, 3/3 прогонов ПОСЛЕ фиксов)** —
`app_effect_basic_t8_1` (представитель merged flat CU, ~1000 файлов / ~2500
test-блоков) падает RUN-FAIL каждый раз. `PASS: 114 FAIL: 4 SKIP: 12`
идентично во всех прогонах (агрегат НЕ изменился ни одним из трёх фиксов).

## 2. Три найденных и исправленных бага (коммиты `1f3650e57`, `8155802a8`)

Все три — реальные, независимо ценные баги; НЕ являются причиной главного AV
(см. §3), но чинить их стоило (нулевая толерантность к багам).

### 2.1 `_nova_fail_top` dangling после `with Fail = |e| interrupt` (emit_with)
`compiler-codegen/src/codegen/emit_c.rs` ~9929: `nova_fail_pop()` (single-level
pop) после interrupt-catch неверен, если body вызывал вложенные Fail-функции
(каждая пушит свой `NovaFailFrame`) — `nova_interrupt()` (effects.c) longjmp'ит
СРАЗУ к ближайшему `NovaInterruptFrame`, НЕ трогая `_nova_fail_top`, так что
вложенные pop'ы не выполняются, и `_nova_fail_top` остаётся висеть на мёртвом
стек-фрейме. Фикс: restore `_nova_fail_top = {ff}.prev` (снятый один раз при
push) вместо one-level pop — hard reset, идентичен старому pop в штатном
случае, self-healing в багованном.

### 2.2 `nova_runtime_reset()` не звался для обычных (non-panics) тест-блоков
`compiler-codegen/src/codegen/emit_c.rs` ~24106-24122 (else-ветка после
panics-clause if): `nova_runtime_reset()` (сброс `_nova_fail_top`/
`_nova_interrupt_top`/handler-слотов/`_nova_active_scope`/
`_nova_active_finalizer_stack`) звался ТОЛЬКО после panics-clause теста
(Plan 173 Ф.5 п.6). Обычный тест (plain assert) точно так же может оставить
это TLS-состояние грязным (тот же longjmp-мимо-эпилогов путь), и оно течёт
в СЛЕДУЮЩИЙ, несвязанный тест-блок в merged CU. Фикс: звать
`nova_runtime_reset()` безусловно после КАЖДОГО тест-блока.

### 2.3 Test-runner crash-detail: substring "panic" — тот же класс ложного
срабатывания, что уже чинили для "fail" в тот же день
`compiler-codegen/src/test_runner.rs` ~3045: `is_real_failure_line` матчила
`t.to_lowercase().contains("panic")` где угодно в строке — обычные
`PASS: ...` строки прозы про Fail/panic-эффект (например "compile to
Panic-class outcome", "runs without panic") ложно матчились, и RUN-FAIL
детали ВСЕГДА показывали одни и те же 4 нерелевантные строки независимо от
реальной точки краша (подтверждено: до фикса — детали про Plan 140.3/D325,
после фикса — про D229, т.е. реально РАЗНЫЕ, точные последние строки).
Фикс: матчить genuine panic-баннер `"panic: "` (effects.h: `nv_panic` пишет
именно эту строку) вместо substring где угодно.

**Этот фикс дал рабочий инструмент** — без него дальнейшая локализация (§3)
была бы невозможна: RUN-FAIL детали были misleading red herring на каждом
прогоне.

## 3. ГЛАВНЫЙ БЛОКЕР — точно локализован, НЕ исправлен

### Метод локализации
`docs/debugging-races.md` §3.1 — `NOVA_DIAG_SEGV=1` на собранном `.exe`
(`segv_diag.c`, VEH+dbghelp, уже вкомпилирован, просто не был документирован
в 196/198 notes ранее). Прямой прогон `.exe` (не через `nova test` — которая
пересобирает C каждый раз) дал ПОЛНУЮ символизированную трассу с ПЕРВОГО
прогона:

```
=== [SEGV-DIAG] EXCEPTION_ACCESS_VIOLATION ===
ExceptionCode:    0xC0000005
FaultAddress:     0x0000000000000028  (offset 0x28 внутри "объекта")

  #00 app_effect_basic_t8_1!Nova_Net_write+0x33                     (app_effect_basic_t8_1.c:6011)
  #01 app_effect_basic_t8_1!Nova_TcpStream_method_write_str+0x5B    (app_effect_basic_t8_1.c:39619)   ← KEYSTONE
  #02 app_effect_basic_t8_1!Nova_D229Point_method_debug+0x74        (app_effect_basic_t8_1.c:76019)
  #03 app_effect_basic_t8_1!nova_test__impl_Debug__auto_derive__memberwise__...D229__558+0x6F
  #04 app_effect_basic_t8_1!nova_test_chunk_8+0x5E19
  #05 app_effect_basic_t8_1!nova_fn_main_impl+0xA3
```

### Нарушенный инвариант
Сгенерированный C (`spec_tests/conformance/app_effect_basic_t8_1.c:76013-76021`):

```c
static nova_unit Nova_D229Point_method_debug(Nova_D229Point* nova_self, Nova_StringBuilder* w) {
    (void)(Nova_StringBuilder_method_write(w, _nova_strlit_cf430de39c7a4ec3));
    Nova_TcpStream_method_write_str(w, _nova_strlit_bf175e197f6a9965);   /* ← ДОЛЖЕН быть StringBuilder-side write_str */
    (void)(Nova_int_method_debug((nova_self->x), w));
    Nova_TcpStream_method_write_str(w, _nova_strlit_1d14a13d6d6fee32);   /* ← тот же баг, второй раз */
    (void)(Nova_int_method_debug((nova_self->y), w));
    (void)(Nova_StringBuilder_method_write(w, _nova_strlit_07c54b07b48e3666));
    return NOVA_UNIT;
}
```

`w` — параметр объявленный как `Write` (протокол), но `w`'s фактический
рантайм-объект — `Nova_StringBuilder*` (это же видно по сигнатуре и по
корректным вызовам `Nova_StringBuilder_method_write(w, ...)` В ТОЙ ЖЕ
функции!). ДВА вызова `w.write_str(literal)` (разделители полей записи)
эмитятся с ЧУЖИМ receiver-типом `Nova_TcpStream_method_write_str` вместо
`Nova_StringBuilder`-side write_str. `TcpStream.write_str` читает поле по
смещению внутри своей структуры, интерпретируя `Nova_StringBuilder*` как
`Nova_TcpStream*` → чтение offset 0x28 у объекта, у которого там либо не то
поле, либо за пределами аллокации → READ AV.

Тот же паттерн — в `Nova_D229Named_method_debug` (соседняя auto-derived
Debug-функция, тот же файл, ~строка 76003-76011): идентичный
`Nova_TcpStream_method_write_str` вместо правильного вызова. Это
СИСТЕМАТИЧЕСКИЙ баг auto-derive Debug-кодогенерации (`protocols/auto_derive.rs`
`synth_debug_record_body`/`synth_display_record_body`, ~строки 1041-1111),
НЕ баг конкретного теста.

### Источник (первопричина НЕ до конца прижата — план ниже)
1. `protocols/auto_derive.rs` синтезирует AST-вызов `w.write_str(literal)`,
   где `w: Write` (протокол) — САМ ПО СЕБЕ корректен, обычный method-call AST.
2. `type_ref_to_c` (`codegen/emit_c.rs:3924`) хардкодит `"Write" =>
   "Nova_StringBuilder*"` — так СИГНАТУРА функции получает правильный
   `Nova_StringBuilder* w`.
3. `extract_protocol_type_name` (`codegen/emit_c.rs:8320-8333`) ЯВНО
   исключает `"Write"` из generic protocol-erasure путей (комментарий:
   "must NOT be treated as an erased protocol variable").
4. Несмотря на (2)+(3) — на call-site `w.write_str(...)` РЕЗОЛВ метода
   (какую `Nova_<Type>_method_write_str` звать) выбирает `TcpStream`, а НЕ
   `StringBuilder`, и ТОЛЬКО в большом merged CU (в изолированном/малом
   прогоне тот же тест, по показаниям nova-198's investigation, PASS —
   т.е. поведение composition-зависимо, СИМПТОМ СОВПАДАЕТ с «плавающей
   точкой краша» из диагноза).
5. Где именно теряется/подменяется receiver-тип на call-site — НЕ найдено
   в отведённое время. Проверено и ИСКЛЮЧЕНО:
   - `type_ref_to_c("Write")` — безусловный, не кэширующий match arm,
     не может дать другого результата.
   - Обычный registry-driven dispatch (`external_registry.lookup(recv_ty,
     method)`, emit_c.rs ~33977-34001) КЛЮЧУЕТСЯ по `recv_ty`, вычисленному
     ИЗ `obj_ty` (C-тип receiver'а) — если `obj_ty` уже "Nova_TcpStream*",
     то дальнейший lookup КОРРЕКТНО находит TcpStream's write_str (т.е. баг
     — В ТОМ, ЧТО ДАЁТ `obj_ty` для идентификатора `w` внутри ЭТОЙ
     конкретной синтезированной функции, а не в candidate-selection).
   - Простое «коллизия имени параметра» ОПРОВЕРГНУТО: `TcpStream.write_str`
     объявлен с параметром `s`, не `w` (`std/src/net/tcp.nv:208`) — значит
     утечка НЕ через `var_types["w"]` от чужой функции с тем же именем
     параметра впрямую.

### План фикса (для следующей волны — НЕ архитектурный, но требует находки
точного места)
1. Найти, где `infer_expr_c_type`/эквивалент для `ExprKind::Ident("w")`
   ВНУТРИ auto-derived debug/display метода получает C-тип receiver'а перед
   method-call codegen (вероятно НЕ через `var_types` по имени параметра
   напрямую, а через какой-то resolved/checker-driven путь для
   protocol-typed параметров — возможно related к duck-typing/structural
   резолву протокола `Write` по СПИСКУ типов, реализующих метод с нужной
   сигнатурой, где список НЕ детерминирован/не отфильтрован по ИМЕННО
   этому параметру, а даёт "любой implementor" — HashMap/Vec order-
   dependent, тот же класс, что уже чинили дважды в этом файле [§3a/3b
   Plan 196 notes]).
2. Проверить: есть ли отдельный путь для method call, когда receiver's
   STATIC (AST-declared) тип — protocol-имя, НЕ являющееся `self.types`
   (конкретным type_decl) — вероятно там ищется "который тип реализует
   метод write_str" среди ВСЕХ зарегистрированных типов вместо того, чтобы
   зафорсить `StringBuilder` (как это уже сделано для C-типа сигнатуры).
3. Минимальный детерминированный репро для дальнейшей атаки: ЛЮБОЙ record-
   тип с `@debug`/`@display`-дериватом + std.net (TcpStream.write_str) ИЛИ
   `std.runtime.write_buffer` (WriteBuffer.write_str) в ОДНОМ CU. Кандидат
   на минимальный файл-пара: `spec_tests/conformance/d229_*.nv` (любой файл
   объявляющий `D229Point`/`D229Named` с `#[derive(Debug)]`-подобной
   аннотацией) + любой файл, использующий `TcpStream.write_str` (или просто
   `import std.net` где регистрируется TcpStream's write_str) — собрать их
   ВДВОЁМ как folder-module и посмотреть, воспроизводится ли (не проверено
   в отведённое время — экономия по эффорту).
4. После находки точного места — вероятный фикс: при резолве
   `w.write_str(...)` где `w`'s ДЕКЛАРИРОВАННЫЙ (AST) тип — protocol-имя
   `Write`, форсить `recv_ty = "StringBuilder"` НАПРЯМУЮ (симметрично тому,
   что `type_ref_to_c` уже делает для сигнатуры), вместо любого
   generic/duck-typed поиска "кто реализует write_str".

## 4. Гейт (честный отчёт, не закрыт)

- `spec_tests/conformance` merged-CU (`app_effect_basic_t8_1`): **3/3
  прогона AV, ДО и ПОСЛЕ трёх фиксов** (§2). Частота НЕ изменилась —
  100% → 100%. Фиксы §2 валидны и оставлены (нулевая толерантность к
  багам — каждый исправлен там, где найден), но НЕ являются причиной
  этого AV.
- `spec_tests/conformance` conformance ПОЛНЫЙ (официальный гейт, без
  `--jobs`, `--positive --compile-error`, база 113/0+7skip) — **НЕ
  прогнан** в этой сессии (фокус был на 198-corpus репро; штатный
  conformance у main НЕ содержит этот merged-CU snapshot, так что
  штатный гейт вероятно не задет этими тремя фиксами напрямую, но
  ДОЛЖЕН быть прогнан перед мёржем §2 фиксов в main).
- `std/src/concurrency` δ — не проверено в этой сессии (фиксы не
  затрагивают concurrency-код напрямую — оба фикса в
  `codegen/emit_c.rs`/`test_runner.rs`, не в `nova_rt/`).

## 5. Коммиты этой сессии

- `957be00c9` — repro corpus snapshot (не для мёржа как есть)
- `1f3650e57` — fail_top restore + runtime_reset unconditional
- `8155802a8` — crash-detail panic-substring fix

Все три — в ветке `race-state-dump` этого worktree, main НЕ трогали.
