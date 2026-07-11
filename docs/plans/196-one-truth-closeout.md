<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 196 — «Одна правда» close-out: завершение typed-IR (§0), раз и навсегда

**Статус:** 🔥 IN PROGRESS 2026-07-11 (решение владельца: «весь зонт сразу, закрыть
раз и навсегда»). **Приоритет:** P0 (ключевая идея 172-186). **Умбрелла над:**
172.1 (unified-type-engine, U-хвосты), 172.12 (typed-IR-mono, Ф.2-Ф.4), 172.13
(constraint-inference, Ф.3). Координирует, НЕ дублирует.

## Проблема (корень, подтверждён recon 2026-07-11)

`infer_expr_c_type` (`compiler-codegen/src/codegen/emit_c.rs:48800`, ~2604 строки) —
**второе окно правды**: codegen ЗАНОВО инферит C-тип выражения (Каналы 1-6 + армы
6c-6z, помечены «ДОСЛОВНЫЙ подъём legacy-арма»), хотя чекер уже разрешил тип
(`resolved_types: HashMap<ExprId, ResolvedType>`, D315). Цель 172.1 U.4 / 172.12 —
`infer_expr_c_type` = **тонкий лоуеринг** `resolved_type_to_c(ir.type_of(expr))`, армы
6c-6z снесены (`[M-172.1-lifted-legacy-arms]`).

**Recon-факты:**
1. Аннотация чекера **ДЫРЯВАЯ**: литералы не аннотируются вовсе; generic
   RecordLit/TupleLit/method-chain — тоже (`[M-104.10-expr-types-coverage]`). ⇒
   keystone = **checker+codegen**, не codegen-only.
2. 172.12 закрыл ПОЛОВИНУ: строковая type-identity (mono-mangle) схлопнута (A1-A8),
   но `struct IrExpr` **никогда не создан**, Ф.2 mono-worklist RT-типизирован но
   ЗНАЧЕНИЯ массово `ResolvedType::Raw(String)`, Ф.3/Ф.4 **не начаты**.
3. **Инвариант дрейфанул:** «0 raw `Nova_`/`____`-decode вне `debt_`» (заход 8) →
   сегодня **70 хитов, 12 вне debt** (Plan 186 добавил без CI-защиты).
4. Крупнейший арм 6q = `infer_call_ret_c` (~2592 строки, generic-method-return
   mono) — НЕ swap; нужен реальный mono-inference-движок (Ф.2 «mono на IR»).

## Фазы (каждая: high-effort агент, byte-identity-гейт + merge)

- **Ф.1 — дешёвое ядро (риск≈0, ~3-4 захода):**
  (a) **CI-линт** grep-инвариант «0 raw `Nova_`/`____`-decode вне `debt_`» (cargo
      test / nova-lint) — чинит корень дрейфа;
  (b) refresh-audit: пересчёт 70/12 + полный A/B/C-каталог всех ~40 под-армов 6z
      построчно;
  (c) аннотация литералов + empty-sum в чекере (`f1_expr_inner`, строго аддитивно);
  (d) prove-dead→delete: trace-инструментация доказывает 0 попаданий в
      литеральные/empty-sum армы → снос.
- **Ф.2 — checker-extension:** ✅ ВЫПОЛНЕНО 2026-07-11 (be70b65b2+8a73c6eb8). Match +
  RecordLit{Some} **удалены** (0 хитов — оказались shadowed БЕЗУСЛОВНЫМИ дубликатами-
  предшественниками «6n»/«6l», т.е. мёртвые-редундантные — §0-находка про дубль-каналы).
  TupleLit-элементы аннотированы (concrete_value_named). **TupleLit-арм ОСТАВЛЕН** —
  корень НЕ generic, а **пробел scope-резолюции чекера** `[M-196-tuplelit-ident-scope]`:
  Ident-элементы не резолвятся через `infer_expr_type(el, scope)`, хотя резолвятся ниже
  через legacy `var_types` — отдельный атом (Ident-scope-плюмбинг), не kind-гейт Ф.2.
- **Ф.3 — non-infer-consumer sweep:** `emit_expr`/`emit_call`/`emit_generic_type_instance`/
  … (12 свежих raw-сайтов в 10 функциях) → `debt_`, восстановить инвариант.
- **Ф.4 — глубокое ядро (масштаб 172.12):** generic-method-return mono-движок =
  **172.13 Ф.3** (constraint-inference: Binary-Join/If-Match-Join/resolve-семья) как
  checker-фундамент + codegen потребляет resolved-тип → снос 6q/6m по мере покрытия.
- **Ф.5 — array-vec-unify:** `[M-array-vec-unify]` (ОСТОРОЖНО — заход 9 172.12 дал
  «Frankenstein» при механическом слиянии; только координированно ctor↔binding).
- **Ф.6 — финал:** снос `ResolvedType::Raw`, закрытие `[M-172.1-lifted-legacy-arms]`,
  опустошение U.7-allowlist. **Тело `infer_expr_c_type` (вся независимая инференция,
  ~2600 строк) УДАЛЯЕТСЯ** (owner Q 2026-07-11) — функция схлопывается в тонкий
  `resolved_type_to_c(ir.type_of(expr))`, либо call-sites зовут `resolved_type_to_c`
  напрямую и функция сносится. Второго окна не остаётся.

- **Ф.1e — УЖЕСТОЧИТЬ wildcard → ОТЛОЖЕНО НА Ф.6 (эмпирика 2026-07-11).** Wildcard `_ =>`
  в `infer_expr_c_type` в release отдаёт ПУСТУЮ строку → тихо `nova_unit` (§3-нарушение,
  D368). ПОПЫТКА ужесточить РАНО ПРОВАЛИЛАСЬ: wildcard **реально достигается**
  непроаннотированными value-kind'ами (`Discriminant(50)` conformance, `Discriminant(39)`
  std — `[M-196-wildcard-reached-kinds]`), громкий panic ломает conformance. Это
  пред-существующие §0-пробелы, НЕ ложно-мёртвые (удаления 196 звучны: 0-hit-kind'ы на
  prove-dead-прогоне ≠ wildcard-reached-kind'ы). Полный харденинг возможен ТОЛЬКО когда
  Ф.4 достроит checker-покрытие этих kind'ов → wildcard станет недостижим → тогда panic.
  Перенесено в Ф.6-приёмку.

## Гейты (КАЖДАЯ фаза)

1. **byte-identical** emitted-`.c` diff vs clean baseline (`nova build --keep-artifacts`
   на pre-change коммите; same-binary control отделяет `[M-codegen-emission-nondeterminism]`).
   Первый шаг Ф.1 — **закрепить воспроизводимый скрипт** (сейчас его нет, каждый заход
   делал вручную).
2. `nova test --positive --compile-error spec_tests/conformance` δ0.
3. **CI-линт** зелёный (после Ф.1a).
4. **compiler-conventions.md полное соответствие:** §0 (единый источник) · §1 (проверки
   в чекере) · §2 (нет хардкод-stdlib) · §3 (нет тихого `nova_int`/wildcard-empty-string
   fallback, D368 — wildcard = ГРОМКИЙ panic, см. Ф.1e) · §9
   (нет дублирования резолва) · §10 (нет двух окон) — грепом на каждой фазе.
5. neg-тесты где применимо.

## Приёмка (close-out)

- `[M-172.1-lifted-legacy-arms]` ЗАКРЫТ (армы 6c-6z сняты).
- grep-инвариант «0 raw-decode вне debt_» = 0 И под CI-защитой.
- U.7 zero-CC-FAIL allowlist ПУСТ.
- `infer_expr_c_type` = тонкий лоуеринг (consume `ir.type_of`), 0 независимой инференции.
- 172.1 U.2.4 (codegen-mangling→SigRegistry) · U.6.4 · U.7 · 172.13 Ф.3 — закрыты.

## Границы

Оптимизации на IR (SSA/DCE) — отдельный горизонт (IR сначала как typed-carrier).
`[N]T` value-семантика — заморожена отдельно (172.12).

## Owner Q&A (2026-07-11)

- **SelfAccess-снос обоснован:** `@` эмитится как `nova_self` (ptr-биндинг в `var_types`,
  ref-параметр на уровне C — `emit_c.rs:956`), его тип берётся ОБЩИМ путём резолва
  биндингов → спец-арм SelfAccess избыточен → 0 хитов на корпусе → удалён корректно.
  Устаревший doc-комментарий (closures-достижимость) отстал от ref-модели `@`.
- **`int as char` под unsafe — ЗАФИКСИРОВАТЬ D-амендментом:** реализовано (cb9944acd) —
  `int as char` разрешён в `unsafe`-контексте (codegen-запрет пропускает под unsafe),
  чекер распознаёт int→char As-cast как unsafe-op (E_UNSAFE_UNUSED §21 map). Позволяет
  `int @to_char()` в .nv через `unsafe { @ as char }`. Нужен D-блок (легитимизировать).
