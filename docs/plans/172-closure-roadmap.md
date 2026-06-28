# Plan 172 — единый секвенированный roadmap закрытия (синтез 2026-06-28)

> Источник: ultracode-workflow `wf_f4e76bbd-b92` (6 ридеров-карт + синтез, verified vs живой код).
> Цель: закрыть ВЕСЬ зонт 172. 172.2 ✅ / 172.3 ✅ закрыты. Драйвер ядра 172.1 — **int-де-схлопывание**
> (tracer): единый `resolved_type_to_c` несёт width+sign; примитивы НЕ схлопываются в `nova_int`
> (`uint≠int`, `u8≠i8≠i16`), кроме `int≡i64` (D129 alias). Каждый поведенческий шаг — §7
> (detect→blast→migrate→verify-vs-CLEAN+ПОЛНЫЙ регресс, НЕ сэмпл — урок Ф.3) + spec_tests/conformance per-D.

## Критический путь
**A(int-де-схлопывание) → B(bounded channeling) ‖ REG(registry) → C(GC-hardening) → D(P67 удаление legacy) → E(FIN endgame) → F(172.4 value-ABI) → G(172.5 mut-ref) → ЗОНТ ЗАКРЫТ.**
A и REG автономны (старт сразу); C(GC) — самый дорогой РЕАЛЬНЫЙ блокер, разведку рано параллельно.

## Session A — int-де-схлопывание (АВТОНОМНО, без гейтов). Опора: ResolvedType.Scalar{width,signed} + единый `primitive_name_to_c`/`resolved_type_to_c` (U.5/U.4.8 landed).
- **K1 (keystone)** ✅ ИСПОЛНЕН (pending full-regress-commit): `receiver_c_type:11829` int-семья БОЛЬШЕ не схлопывает→nova_int; делегирует в `primitive_name_to_c` (единый скалярный лист). `int`/`i64`→nova_int (D129), `uint`→nova_uint, `u8`→nova_byte, `u16/u32/u64`→uintN_t, `i8/i16/i32`→intN_t. `size`→nova_int (нет в таблице, отдельный вопрос). Pointer/value-record/tuple/generic-T/array ветки НЕ тронуты. Verified: `Nova_uint_method_*(nova_uint,...)`, беззнаковое сравнение (flip-тест). Acceptance: `spec_tests/conformance/d130_uint_method_compare.nv`.
- **K2** op-emission operand-каналы: `is_typed_integer:35749` добавить `nova_uint` (uint теряет promotion). Bare-операторы НЕ трогать (знак из C-типов операндов). Deps: K1 (parallel-able).
- **K3** overflow checked-vs-wrap по ResolvedType (`:20837`): int(i64)→checked-panic; sized iN/uN→wrap; u64/uint→wrap. Запрос `resolved_types[expr.id].Scalar` напрямую. Deps: K1 (после K2).
- **K4** литералы+lexer+const+cast-SRC: `emit_typed_int_literal:5322` добавить `nova_uint(ULL)`; source `unwrap_or('nova_int'):5353/19851` → канал + `[E_*]` (§1); lexer decimal:544 u64-fallback (carrier IntLit i64→u64+sign); const CharLit:5378→nova_char; as-cast SRC `:21709` из `resolved_types[inner.id]`. Deps: K1. D227/D128/D54.
- **K5** дубли re-derive: `sum_schema_registry:771` collapse-dict → делегация; `emit_method_value_typed:33918` + method-value binding `:18736` (char→nova_int `:18741` VIOLATES D128) → `primitive_name_to_c`. Parallel K1-K4.

## Session B — bounded legacy-channeling (‖ REG). Consumer-flip READY, работа в ЧЕКЕРЕ.
- **172.1-P** (P1-P5/P4a): P1 generator-self-typing; P2 Member/Index→канал из checker SCHEMA (НЕ poisoned array_element_types); P3 Binary-result (остаток generic-operands); P4a bounded-Call→resolved_callees (~60% mechanical U.3); P5 Ident/SelfAccess consumer-lock. COUPLING: lossless-канал из A делает int-flip ЗВУЧНЫМ. Channel-filling DRIED UP (step1 −20) — НЕ гриндить, только bounded-победы.

## REG — единый реестр (§0.6, ‖ B). U.2.4→U.1.5/1.6→U.3.2/net/generic-Index.
- U.2.4 (codegen consume unified method_overloads, byte-identical INFEASIBLE→re-scope); U.1.5 (3 checker name-res через shared registry+import-scope); U.1.6 (remove `builtins:HashSet` types/mod.rs:11681 + `include_str!` external_registry.rs:110-132). Unblocks U.3.2/net-slice/generic-Index. Lib-names GATED на U.4(P67).

## Session C — FORK: GC-root-finding-hardening (Plan 144). РЕАЛЬНЫЙ блокер (§7.7 — runtime-segfault).
- `types/mod.rs:9973-9996` container-return guard BAILS на Array/Tuple/Named-generic (резолв container-return → fresh mono → reshuffles layout → conservative-GC segfault plan154). ЛИБО Plan 144 GC-hardening (robust layout-stable root-finding), ЛИБО codegen-time mono-subst аннотация с гарантией. Unblocks 172.1.2 P4b (chain-Call) И 172.4 Ф.3 (общий receiver-inference-depth корень). НЕ гриндить channel-filling до.

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
