# Checkpoint — план 234 (A-V12), окно p234-bitwise-2, sonnet

## Часть B (шаги 2-4) — СТОП, задокументировано, изменения НЕ внесены

Диагноз в шапке `int128.nv`'s `i64 @to_i128()` перепроверен ФАКТОМ (сборка
release + минимальные репро в scratchpad). Вывод: закрытие №129
(`mono_method_registry.rs::check_mono_method_decl_collision` — честная
ошибка при коллизии ОДНОЙ формы на одном ключе) НЕ покрывает этот класс —
это ДРУГОЙ дефект, дальше не закрыт.

Репро (сохранены в scratchpad, `p234_repro_blanket2.nv`/`p234_repro_blanket3.nv`):
- Бинарный вопрос "конкретный i64/u64/int @mk() + два type-set blanket'а
  (SignedInts/UnsignedInts) с ОДНИМ именем метода — какой побеждает?" —
  для БАРЕ-каста ресивера (`(7 as i8).mk()`, `(7 as i64).mk()`) дispatch
  КОРРЕКТЕН (конкретный метод побеждает на точном типе, blanket подхватывает
  узкие ширины) — этот класс закрыт накопленными фиксами канала.
- НО для ресивера вида `Unary{Neg}(Cast(...))` — то есть буквально
  `(-N as i64).mk()` (ИМЕННО та форма, что в оригинальном отчёте №149!) —
  dispatch ОШИБОЧНО уходит в blanket ВМЕСТО конкретного `i64 @mk()`:
  `(42 as i64).mk()` → 142 (концы, верно), `(-42 as i64).mk()` → -958
  (blanket-формула, НЕВЕРНО — должно быть 58).
- Эмпирически подтверждено на `nova test std/src/math` (int128_test.nv, до
  ревёрта): именно с добавленными двумя blanket'ами существующие тесты
  «умножение со знаками», «деление: усечение к нулю…», «abs», «to_str:
  базовое» — ВСЕ используют `(-N as i64).to_i128()` — упали RUN-FAIL.

Гипотеза (НЕ проверена трассировкой до конца — вне периметра этого окна):
`recv_c_type_materialized` (emit_c.rs ~56276) для НЕ-Ident receiver уходит
в `infer_expr_c_type(obj)`; для `Unary{Neg}` узла это может резолвиться
иначе, чем для голого `Cast`, из-за чего либо (а) конкретный-метод-первым
lookup выше по стеку emit_expr пропускает i64-конкретную перегрузку и
считает ресивер «кандидатом на blanket» (`recv_is_candidate`/
`has_typeset_blanket_for_primitive`), либо (б) сам чекер уже резолвит
`resolved_callees` для этого ExprId в blanket, а не в конкретный метод.
Не выяснено, чья это зона (канал types/mod.rs или codegen emit_c.rs) —
нужна отдельная трассировка.

Действие: изменения int128.nv/int128_test.nv (добавленные два blanket'а +
тесты) СДЕЛАНЫ и ОТКАЧЕНЫ (`git checkout --`), файлы в исходном состоянии.
Обход в int128.nv (`(x as u64).to_i128()`/`(x as i64).to_i128()` для узких
ширин) ОСТАЁТСЯ. Часть B шаги 2-4 остаются заблокированы новым, более узким
диагнозом (не общим №129-классом, а receiver-shape-специфичным dispatch
багом на Unary(Neg(Cast))). Отчёт интегратору — это НОВЫЙ диагноз, а не
подтверждение старого.

## Часть A — компилятор реализован, идут гейты (2026-07-31)

Реализовано и СОБИРАЕТСЯ (`cargo build --release` чисто, только baseline-
warnings), точечно проверено scratch-репро (build/run, не commit):

- **Ф.1** (rename `and`/`or` → `bitand`/`bitor`, добавлен `bitxor`):
  `emit_c.rs` ~34272 (self_method_decls generic-sum dispatch, был
  единственный сайт с "and"/"or"-строками — грепнуто по всему
  compiler-codegen, других нет). Плюс **найден и закрыт СМЕЖНЫЙ гэп**,
  без которого Ф.1 был бы неполон против образца @plus/@times:
  - `a & b`/`a | b`/`a ^ b` на ПЛОСКОМ (не-generic) heap-record/value-record
    типе НЕ дispatch'ились ВООБЩЕ (сырой C `&`/`|`/`^` над `Nova_T*` → CC-FAIL
    "invalid operands") — добавлен fast-path зеркальный @plus/@times
    (`is_single_nova_ptr`, emit_c.rs, прямо перед Sub-веткой).
  - Даже с dispatch-строкой корректной, ТЕЛО `@bitand`/`@bitor`/`@bitxor`
    НЕ эмитилось (reachability-DCE, Plan 159, `lints.rs::collect_used_names`)
    — бинарные операторы сидят "plus"/"minus"/"times"/"div"/"rem"/"equal"/
    "eq"/"compare"/"concat" как magic-selectors, НЕ видимые AST-обходом, но
    "bitand"/"bitor"/"bitxor" не были в списке. Добавлены (+ "bitnot" для
    Ф.2, унарный arm). Без обоих фиксов dispatch НИКОГДА не работал на
    обычных record-типах — только эмиссия строки, ведущей в необъявленный
    символ (undefined symbol на линковке).
- **Ф.2** (`~`): лексер-токен `Tilde` (свободен, как и предполагал план),
  парсер (`parse_unary`, тот же приоритет что `!`/унарный `-`), AST
  `UnOp::BitNot`, чекер (`infer_expr_type`/`infer_expr_c_type` — тип
  результата = тип операнда), codegen: примитивы — integer-promotion
  таблица (`nova_byte`/`uint16_t` → XOR-маска, остальные — голый `~`),
  пользовательские типы — @bitnot dispatch (тот же механизм, что @neg/@not,
  emit_c.rs ~34654). **Чекер-диагностика `E_UNARY_OPERAND_TYPE`** —
  единая для ВСЕГО унарного семейства `!`/`-`/`~` × неподдерживаемый тип
  (types/mod.rs, ExprKind::Unary walk) — покрывает весь neg-список гейта
  (~true/~1.5/~f32/~f64/~"str", -true/-"str", и "прочие" — добавлено !5
  для полноты). Тип без `@bitnot` под `~` — честная CC-FAIL от clang
  ("invalid argument type"), НЕ ICE компилятора (проверено).
- **Ф.2а** (compound `&= |= ^= <<= >>=`): 5 новых токенов лексера
  (`AmpEq`/`PipeEq`/`CaretEq`/`ShlEq`/`ShrEq`), `AssignOp` +5 вариантов,
  парсер, desugar в emit_c.rs — `&=`/`|=`/`^=` роутятся через ТОТ ЖЕ
  overloaded-dispatch, что `+=`/`-=` (переиспользует Ф.1 fast-path);
  `<<=`/`>>=` НЕ включены в overloaded-route (см. попутные находки ниже) —
  остаются raw C compound-assign (корректно для примитивов ЛЮБОЙ ширины —
  AND/OR/XOR/shift compound-assign НЕ имеют integer-promotion ловушки `~`,
  проверено рассуждением + пин-тестами на u8/i8).
- **Ф.3** (миграция std): ЕЩЁ НЕ СДЕЛАНО на момент этой записи — следующий шаг.

**Попутно найдено, НЕ чиню (вне периметра плана 234):**
- `<<`/`>>` (голые, не compound) на ПОЛЬЗОВАТЕЛЬСКОМ Nova_T*-типе не
  дispatch'ятся на `@shl`/`@shr` вообще (всегда сырой C `<<`/`>>`,
  независимо от типа receiver'а) — тот же класс дыры, что был у bitand/
  bitor/bitxor до этого окна, но для shl/shr никто ещё не заводил
  fast-path. Из-за этого `<<=`/`>>=` НЕ роутятся через overloaded-dispatch
  (см. Ф.2а выше) — на пользовательском типе они дадут ту же CC-FAIL, что
  и голый `<<`, а не честную ошибку раньше.
- Унарные `@neg`/`@not` (уже существовавшие ДО плана 234) имеют ТОЧНО ТАКОЙ
  ЖЕ reachability-DCE гэп, что был у bitand/bitor/bitxor — `collect_used_names`
  для `ExprKind::Unary` не сидит "neg"/"not" вовсе. `~`/"bitnot" я засеял
  (нужно для СВОЕЙ фичи), "neg"/"not" НЕ трогал (пред-существующий баг вне
  D46-амендмента этого плана; вероятно latent — на custom-типах `-x`/`!x`
  без явного `.neg()`/`.not()` где-то рядом в std сегодня, видимо, не
  бьёт — не проверял explicitly).

## ЗАВЕРШЕНО (2026-07-31) — все коммиты в ветке p234-bitwise-2, main НЕ трогал

Коммиты: d8bac6832 (Ф.1/Ф.2/Ф.2а компилятор), 197d25d19 (pos-фикстуры +
Set[T] rename), e3cbf83c1 (neg-фикстуры), 236459e0d (Ф.3 миграция std),
9701fcae0 (ratchet-рефактор bitwise_ops.rs).

Попутно найдено и МИГРИРОВАНО (не просто найдено): `std/src/collections/
set/core.nv` — `Set[T] @and`/`@or` жили под операторной перегрузкой D46
(`a & b`/`a | b` intersection/union) — план ошибочно считал int128.nv
ЕДИНСТВЕННЫМ потребителем старых имён; найдено мега-CU прогоном
(`plan123_chain_elem_p1_set_ops_iter_ok.nv`), переименовано в @bitand/@bitor
той же волной (иначе Ф.1 был бы регрессией для Set).

### Гейты (вердикты дословно)

- `cargo build --release --manifest-path nova-cli/Cargo.toml` → `Finished
  release profile [optimized] target(s)` — чисто, 58 warnings (baseline,
  без новых).
- Фикстуры pos (изолированные копии, test-build): `iso_m234_bitwise_rename_pos`
  → `PASS`; `iso_m234_bitnot_pos` → `PASS`; `iso_m234_compound_assign_pos`
  → `PASS`.
- Фикстуры neg (`nova check`, все шесть): `[E_UNARY_OPERAND_TYPE]` — ровно
  ожидаемая диагностика на каждом из `m234_bitnot_bool_neg`,
  `m234_bitnot_f64_neg`, `m234_bitnot_str_neg`, `m234_neg_bool_neg`,
  `m234_neg_str_neg`, `m234_not_int_neg`.
- `nova check std/src` → `PASS: 147  FAIL: 26  WARN: 60` — FAIL: 26
  ровно baseline, не сдвинулся.
- `nova test std/src/math std/src/checksums` → `PASS: 8  FAIL: 0` (int128
  δ0 — int128_test.nv PASS, включая упрощённый `@bitnot` тела через `~`).
  `std/src/crypto` — КАЖДЫЙ файл individually test-build PASS (md5_test,
  sha1_test, sha256_test, hmac_test, jwt_test); folder-batch `nova test
  std/src/crypto` падает `P67-LEGACY` ICE на `Timestamp.now()` в jwt.nv —
  ПРЕД-СУЩЕСТВУЮЩИЙ мега-CU/cross-file гэп, воспроизведён БЕЗ участия
  migrated-файлов (jwt_test.nv + hmac_test.nv individually PASS, комбо —
  крашится), НЕ регрессия этого окна.
- `nova test std/src/collections/set` → `PASS: 1  FAIL: 0` (Set-рename δ0).
- Флагман: `nova build examples/flagship/aggregator/src/main.nv
  --strict-effects` → `built:` (только пред-существующие warnings —
  new-then-cap лint, W_DEP_PATH_NO_RELEASE на внешних git-зависимостях,
  unused-import в main.nv — ни один не мой).
- `bash scripts/guards/arch-ratchet.sh` → **FAIL**: `lines=63849 >
  baseline=63807` (+42), `infer=349 > baseline=348` (+1). Ratchet
  baseline СВОЙ НЕ двигал (путь B — решение интегратора после личной
  проверки). Обоснование остатка: 5 новых dispatch-точек (2×Bit*-fast-path,
  1×compound-assign-route, 1×BitNot-custom-dispatch, 1×BitNot-primitive-
  emission) + 1 infer_expr_c_type-вызов (чтение ширины operand'а для `~`
  таблицы) — держал НОВУЮ логику максимально в отдельном модуле
  `codegen/bitwise_ops.rs` (mono_method_registry.rs/№129, assoc_ro.rs/№157
  паттерн), emit_c.rs получил только тонкие call-сайты; трижды сжимал
  комментарии (изначально +115/+1 → +77/+1 после экстракции модуля →
  +42/+1 после финальной чистки).
- `bash scripts/guards/check-marker-registry-sync.sh` → `ok: неучтённых
  0 <= baseline 0`.

### Попутно найдено, НЕ чиню (вне периметра плана 234) — сводка

1. `<<`/`>>` (голые) на пользовательском `Nova_T*`-типе не dispatch'ятся
   на `@shl`/`@shr` вовсе (тот же класс дыры, что чинил Ф.1 для Bit*, но
   для shl/shr никто fast-path не заводил) — из-за этого `<<=`/`>>=`
   (Ф.2а) на пользовательском типе не переиспользуют overloaded-route.
2. `@neg`/`@not` (пред-существующие) имеют ТОЧНО ТАКОЙ ЖЕ
   reachability-DCE гэп на плоских record-типах, что был у bitand/bitor/
   bitxor до этого окна (`collect_used_names` не сидит "neg"/"not" для
   `ExprKind::Unary` вовсе) — "bitnot" я засеял (нужно для своей фичи),
   "neg"/"not" НЕ трогал.
3. `int`/`uint` (wide-default) литеральный каст для значений, чей i64-
   bit-pattern отрицателен (например `18446744073709551610 as uint`),
   идёт через промежуточную `nova_int_to_uint`-конверсию, которая КЛАМПИТ
   отрицательное в 0 (не raw bit-reinterpret) — обнаружено при попытке
   написать пин-тест на `~(5 as uint)` против большого decimal-литерала;
   не относится к `~`, переписал фикстуру на XOR-с-all-ones вместо
   литерала. `to_str`/`println` для `u64`/`uint` со старшим битом тоже
   печатают как ЗНАКОВОЕ (`-6` вместо `18446744073709551610`).
4. Часть B (см. выше) — новый, более узкий диагноз receiver-shape-
   специфичного dispatch-бага (`Unary(Neg(Cast))` → неверно выбирает
   blanket вместо конкретного метода).
