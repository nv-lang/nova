<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# PROGRESS — p616-mode-modifiers

Бриф: `docs/plans/wip/brief-p616-mode-modifiers.md` (в главном дереве).
Дерево: `d:/Sources/nv-lang/nova-p255`, ветка `p616-mode-modifiers`.

## Статус: ЗАВЕРШЕНО, все коммиты сделаны

Все три задачи (№616 `-> consume T`, №611 постфикс параметра, №615
R2-split) реализованы, проверены точечно и закоммичены пятью коммитами
в порядке брифа: спека (`9b5b25533`) → компилятор (`f665823c2`) →
std-корпус (`4ae57086d`) → spec_tests-корпус+фикстуры (`baaa5dff0`) →
examples (`79ae493cb`). Рабочее дерево чистое. Остался только отчёт
владельцу.

## Что сделано (все пункты приёмки закрыты)

1. **Спека** — `spec/decisions/02-types.md`: D445 AMEND (2026-08-12) —
   убрана строка `-> consume T` из таблицы, амендмент 2026-07-17 к D374
   (AMEND ×3) отменён ПО ИМЕНИ новым AMEND ×4, R2-split-как-санкционированное
   снято. `docs/dev/nv-coding-style.md` §18б переписан под новый канон.
2. **Компилятор** — `compiler-codegen/src/parser/mod.rs`:
   - `-> consume T` (префикс) → `E_RETURN_CONSUME_PREFIX_RETRACTED` (новая).
   - `-> T consume` (постфикс, №301) → `E_RETURN_CONSUME_POSTFIX_RETRACTED`
     (существовала, текст сообщения обновлён под новую отмену).
   - постфикс `имя mut Тип` (голый И `ro имя mut Тип`) в параметре →
     `E_PARAM_TYPE_POS_MUT_RETRACTED` (новая), без exemption по типу.
   - Поле `Param::mut_type_pos_legacy` и лint `W_PARAM_TYPE_POS_MUT`
     (`lints.rs`) удалены целиком (недостижимы после хард-ошибки) — задело
     `ast/mod.rs`, `lints.rs`, `protocols/auto_derive.rs`,
     `const_fn_closure.rs`, `codegen/may_gc.rs` (структурные литералы
     `Param{..}`).
3. **std-корпус** — 5 носителей `-> consume T` в `std/src/runtime/sync.nv`
   переписаны на голый `-> T`; ~64 сигнатуры (67 вхождений) постфиксного
   `mut`-параметра в `std/src/{fs,io,net,identifiers,crypto,text}` —
   переписаны на префиксную форму. **ВАЖНО (найдено в процессе):**
   handler-ЛИТЕРАЛЫ (`effect X { op(...) { body } }`, НЕ декларации типа
   `type X effect {...}`) парсят параметры ЧЕРЕЗ ДРУГУЮ грамматику
   (`parse_handler_methods`, НЕ `parse_param`) — там `name mut Type`
   раньше не был «постфиксным legacy-параметром», а был `name`
   + type-level `Mut(Type)` модификатором (другой, НЕ ретрактированный
   механизм). Эти вхождения (в `std/src/{fs/fs.nv,fs/mock.nv,io/console.nv,
   net/mock.nv,net/tcp.nv}`, внутри `real_fs()`/`mock_fs()`/`real_io()`/
   `mock_io()`/`real_net()`/`mock_net()`) — НЕ ТРОГАТЬ, оставлены как есть.
4. **spec_tests-корпус**: `p176repro_*` (2 файла), `d172_*_park_neg.nv`
   (2 файла, return-consume carrier), `m465_zero_on_move_autoinject_pos.nv`,
   `standalone/m2211_38_*`, `standalone/m222_7_*` — переписаны. Удалены
   ДВА позитивных дискриминатора, чья предпосылка стала ложной:
   `d246_param_ro_mut_view.nv` (R2-split «легально») и
   `return_consume_prefix_canon_ok.nv` (`-> consume T` «канон»); их роль
   теперь у новых негативных фикстур.
5. **`examples/real_world/orm_decorators.nv`** — 1 сигнатура переписана.
6. **Фикстуры (новые)**:
   - `spec_tests/conformance/neg/return_consume_prefix_retracted_neg.nv`
   - `spec_tests/conformance/neg/param_type_pos_mut_retracted_neg.nv`
   - `spec_tests/conformance/neg/param_type_pos_mut_r2_split_retracted_neg.nv`
   - `spec_tests/conformance/d616_return_consume_retracted_linearity_kept.nv`
     (позитив — печатает `616`, D180-линейность жива без модификатора)
7. **Верификация:**
   - `nova check std/src` ДО и ПОСЛЕ — **PASS: 154 FAIL: 26 WARN: 62**
     ОБЕИХ строк дословно одинаковы (те же 26 файлов — намеренные neg/
     фикстуры не из этой волны). `W_PARAM_TYPE_POS_MUT` в std НИКОГДА не
     стрелял (все вхождения были slice/fixed-array — уже исключение до
     этой волны), поэтому WARN не падает численно, хотя лint снят целиком.
   - `nova test spec_tests/conformance --filter retracted --full` — все
     12 (включая 3 новых neg) PASS.
   - Позитив `d616...` — точечно верифицирован standalone-копией
     (`nova check` + `nova build` + запуск exe) — печатает `616`; ПРЯМОЙ
     `nova test <d616-файл>` внутри общего folder-module даёт
     `NEG-WRONG-STDOUT` из-за оркестраторного бага раннера НЕ связанного с
     этой волной (воспроизводится и на чужом pre-existing файле
     `p1_canonical_range.nv` той же командой — не регрессия этого окна).
   - Сабботаж (task 2/3, `E_PARAM_TYPE_POS_MUT_RETRACTED`): временно
     откачен фикс в parser/mod.rs → те же 2 фикстуры красные
     (`NEG-WRONG-MSG`, `FAIL: 2`) → фикс восстановлен → `PASS: 12 FAIL: 0`.

## Осталось сделать в ЭТОМ окне

- Закоммитить по частям (спека → компилятор → std-корпус →
  spec_tests-корпус → examples), греп конфликт-маркеров в одной команде с
  каждым коммитом.
- Отчёт владельцу по формату брифа (§ФОРМАТ ОТЧЁТА).
