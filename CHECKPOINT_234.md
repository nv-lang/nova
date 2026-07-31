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

Дальше по плану: фикстуры m234_* pos/neg (spec_tests/conformance +
conformance/neg/), Ф.3 миграция std (9 сайтов), гейты (nova check std/src,
nova test std/src/math int128 δ0, флагман --strict-effects, ratchet,
marker-sync).
