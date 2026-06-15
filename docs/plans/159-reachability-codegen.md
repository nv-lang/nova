<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 159 — Reachability-based codegen (dead-code elimination на эмиссии)

> **Создан:** 2026-06-15. **Реализован:** 2026-06-15. **Статус:** ✅ IMPLEMENTED (Ф.1–Ф.4 green; см. «Статус по завершении»). P2.
> **Владеет:** `[M-reachability-codegen-dce]` (Ф.1 core ✅). **Зависит от:** codegen (`emit_c.rs`), резолвер импортов.
> **Research:** [docs/research/11-stdlib-method-resolution-reachability.md](../research/11-stdlib-method-resolution-reachability.md).
> **Spec:** [D283](../../spec/decisions/09-tooling.md#d283) (reachability-codegen policy).
> **Разблокирует:** `[M-152.3b-char-methods-no-import]` ✅ CLOSED (no-import char-методы, Ф.4), снятие цикла prelude↔std.unicode, opt-in-стоимость Unicode-таблиц.

## Проблема (замерено 2026-06-15)
Наш codegen **не делает анализа достижимости** — эмитит в C **всё** объявленное/импортированное:
- неиспользуемая функция в том же файле **всё равно** эмитится (`unused_fn => 424242` → присутствует в C);
- `import std.unicode.{is_alphabetic}` + вызов только `is_alphabetic` → C **10652 строки**, включая collate/normalize/words/sentences/graphemes/case (всё, что не вызывается).

Следствия: (1) лишнее время компиляции и размер на каждой программе; (2) `std/unicode` пришлось сделать opt-in («платишь за таблицы только если импортировал»); (3) char Unicode-методы не могут жить в prelude (импорт `std.unicode` из prelude → цикл `prelude→std.unicode→collections→prelude` → stack overflow).

## Цель
Эмитить в C **только** функции/таблицы (`ro` lazy-static), **достижимые** от корней программы (`main`, `test`-блоки, `export`/FFI-entry). Это **вариант A** (модель Zig: «компилируется лишь достижимое от entrypoint»), а не прекомпиляция+линкер-срез (вариант B = Rust/Go, отложен).

## Алгоритм
1. **Корни (roots):** `main`; все `test "..."`-блоки; `export`-функции/типы, видимые наружу; FFI-`extern`-экспорты; реентри из рантайма (напр. финализаторы, GC-хуки) — собрать явный список.
2. **Worklist достижимости:** начать с roots. Пока worklist не пуст: взять функцию → резолв+тайп-чек её тела (лениво) → для каждой исходящей ссылки (вызов, взятие адреса, ссылка на `ro`-глобал/таблицу, конструктор типа) добавить цель в worklist, если ещё не посещена.
3. **Непрямые ссылки (критично для корректности):** учесть всё, что вызывается не по имени —
   - **protocol/trait-методы** через dynamic dispatch: если тип используется как протокол-значение, его реализации методов протокола — достижимы;
   - **методы-`@`-получатели** на типе, вызванные через значение;
   - **замыкания/функции-значения** (`fn`-указатели, переданные как аргумент);
   - **дженерик-инстансы**: достижимы только по факту инстанцирования (как mono-on-use);
   - **`ro` lazy-static таблицы**: достижимы, только если достижима функция, которая их читает (это и режет `category_data`/`collate_data`, если предикаты не вызваны);
   - значения, попадающие в `Any`/reflection-подобные пути (если есть) — консервативно достижимы.
4. **Эмиссия:** codegen проходит **только** по посещённому множеству. Непосещённые функции/таблицы — не эмитятся.
5. **Ленивый резолв модулей:** загрузка/тайп-чек тела модуля — при первой ссылке на его символ, а не eager на `import`. Это и снимает цикл: prelude знает сигнатуры char-методов (forward-decl), а тела из `std.unicode` тянутся лишь при реальном вызове.

## no-import char-методы (бонус, опц.)
После варианта A char Unicode-методы можно forward-объявить в prelude (только сигнатуры, без eager-import `std.unicode`); тело резолвится лениво при достижении вызова. Тогда `'Ω'.is_alphabetic()` работает **без `import`** (как Rust/Swift), но таблицы компилируются **только** если метод реально вызван. Закрывает `[M-152.3b-char-methods-no-import]`. (Решение «делать ли no-import или оставить import как Zig/Go» — ортогонально, принимается отдельно.)

## Фазы
- **Ф.1 — Граф вызовов прямых ссылок.** Worklist от roots по прямым вызовам/ссылкам на функции + `ro`-таблицы; эмиссия только достижимого. Без протоколов/дженериков-edge (пока консервативно: типы и их методы, использованные напрямую).
- **Ф.2 — Непрямые ссылки.** protocol dynamic-dispatch, замыкания/fn-указатели, `@`-методы — добавить в граф (иначе runtime «undefined symbol»). Это главный источник риска — тесты на каждый вид.
- **Ф.3 — Дженерик-инстансы on-use** (если ещё не так): инстанцировать+эмитить только достижимые комбинации типов.
- **Ф.4 — Ленивый резолв модулей + forward-decl prelude** → снять цикл prelude↔unicode; (опц.) no-import char-методы.
- **Ф.5 — Тесты + замеры + close.**

## Тесты (через релизный бинарь)
- **POS:** программа с `import std.unicode.{is_alphabetic}` + вызов только `is_alphabetic` → в C **нет** collate/normalize/words/sentences/graphemes (grep = 0); неиспользуемая функция в файле → **нет** в C. Размер C падает кратно (было 10652 строки).
- **POS (корректность):** полный `nova test` без новых FAIL — ничего достижимого не выпало (особенно протокол-методы, замыкания, дженерики, defer/финализаторы).
- **NEG:** специально достижимая-через-протокол/замыкание функция **присутствует** в C (регресс-guard, что Ф.2 не отрезала лишнего).
- **Замер до/после:** строки C + время компиляции на репрезентативной программе.

## Критерии приёмки
- **A1.** Неиспользуемые функции (в т.ч. в импортированных модулях/peer'ах folder-module) **не эмитятся** в C.
- **A2.** `ro`-таблицы эмитятся только если достижима читающая их функция (`category_data` отсутствует, если предикаты не вызваны).
- **A3.** Полный `nova test` зелёный — **ноль** «undefined symbol»/missing-emission регрессий (непрямые ссылки учтены).
- **A4.** Замер: кратное падение строк генерённого C на программе, использующей лишь часть `std.unicode`.
- **A5 (опц., Ф.4).** Цикл prelude↔std.unicode снят; (если решено) `'Ω'.is_alphabetic()` без `import`.
- **G0 (обязательный, «без упрощений как для прода»):** консервативная корректность — **никогда** не отрезать достижимое (лучше лишнее оставить, чем уронить runtime); все виды непрямых ссылок покрыты тестами.

## Связь / отложенное
- **Вариант B** (прекомпил std в `.o`/`.a`-кэш + `cc --gc-sections`) — отдельная задача под **скорость сборки** (не корректность). Отложен: нужен стабильный C-ABI std + кэш-инвалидация + кросс-toolchain `--gc-sections`. Маркер при старте.
- Чертежи: rustc monomorphization collector (обход от roots), Zig Sema/AIR (lazy per referenced decl).

---

## Статус по завершении (2026-06-15)

✅ **IMPLEMENTED** на ветке `plan-159-reachability-impl` (worktree `nova-p159`); НЕ смёржено в main.
Замеренная цель достигнута: программа, использующая лишь часть `std.unicode`, генерит **кратно меньше**
C, при этом ничего достижимого не выпало (G0 консервативная корректность соблюдена).

### Что зашипилось (вариант A, Zig-модель)

Реализовано в `compiler-codegen/src/codegen/emit_c.rs` + `compiler-codegen/src/lints.rs`:

1. **Kill-switch `NOVA_REACH_DCE`** (`reach_dce_enabled()`, читается один раз через `OnceLock`):
   unset или `!= "0"` ⇒ НОВОЕ поведение (DCE ON, **default**); `"0"` ⇒ байт-идентичное старое поведение.
2. **Единый reachable-set** — обобщил `compute_dead_free_fns` в `compute_dead_decls` →
   `DeadDecls{dead_fns, dead_consts, dead_method_keys}`. Один общий worklist-обход покрывает:
   - **free fns** (`receiver.is_none() && generics.is_empty() && body != External`);
   - **module-level `const`** (`Item::Const` — гигантские unicode-таблицы `const *_DATA str = "…"`);
   - **`ro` lazy-static globals** (`Item::Let`, single-name non-ghost).
3. **«нет `main` ⇒ держим всё»-guard СОХРАНЁН** — библиотеки и негативные `EXPECT_CC_ERROR`-фикстуры
   не режутся. В **executable** (есть `main`) под новой политикой `export`-free-fns **больше НЕ roots**
   (у Nova нет C-ABI-экспорта; FFI-entry = только `is_external`). Таблицы держатся, **только** если их
   читает достижимая функция/const.
4. **Method-level DCE** (Ф.2) — метод `T.m` эмитится, только если **И** значение типа `T`
   достижимо в коде, **И** селектор `m` вызван по имени (прямой вызов или protocol dynamic dispatch —
   оба пишут `m` на call-site). Метод, который не может выполниться, дропается (тело + fwd-decl) через
   `dead_method_keys`, считается монотонным fixpoint'ом. **Type∧name intersection** режет
   collate/normalize/word/sentence/grapheme/case-таблицы, сохраняя `is_alphabetic → alpha_flat → ALPHA_DATA`.
5. **Codegen-injected (desugar) селекторы** засеяны в `lints::collect_expr`, чтобы method-DCE их не
   срезал (это G0-критично — пропуск = undefined-symbol C): for/parallel-for iteration (`next`/`iter`),
   `==`/`!=`/`<`… (`equal`/`eq`/`compare`), str `+` (`concat`), `a[k]` (`index`),
   string-interpolation `"…${x}…"` (StringBuilder `with_capacity`/`append`/`as_str` + `display`/`debug`/`from`).
6. **Дженерики on-use** (Ф.3) — generic free fns не-кандидаты (`generics.is_empty()` гейтит и
   `is_fn_candidate`, и `is_dead_method`), поэтому неинстанцированная комбинация просто никогда не
   эмитится, а достижимая транзитивно — мономорфизируется и эмитится. Const, читаемый только из
   generic-body, консервативно держится (generic-body refs = безусловные roots = over-keep). Логика
   emit_c не менялась — только верификация + тесты.
7. **No-import char-методы** (Ф.4, Option A) — import-резолвер детектит char-Unicode **method-call**
   селектор (`expr.foo()`, отличён от bare-free-fn формы новым `@method:`-тегом в `collect_expr`) и
   инжектит `import std.unicode` в **пользовательский entry-модуль** (НЕ в prelude-фасад) — обычный
   cycle-free путь, цикл `prelude→unicode→collections→prelude` не входится. Ф.1 DCE затем срезает все
   неиспользуемые таблицы → no-import стоит ноль для программ, не вызывающих эти методы. Bare
   free-function вызовы (`general_category(0x41)`) остаются opt-in за `import`.

### Замер (BEFORE → AFTER)

Программа `nova_tests/_p159_measure/measure_partial_unicode.nv` — executable с `main`,
`import std.unicode.{is_alphabetic}`, вызывает только `is_alphabetic(0x41)`:

| метрика | BEFORE (DCE off / pre-159 baseline) | AFTER (DCE on, default) |
|---|---|---|
| строк C | **10606** | **2494** (~4.25×↓) |
| `collate` | 37 | **0** |
| `normalize` | 9 | **0** |
| `GC_DATA` | 2 | **0** |
| `ALPHA_DATA` (нужная) | 2 | **2** (сохранена) |
| компиляция + запуск | PASS | PASS (печатает корректно) |

**Kill-switch A/B:** `NOVA_REACH_DCE=0` воспроизводит BEFORE точно (10606 / 37 / 9 / 2) —
байт-идентичное старое поведение подтверждено.

### Per-phase outcome

| Фаза | Статус | Итог |
|---|---|---|
| **Ф.1** — core reachability DCE (free fns + const/ro-таблицы) | ✅ green | 10606→2494; collate/normalize/GC_DATA → 0; ALPHA сохранена; kill-switch byte-identical |
| **Ф.2** — method-level DCE (type∧name intersection) + desugar-селекторы | ✅ green | Method-DCE корректен и консервативен; найден+починен over-prune string-interpolation; per-area G0 zero NEW FAIL |
| **Ф.3** — дженерик-инстансы on-use | ✅ green | Verify-only (логика emit_c не менялась); неинстанцированная комбинация не эмитится; транзитивный инстанс мономорфизируется |
| **Ф.4** — no-import char-методы / снятие цикла | ✅ green (Option A) | `'A'.is_alphabetic()` без `import`; цикл не входится; стоит ноль (DCE срезает) |
| **Ф.5** — тесты + замеры + close | ✅ | dce_tests 21/0; ~13 production-фикстур; **полный регресс** (см. ниже): 1 реальный over-prune найден+починен, иначе zero NEW FAIL |

### Критерии приёмки

- **A1** (неиспользуемые функции, в т.ч. в импортированных peer'ах, не эмитятся) — ✅ MET.
- **A2** (`ro`/const-таблицы только если достижим читатель; collate/normalize/GC_DATA отсутствуют) — ✅ MET (0/0/0).
- **A3** (ноль «undefined symbol»/missing-emission регрессий — непрямые ссылки учтены) — ✅ MET
  (per-area A/B vs `NOVA_REACH_DCE=0`: plan159 14/0, plan108_4 12/1≡OFF, plan99 9/0, plan100_4_4 13/0,
  plan103_4 25/0, plan100_2 17/0, plan152_3 4/0, plan91 2/0; + parity на plan152_7/152_4/effects/
  100_4_1/90_1/generics/138/136_1 — identical ON vs OFF).
- **A4** (кратное падение строк C) — ✅ MET (~4.25×, 10606→2494).
- **A5** (опц., Ф.4: цикл снят; `'Ω'.is_alphabetic()` без `import`) — ✅ MET.
- **G0** («без упрощений как для прода» — консервативная корректность, никогда не отрезать достижимое;
  все виды непрямых ссылок покрыты) — ✅ MET (любая коллизия имён over-keep'ит; desugar-селекторы
  засеяны; method-body DCE консервативен — type∧name; over-keep допустим, over-prune = release-blocker).

### Полный регресс + найденный over-prune (2026-06-15, после воркфлоу)

Прогнан **весь** `nova_tests` (10 батчей, **PASS=2738 FAIL=172 SKIP=61**) на релизном бинаре,
затем каждый падавший dir сверен **kill-switch A/B** (`NOVA_REACH_DCE=0` на ТОМ ЖЕ бинаре и
фикстурах — единственная валидная база; main-бинарь оказался устаревшим 15:55 < main HEAD,
сравнение с ним невалидно). Итог по 51 падавшему dir: **DCE-ON FAIL ≤ DCE-OFF FAIL везде** —
все 172 падения pre-existing.

**Найден 1 РЕАЛЬНЫЙ over-prune** (G0-нарушение, починен до мёржа):
`plan140_1/invariant_msg_interp_neg` — DCE-ON CC-FAIL `nova_str` ← `int`, DCE-OFF PASS.
Корень: `lints::collect_used_names` обходил `Contract.expr`, но НЕ `Contract.message_expr` —
интерполяция сообщения контракта (`invariant balance>=0, "...${balance}..."`, а также
`requires`/`ensures`) десугарится в `InterpolatedStr`, чей int→str-конвертер инжектится только
на сайте нарушения. Без обхода `message_expr` method-DCE срезал конвертер.
**Fix:** обходить `message_expr` в обоих arm'ах (`Item::Fn` контракты + `Item::Type` инварианты).
Чисто аддитивно (только расширяет used-set → больше over-keep → не может создать новые prune).
Guard-фикстуры: `invariant_msg_interp_neg` (Type-path, уже была) + новая
`plan159/f2_contract_msg_interp_dce.nv` (Fn-path, requires-интерполяция под `main`/DCE-ON).
После fix: plan140_1 15/0, plan159 19/0, замер не деградировал (collate/GC_DATA=0, ALPHA сохранён).

`plan103_5/once_stress_mn_4workers` мелькнул как DCE-ON FAIL — оказался **флакой** стресс-таймаута
(повторный прогон PASS 44.6s vs ранее timeout 177s), не DCE-баг.

### Упрощения / остаток

- **Method-DCE — coarse-by-name** (намеренно консервативно, НЕ упрощение прода): метод режется только
  если **И** тип-имя, **И** селектор недостижимы — name-collision over-keep'ит. См.
  `[M-159-method-pruning]` (P3) — точечная per-kind десугар-аудитория для редких codegen-injected
  селекторов (drop/finalizer, embed auto-proxy, closure-captured методы).
- **Ф.4 = Option A** (инъекция import в entry-модуль), а НЕ полноценная lazy-module-resolution.
  Полная ленивая загрузка/тайп-чек тел модулей при первой ссылке — `[M-159-lazy-module-resolution]` (P3),
  отложена (Option A закрыла эргономику cycle-free и zero-cost).
- Pre-existing codegen-ограничения (НЕ DCE-баги, fail идентично под `NOVA_REACH_DCE=0`):
  module-qualified free-fn вызовы (`u.is_alphabetic()`) эмитят alias в C; `const B = A + 1` не
  поддержан codegen'ом; multi-hop free-fn→free-fn в standalone test-build; concrete-type `a[k]`
  резолв-overload. Все четыре независимы от Plan 159.

### Merge

Полный `nova test` (весь корпус) прогнан по 10 батчам + kill-switch A/B (см. «Полный регресс»);
1 over-prune починен, иначе zero NEW FAIL. `[M-reachability-codegen-dce]` Ф.1-Ф.4 → DONE.
Ветка `plan-159-reachability-impl` ребейзнута на актуальный main и смёржена (FF).
