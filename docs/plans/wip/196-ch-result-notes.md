<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 196 — Зона CH (продюсер резолва): Result/Option канал, чекпойнт

**Worktree:** `nova-196ch`, ветка `p196-ch-result-producer`. **База:** main `b257d1f22`.
**Модель:** opus. **Зона:** `compiler-codegen/src/types/mod.rs` (чекер/продюсер) — emit_c НЕ тронут.

---

## Итог одной строкой

Ключевая находка ПО КОДУ+ТРЕЙСУ: продюсер `resolved_types[call.id]` для **generic free-fn /
static-ctor Result/Option-ВОЗВРАТА уже доставлен** прежней волной 196.5 (producers-widen,
`[M-172.1-U4-freefn-generic-return]` + `[M-196.5 producers-widen Class-1/2]`, `f1_check_call`
~11588–11757). Для этого класса легаси `infer_call_ret_c` **НЕ достигается** (ICR-трейс пуст на
d85/d85_payload; Channel-2 `resolved_types` несёт конкретный `Result[int,str]`, Channel-1
`fn_ret_by_span` для generic-возврата гейтится `debt_is_generic_stub_c`). Карта волны-2 (196.3
«:10478 skip generic-возврат») **устарела** — гард снят до меня. Остаточный продюсер-gap класса
«ctor» — **node_substs для static-generic-ctor** (тип-параметры на РЕСИВЕРЕ, `callee.generics`
пуст) — закрыт этой волной byte-parity-safe.

## 1. Диагностика (трейс-доказательство состояния ДО)

Инструмент: `nova-codegen compile <fixture> -o x.c` (standalone, только builtin-prelude) +
`NOVA_TRACE_ICR=1` (какие легаси-бакеты `infer_call_ret_c` достигаются) +
`NOVA_NODE_SUBSTS_TRACE=1` (какие call-сайты продюсер аннотирует).

ICR-бакеты (легаси-возврат достигнут) на gate-фикстурах:
- `d85_question_return`: **пусто** (полностью канализировано).
- `d85_result_payload_width`: **пусто**.
- `d30_try_op_unwrap_pair`: `B10f_user_fn_sigs`, `B11d_typed_pointer_methods`,
  `B11r_result_like_methods` — METHOD-класс (`.is_ok()`/`.ok()` на Result-объекте) + non-generic.
- `d408_option_chain_sized_width`: `B11q_novaopt_methods` — METHOD-класс (`.map`/`.unwrap_or`).

Вывод: остаточные легаси-хиты gate-фикстур — **METHOD-generic класс** (вне ПЕРИМЕТРА этой волны,
«вне method-generic-класса») + non-generic. Free-fn/ctor Result/Option-ВОЗВРАТ легаси НЕ трогает.

Минимальный репро (generic free-fn `wrap_ok`/`wrap_some`/`id_result` + static-ctor
`Box[T].try_make -> Result[Box[T],str]` / `Box[T].maybe -> Option[Box[T]]`): продюсер=A пишет
`node_substs` для free-fn (n=1/1/2), но **НЕ для static-ctor** — потому что старый гейт строил
`ordered` и денумератор полноты ТОЛЬКО по `callee.generics` (пусто, когда `T` на ресивере).
`resolved_types` для static-ctor при этом ПИШЕТСЯ (ветка `!ret_is_concrete`, ~11711 — конкретный
`Result[Box[int],str]`), т.е. gap был именно в позиционном subst-канале node_substs.

## 2. Что доставил продюсер (файл:функция)

`compiler-codegen/src/types/mod.rs`, `f1_check_call` (node_substs-writer ~11721–11780),
маркер `[M-196-ch-static-ctor-node-substs]`:

- Раньше: `ordered` итерировался по `callee.generics`, гейт `!callee.generics.is_empty() &&
  ordered.len()==callee.generics.len()`. Static-generic-ctor (`fn Box[T].make(...)`, вызов
  `Box.make(v)`) несёт тип-параметры на РЕСИВЕРЕ → `callee.generics` ПУСТ → node_substs НЕ
  писался для всего класса, хотя `subst` (arg-unify выше) полностью связал `T`.
- Теперь: строится `gen_names` = `receiver.generics` (в порядке декларации) ++ `callee.generics`
  (dedup), `ordered`/гейт полноты — по `gen_names`. `callee_gs_inner` УЖЕ содержал оба (через
  `fn_generic_scope`) — правка чинит только ПОРЯДОК + знаменатель полноты. Для free-fn (нет
  ресивера) `gen_names == callee.generics` → байт-идентично.

**Additive + byte-parity-safe:** потребитель codegen `rt_slots_from_args` ключует node_substs ПО
ИМЕНИ, свежая запись = channel-HIT под per-key byte-identity гардом (`subst_map_adopt_rt`) с
MISS-fallback на легаси; debug-`shadow_check_node_substs` ассертит, что канал лоуэрится в ТУ ЖЕ
C-строку, что легаси.

## 3. Byte-parity доказательство (диф .c ДО/ПОСЛЕ)

Цикл: сохранил правку → `git checkout` (baseline) → сборка baseline-бинаря → генерация baseline
.c → восстановил правку → пересборка → генерация .c → диф. 15 фикстур (4 gate +
`d30_result_option_ret_generic`[16 node_substs-хитов] + `m196_facetc_generic_static_typaram`[4] +
`d88_default_generic_params`[4] + d102/d108/d182/d368/probe_option/probe_result + минимальный
репро):

**non-forward-decl diff = 0 ВЕЗДЕ.** Единственные диффы — переупорядочивание opaque
`typedef struct Nova_X Nova_X;` (forward-decls user-типов). Доказано, что это ПРЕ-СУЩЕСТВУЮЩИЙ
run-to-run недетерминизм (тот же бинарь, run1↔run2 ≠ run2↔run3, варьируются ровно эти же opaque
typedef'ы — HashMap/HashSet итерация в forward-decl эмиссии), НЕ следствие правки. Конкретные
`NovaRes_`/`NovaOpt_` mono-имена — байт-идентичны.

Debug SHADOW-гейт (`shadow_check_node_substs`, byte-identity канал↔легаси) прошёл на ВСЕХ
компиляциях (0 ICE) — доказывает, что новые node_substs-значения байт-идентичны легаси.

## 4. Трейс новых channel-аннотированных сайтов (было 0)

`NOVA_NODE_SUBSTS_TRACE=1` на репро ПОСЛЕ правки: `producer=A callee=try_make n=1`,
`producer=A callee=maybe n=1` — static-ctor сайты теперь в node_substs (было 0). На реальном
корпусе затронутый класс = generic static-ctor (`fn Type[T].method(...)` без турбофиша).
Free-fn Result/Option-возврат уже был channel-covered (resolved_types) до правки — НЕ новьё.

## 5. Флагман

`nova check --strict-effects examples/flagship/aggregator/src/main.nv` → FAIL, но на
**ПРЕ-СУЩЕСТВУЮЩЕЙ** ошибке во ВНЕШНЕЙ зависимости: `nova-tls/.../handshake_test.nv:29
[E_CONSUME_PATTERN_REQUIRED]` (`Ok(s)` требует `Ok(consume s)`, D157-амендмент — свежий
consume-энфорс Plan 214/216/217). Верифицировано revert-циклом: baseline-бинарь даёт
ИДЕНТИЧНУЮ ошибку → моя правка НЕ регрессирует флагман (вне периметра: чужая dep, чужой
subsystem).

## 6. Разблокировка для GEN-сноса (для следующего агента)

- **`rt_slots_from_args` (per-arg структурный re-derive, node_substs MISS-fallback)** — теперь
  МЁРТВ для static-generic-ctor сайтов (канал их покрывает). Класс free-fn уже покрывался.
  Снос тела гейтится покрытием ОСТАЛЬНЫХ классов (instance-method — Producer B/`resolve_return_
  channel`; вне этой волны).
- **`resolve_result_option_ret` / `infer_result_type_params` / `resolve_result_te`** для
  free-fn/ctor Result/Option — уже channel-first (`infer_result_type_params` = legacy||channel,
  ~18067; `infer_result_type_params_channel` читает `resolved_types`). Их снос гейтится НЕ
  продюсером (он готов), а переносом SIDE-EFFECT'а `register_novares_decl`/`register_novaopt_decl`
  (typedef-регистрация внутри `resolve_result_option_ret` ~19207) в отдельный emit-pass
  (§10 196.4 risk «mono side-effects»). Это GEN/emit-pass работа, НЕ продюсер.
- **`infer_method_level_return_for_sum` / B11r/B11q** — METHOD-generic класс, ВНЕ периметра
  этой волны («вне method-generic-класса»); нужен Producer B (`resolve_return_channel`) widen.

## 7. Флаг конфликта зоны emit_c

НЕ тронут. Только `types/mod.rs`. Пересечений с codegen-детерминизацией / GEN-агентом нет.

## 8. Коммиты

1. `feat(types,196-ch): [M-196-ch-static-ctor-node-substs] node_substs для static-generic-ctor —
   receiver-carrier generics в позиционный subst-канал` — §2.
2. `docs(196): Zone CH продюсер — чекпойнт (state-verification + static-ctor node_substs)` —
   этот файл.

**В main НЕ мёржено. Push запрещён по заданию.**
