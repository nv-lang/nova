# Plan 196 — Карта архитектуры под typed-IR (read-only разведка, 2026-07-11)

> Грунтовка для переплана 196 под §0 «одна правда». Факты собраны read-only
> обходом main. Вход для решения «в-lite vs в-full» и структуры фаз.

## 1. Пайплайн и наличие IR

Драйвер: `main.rs:376-508` (`cmd_compile`). Порядок: parse→AST → embed → auto_derive(serde)
→ alpha_rename → **number_exprs** (ExprId(1..N) + СЕМЯ `HashMap<ExprId,ResolvedType>`)
→ imports → **types::check_module** (чекер: наполняет `resolved_types`/`resolved_callees`,
мутирует AST, НОВЫЙ IR НЕ создаёт) → слияние семя+чекер → const_fn_mono (const-fn, не типы)
→ desugar (плодит UNSET-узлы) → infer_effects → escape → **CEmitter** (codegen).

**Промежуточного typed-IR НЕТ.** `struct IrExpr` никогда не создан. AST — единственный IR.
Codegen ходит по AST и ПЕРЕвыводит C-типы через `infer_expr_c_type` (**249 сайтов** в emit_c.rs).
Граница чекер→codegen = `ModuleEnv` side-tables (keyed by ExprId u32 / Span) + мутированный AST.

## 2. `resolved_types` — ПОЛНОТА: подмножество, дырявое by design

Тип: `HashMap<ExprId, ResolvedType>` (`types/mod.rs:874`). Два продюсера, оба частичные:
синтаксическое семя (9 литералов + примитивная арифм/As/turbofish-ctor) + семантический чекер
(Ident/Member/Index/Range/Call-chain/Match/...). **НЕ покрыто** ([M-104.10-expr-types-coverage]):
generic instance method-chain returns (`v.map(f).filter(g)` mid-chain), non-primitive TupleLit,
generic-instance RecordLit/container-element с несвязанным type-param; UNSET-узлы desugar.
Абсанс → codegen падает в legacy-fallback → потому `infer_expr_c_type` жив.

## 3. Мономорфизация — ЛЕНИВО в codegen

generic-type/method mono = `mono_worklist` дренится ВО ВРЕМЯ эмиссии (`emit_c.rs:1421`);
type-args через `current_type_subst`. Типы в `resolved_types` — **GENERIC-уровня**
(`ResolvedType::TypeParam(String)`); конкретизация поздняя/ленивая в `resolved_type_to_c:3203`.
⇒ **post-mono MIR несовместим с текущим порядком: mono надо ДВИГАТЬ РАНЬШЕ в явный пасс.**

## 4. `infer_expr_c_type` — доля работы

~2615 строк. Каналы 1-6 читают состояние чекера; Канал 6z = legacy match (44 арма+wildcard).
**По СЧЁТУ армов:** A(перевыражает известное)19 + B(нужна wire-аннотация)17 = **80% тривиальны**;
C(нужен настоящий mono/constraint-движок)7+mixed1 = 18%. **По СТРОКАМ обратное:** один class-C
арм `Call`→`infer_call_ret_c` = **2591 строка** (generic-method-return mono) = ~90% всего кода.
⇒ **арифметически большинство армов снимается таблицей, но почти весь КОД — это class-C
инференс (Call/closure/Vec-mono), которого в чекере СЕГОДНЯ НЕТ.**

## 5. Lossless: `ResolvedType` уже C-lossless

`resolved_types` несёт **`ResolvedType`** (богатый: module/effects/readonly/typed-ptr/fixed-array),
НЕ лоссовый TypeRef. `from_type_ref` не-инъективна (`[]T→Vec` :214, `Mut/Ref/Uninit` срез :263-281),
**НО для codegen-чтения это C-НЕРЕЛЕВАНТНО** (`[]T` и `Vec[T]` → один C; ref/mut ABI-прозрачны).
`resolved_type_to_c` (`emit_c.rs:3184`) УЖЕ и есть тонкое авторитетное окно C-лоуэринга.
Потеря бьёт ТОЛЬКО в checker-внутренний round-trip TypeRef↔Ty (chained receiver mangle).
⇒ **если MIR несёт `ResolvedType` end-to-end (не гоняет через TypeRef), P1 обходится структурно.**
**Новые узловые виды ради C-лоуэринга структурно НЕ нужны.**

## 6. Вердикт lite/full

**Убить `infer_expr_c_type` — в-lite ТЕХНИЧЕСКИ достаточно:** `resolved_type_to_c` уже тонкий,
`ResolvedType` уже C-lossless; блокер = (a) ПОЛНОТА таблицы + (b) отсутствующий class-C mono/
constraint-движок. Это дыры СПОСОБНОСТИ ЧЕКЕРА, не структуры IR.

**в-full (MIR+CFG) §0-целью НЕ требуется, но оправдан широкими целями (borrow-check/opts) —
при явном признании, что РАСШИРЯЕТ объём, а не сокращает трудное.** Даёт сверх lite: post-mono
явность (TypeParam исчезает), дом для class-C (запечь в узлы вместо 249 перевыводов), субстрат
borrow-check/SSA. **РЕШАЮЩИЙ ФАКТ: самая тяжёлая работа (class-C mono/constraint — Call/closure/
Vec-mono) ИДЕНТИЧНА под lite и full. MIR её НЕ уменьшает.** в-full ДОБАВЛЯЕТ: CFG с нуля +
переезд control-flow и effect/concurrency lowering (сегодня AST→C напрямую) на MIR + извлечение
mono в пре-пасс.

## (в-full)-специфика

- **Q1 mono:** сейчас ленивый в codegen. Post-mono MIR требует **извлечь mono в пре-MIR пасс** —
  первоклассная фаза-предусловие (reachability-worklist + mono_worklist — семя логики уже есть).
- **Q2 CFG:** **отсутствует полностью.** Control-flow лоуэрится AST→C-текст напрямую (emit_if/match/
  for/while, ранние return через `_nova_result`+`goto`). CFG строить С НУЛЯ. `return X→{res=X;goto L}`
  уже MIR-терминатор-формы, но живёт как строковая эмиссия.
- **Q3 effect/concurrency lowering:** spawn/detach/blocking/supervised/with/handler → AST→C напрямую
  (`emit_spawn:10372`, `emit_supervised:10908`, `emit_with:8957`, ...). Переплетённость с типами
  средне-высокая. **Под в-full ОБЯЗАНЫ переехать на MIR — материально расширяет объём.** Смягчение:
  MIR сперва держит их непрозрачными «runtime-call» стейтментами, уточнять позже.
- **Q4 borrow-check хук:** post-mono типизированный MIR+CFG, после typeck/mono. Прецедент для
  апгрейда: `escape_analyze.rs` (Plan 127, flow-нечувствительный V1 over-promote). MIR должен держать
  локалы явными биндингами со стабильными id + явные `&`/deref/move (не стирать в C-строки).

## 7. Риски

- **★ Byte-parity гейт vs MIR-флип (главный).** Весь 196 построен на per-phase byte-identical .c.
  MIR = тотальная замена лоуэринга, инкрементно byte-identical БЫТЬ НЕ МОЖЕТ. **Нужна НОВАЯ стратегия
  приёмки (conformance/поведенческая паритетность вместо текстовой) — заложить в переплан.**
- Class-C движок — long-pole, идентичен lite/full. `constraint_solver.rs` есть, но не авторитет
  (co-authority = верификатор-подмножество, 0 строк снимаемо).
- 249 сайтов `infer_expr_c_type` — механическая, но огромная миграция на `node.ty`.
- ExprId staleness: number_exprs ДО desugar; UNSET-узлы; `number_unset_exprs` вызов в compile-пути
  НЕ подтверждён. MIR post-desugar обязан переустановить идентичность синтетики.
- Второй потребитель AST: `interp/` (tree-walker) — вероятно вне scope.
- peer-files: number_exprs нумерует peer-items как разные Expr — MIR учесть дупликацию.

## Итог для переплана

Хребет верный (typed-carrier, codegen читает `node.ty`), но:
1. **Долгий шест = class-C движок + полнота таблицы — общий для lite и full.** С него и начинать.
2. **в-full = §0-хребет + ТРИ доп. воркстрима** (mono-пре-пасс, CFG-с-нуля, concurrency→MIR).
3. **Byte-parity гейт умирает — новая приёмка обязательна.**
