# Plan 172 — единый секвенированный roadmap закрытия (синтез 2026-06-28)

> Источник: ultracode-workflow `wf_f4e76bbd-b92` (6 ридеров-карт + синтез, verified vs живой код).
> Цель: закрыть ВЕСЬ зонт 172. 172.2 ✅ / 172.3 ✅ закрыты. Драйвер ядра 172.1 — **int-де-схлопывание**
> (tracer): единый `resolved_type_to_c` несёт width+sign; примитивы НЕ схлопываются в `nova_int`
> (`uint≠int`, `u8≠i8≠i16`), кроме `int≡i64` (D129 alias). Каждый поведенческий шаг — §7
> (detect→blast→migrate→verify-vs-CLEAN+ПОЛНЫЙ регресс, НЕ сэмпл — урок Ф.3) + spec_tests/conformance per-D.

## Критический путь
**A(int-де-схлопывание) → B(bounded channeling) ‖ REG(registry) → C(GC-hardening) → D(P67 удаление legacy) → E(FIN endgame) → F(172.4 value-ABI) → G(172.5 mut-ref) → ЗОНТ ЗАКРЫТ.**
A и REG автономны (старт сразу); C(GC) — самый дорогой РЕАЛЬНЫЙ блокер, разведку рано параллельно.

## V — D-conformance acceptance suite (= **172.1 U.7** расширенный) — ПАРАЛЛЕЛЬНО ВСЕМ фазам, driver закрытия
Независимая верификация: **каждый 172-релевантный D-блок (172.1-172.5 scope) → conformance-тест в `spec_tests/conformance/`** (один пир-модуль, минимум CU; против ТЕКУЩЕЙ spec-семантики с inline-амендментами; forward-compat D — carrier-level). **Проходящий = D конформен (часть 172 закрыта); падающий = gap (driver code-работы A-G).** Когда suite ПОЛОН и весь зелёный + legacy удалён (D) → движок конформен спеке/D = **критерий приёмки зонта**. Идёт ‖ всем code-фазам (не блокирует, не блокируется — это U.7 «C не ловит типы», broadened). Покрыто: D130 (uint K1/K4), D328 (==Ф.2); авторено (триаж): D52/D54/D55/D72/D119/D123/D125/D128/D129/D156/D215/D216/D226/D239/D310/D315/D326/D327. Методология: [test-conventions.md §spec/D-conformance suite].

## Session A — int-де-схлопывание (АВТОНОМНО, без гейтов). Опора: ResolvedType.Scalar{width,signed} + единый `primitive_name_to_c`/`resolved_type_to_c` (U.5/U.4.8 landed).
**✅ Session A ЗАКРЫТ (2026-06-28): int-де-схлопывание — uint≠int, u8≠i8≠i16, точные C-типы во всех позициях, беззнаковые операции, uint-литералы до u64::MAX. 0 регрессий vs clean (PASS:490 FAIL:39 неизменно).**
- **K1 ✅ COMMITTED cfc8e69e:** `receiver_c_type:11829` делегирует в единый `primitive_name_to_c` (int/i64→nova_int D129; uint→nova_uint; u8→nova_byte; u16/u32/u64→uintN_t; i8/i16/i32→intN_t). Acceptance `d130_uint_method_compare.nv` (flip-доказан).
- **K2/K4/K5 ✅ COMMITTED b0e95496:** K2 `is_typed_integer+=nova_uint`; K4 `emit_typed_int_literal+=nova_uint` + lexer decimal u64-fallback (uint-литералы D130); K5b `emit_method_value_typed` делегирует (пропускал i8/i16/i32/u16/u32/u64→Nova_X*); K5c `is_type+=uint/size` + `nova_type_name_from_c+=char/uint` (мис-диспатч D128/D130). Acceptance `d130_uint_literal_width.nv`.
- **K3 (overflow) DEFERRED — фактически no-op:** sized/uint ЛОКАЛЫ уже несут точный C-тип из declaration-lowering → условие `lty==nova_int` их исключает → wrap (верно). Остаток (legacy-infer-collapse операндов) = P-фазы-канал, не точечный K3.
- **K5a (sum-schema collapse-dict `sum_schema_registry:771`) DEFERRED — латентен:** схемы prelude/errors.nv используют только int/str/bool/char (sized не встречается); + кросс-модульный доступ к `primitive_name_to_c`. Сделать при де-хардкоде sum-schema (U.8) или отдельно.

## Session B — bounded legacy-channeling (‖ REG). Consumer-flip READY, работа в ЧЕКЕРЕ.
- **172.1-P** (P1-P5/P4a): P1 generator-self-typing; P2 Member/Index→канал из checker SCHEMA (НЕ poisoned array_element_types); P3 Binary-result (остаток generic-operands); P4a bounded-Call→resolved_callees (~60% mechanical U.3); P5 Ident/SelfAccess consumer-lock. COUPLING: lossless-канал из A делает int-flip ЗВУЧНЫМ. Channel-filling DRIED UP (step1 −20) — НЕ гриндить, только bounded-победы.
- **✅ Numeric residual-collapse audit (2026-06-29, wf_358871b5):** 5 подтверждённых collapse-багов (26 проверено, 21 spec-correct отсеян). Закрыто 3 рангами:
  - **RANK 1 `ba1aac55`** — checker Binary-арм: позиционный `infer_expr_type(left).or_else(right)` схлопывал `литерал<op>narrowvar` (`2*a`u8, `1+n`u32/uint, `0xFF&b`, сдвиги) в int. Фикс §0: общий `number_exprs::promote_arith_rt` (seed+checker одно правило) + uint в `is_typed_int` (зеркалит legacy). Закрывает 2 бага (A1+A3) + CRC-shift hazard.
  - **RANK 2 `6912055e`** — `f3_check_member` NamedTuple-арм материализует substituted field-тип в канал (как Record-арм); generic `Pair[u8].a`→nova_byte (был nova_int). Закрывает bug B.
  - **RANK 3 `d2752b84`** — ReadBuffer sized `read_*` (read_i8/u16/i16/u32/i32) width-exact C-тип вместо хардкод-nova_int. Закрывает bug E. [Layer B: routing через resolved-callees + delete блока — followup.]
  - **RANK 4 (bug D, DEFERRED — §7-careful session):** `infer_expr_type` (mod.rs:9818) без Binary/Member/Index-армов → `would_narrow_into` (D54) не доходит → `ro c u8 = a+b` молча truncate'ит (НЕ collapse — отсутствие narrowing-диагностики; pre-existing debt `[M-scalar-nonliteral-narrowing-not-enforced]`). HIGHEST-risk (shared inference engine + GC-layout perturbation plan154); нужен detect-mode blast-radius + zero-false-positive калибровка. Каждый ранг: detect172 pos+neg(14/0) + conformance + регресс — зелёные.

## REG — единый реестр (§0.6, ‖ A, лёгкое касание). → [172.1-reg-execution.md](172.1-reg-execution.md)
**Coupling с A: PARALLEL** — REG-4 ЯВНО оставляет int-примитивы в language_builtins (удаляет только stdlib-имена); REG-6 наследует A's int-de-collapse в type_ref_to_c БЕСПЛАТНО. REG-0/1/2 ‖ A; REG-4/6/7 sync с финальным int-каноном A.
- **REG-0 (KEYSTONE/ENABLER, first-exec, ADDITIVE):** from_module-merge (`emit_c.rs:2619-2622`) расширить на `receiver_types`+`type_decls` (уже строятся, выбрасываются). §10-предусловие: снабжение несёт типы ДО удаления include_str!.
- **REG-1** declared-intrinsic table (gc/bench/fibers/runtime из .nv extern-сигнатур); **REG-2** build_base индексирует Item::Type→is_known_type registry-aware; **REG-3** 3 name-res сайта→единый predicate + сузить PascalCase-permissive-hole (§3 soundness); **REG-4** remove stdlib-имена из builtins:HashSet (оставить language_builtins); **REG-5** remove include_str!+BUILTIN_SIG_MODULES (sync/net GATED U.4); **REG-6** method_overloads из единого SigRegistry (soundness-гейт, не byte-identical); **REG-7** unblocks U.3.2/net-slice/generic-Index.
- §10 ordering — главный риск: REG-0 ПЕРВЫМ; REG-5 строго после REG-0+2+3+4; sync/net GATED U.4. Каждое removal — revertable-коммит + FULL regress.

## Session C — container/generic channeling блокер. **ВЕРДИКТ recon: путь B (codegen-mono-subst), НЕ Plan 144.** → [172.1-session-c-gc-recon.md](172.1-session-c-gc-recon.md)
- Блокер СУЖЕН (recon `wf_ba30367c-daf`): `types/mod.rs:9992-9996` container-bail имеет ДВЕ причины — (a) mono-subst-timing-layout-crash [КАНАЛ], (b) f3_check_member false-positive [INLINE]. **DECOUPLE (`types/mod.rs:9717-9723`) УЖЕ изолировал (b) от канала** → channeling упирается ТОЛЬКО в (a).
- **B (codegen-time mono-subst annotation)** — §0-выровнен (тот же канал + единый `resolved_type_to_c` с subst-резолвом `emit_c.rs:2120-2127`), §7-измерим. A (Plan 144 GC-hardening: Boehm conservative `slot_size`-зависимый root-range → layout-Heisenbug→SEGV) = 6-12 мес, parallel GC-носитель, Plan 144 ЯВНО «не блокирует» + NOT STARTED (144.2/144.3/144.0.1 🔴). Ф.3-урок (robust EMIT-based, not infer) подтверждает B.
- **План B:** (1) container-возврат → ResolvedType в канал, лоуэрить в mono-эмиссии (subst populated); снять bail ПОЭТАПНО по арму. (2) энумерировать mono-side subst-population gaps (env-log в `resolved_named_to_c`; proba D: gaps = МНОЖЕСТВО, сломала modules/lru). (3) закрыть gaps + restore modules/lru. (4) verify plan154 no-segfault + полный регресс.
- Unblocks: **172.1.2 P4b** (chain-Call channeling) + **172.4 Ф.3** (fluent value-ABI). Careful multi-session (НЕ хвост-сессии). НЕ гриндить channel-filling до.

## Session D — keystone-deletion: 172.1-P67 (после P+REG+GC+K).
- P6 (cond) generic-mono erased-stub substrate (только container-without-var_types остаток). P7: УДАЛИТЬ `infer_expr_c_type_legacy:36432` (~1000 строк) + call `:36146`; relocate side-effects (typedef/mono-reg); author **D312**; whitelist-grep (forbid reachable re-derive); reconcile acceptance byte-identical→soundness. Headline: «C не ловит типы».

## Session E — 172.1 endgame: 172.1-FIN.
- U.6.1 (`type_ref_to_c:6369` full-retire→resolved_type_to_c); U.8 (LAST: de-hardcode sum-schema baseline Option/Result/RuntimeError из .nv + remove legacy sum_schemas, Deps U.4+U.5); U.7.3 (zero-CC-FAIL allowlist 17→0); U.6.4/U.5.5b/U.7.2/U.1.7-8/D312. → **172.1 ЗАКРЫТ** (6 критериев зонта).

## Session F — value-ABI: 172.4-V (gated на GC + U.5/U.6).
- §2.1 ЗАПРЕЩАЕТ targeted band-aid (self-return-flag+chain-root+deref = name-keyed спец-кейсы). CORRECT: careful carrier-model refactor (consistent pointer-carrier value-record во ВСЕХ контекстах + value-decay РОВНО в explicit value-position). Ф.3 re-attempt: (1) decay на **robust EMIT-BASED сигнале** (флаг «RHS заэмитил value-record-ptr»), НЕ context-fragile infer (16 NEW fails); (2) ПОЛНЫЙ регресс; (3) `spec_tests/conformance/d326_value_record_fluent.nv` (с landing). Ф.2 == ✅ (D328). Затем Ф.4 auto-by-ref / Ф.5 heap↔stack / Ф.6 RVO (perf).

## Session G — 172.5 mut-ref + exclusivity (HARD после 172.4, общий @/-> @).
- Ф.1 Parser (ro/mut ref, `ref <place>`); Ф.2 Checker exclusivity (`E_REF_ALIAS_OVERLAP`); Ф.3 Checker -> @ mode+chain-gating (D181); Ф.4 Codegen mut-ref→C-pointer; Ф.5/6 pos/neg+регресс. Ф.0 spec D326 ✅. → **ЗОНТ 172 ЗАКРЫТ**.

## Жёсткие гейты (все шаги)
1. Поведенческий шаг: detect→blast-radius→миграция→verify vs CLEAN-бинарь (kill-switch ТОТ ЖЕ бинарь) + ПОЛНЫЙ регресс (НЕ сэмпл — Ф.3 §7.6).
2. Удаление/behavior-change гейт = SOUNDNESS (detect172 pos+neg + 0-CC-FAIL + nova test green + baseline-DELTA vs clean), НЕ byte-identical (§0.5 снят).
3. §0/§10: каждый фикс СТРОИТ к единому источнику (resolved_type_to_c/primitive_name_to_c), НЕ патчит расходящийся путь. Band-aid «receiver uint→nova_uint» ОТВЕРГНУТ в пользу делегации.
4. §1: no silent nova_int — resolve-failure = `[E_*]`, НЕ угаданный тип.
5. Spec-first §5: неунормированное поведение → D ПЕРЕД кодом; acceptance = spec_tests/conformance/ per-D, ТОЛЬКО проходящие.
6. Container/generic channeling (P4b/Ф.3) НЕ форсить до GC-hardening — РЕАЛЬНЫЙ блокер. Channel-filling-grind НЕ метрика прогресса.
7. git add конкретных файлов; `git diff --cached --stat` до commit; БЕЗ Co-Authored-By Claude; commit на задачу.
