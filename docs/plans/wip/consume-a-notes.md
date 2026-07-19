# Consume-волна А — рабочие заметки (checkpoint)

Маркер: `[M-d180-consume-propagation-match-payload-mut-rebind]`
(`docs/plans/backlog-followups.md` §P1). Решение владельца 2026-07-18:
вариант А (enforce по букве). Worktree: `d:/Sources/nv-lang/nova-consumeA`,
ветка `p-consume-enforce-a` (от `main` @ `b2bfa0505`).

## Статус: исследование завершено, начинаю реализацию (checker + parser)

## Корневая причина (подтверждено чтением кода)

1. **`consume_walk_expr` / `ExprKind::Match` и `ExprKind::IfLet`**
   (`compiler-codegen/src/types/mod.rs`, около строк 30114-30152) —
   pattern-биндинги арма регистрируются ГОЛО:
   ```rust
   let mut names = Vec::new();
   consume_pattern_names(&arm.pattern, &mut names);
   for n in &names { ctx.declare(n, None); }
   ```
   Тип `None` → `ctx.var_types` не получает запись → `ctx.consume_obligations`
   не получает запись → ВСЯ D180/D133-дисциплина ниже по потоку молчит.
   Это ровно совпадает с диагнозом маркера (б).

2. **Rule 2 (E_VIEW_BINDING_FORBIDDEN)** уже существует в
   `consume_walk_stmt`/`Stmt::Let` (строка ~28133) и вычисляет
   `alias_obligated` из `ctx.consume_obligations.contains(canon)` ИЛИ
   `ctx.var_types.get(canon)` = consume-тип. Как только (1) чинится —
   `mut stream = tcp` (алиас pattern-bound `tcp`) автоматически подхватывается
   ЭТИМ уже существующим кодом — отдельный фикс для (в) НЕ нужен, это
   downstream-симптом (б), не отдельный баг. Подтверждает разбор маркера
   один-в-один.

3. **`Pattern::Ident` (`compiler-codegen/src/ast/mod.rs:3081`) не имеет
   `is_consume`-флага** (только `is_mut`, Plan 108.3). `parse_pattern()`
   (`compiler-codegen/src/parser/mod.rs:11097`) читает ТОЛЬКО `mut`-префикс
   (`pat_is_mut`), `consume` внутри pattern НЕ парсится вообще. Синтаксис
   `Some(consume f)` из D157-спеки (строка 771) — **никогда не был
   реализован** (D157 header: "proposed; implementation pending"). Нужно
   реализовать: `is_consume: bool` на `Pattern::Ident`, парсер —
   symmetric to `is_mut`.

4. **D184 (`if consume Pat = e` → `E_CONSUME_IN_CONDITION`,
   parser/mod.rs:9584)** — это ДРУГОЙ, уже существующий и НЕ трогаемый
   запрет: top-level `consume`-префикс перед ВСЕЙ scrutinee-конструкцией
   в if/while condition-position. Новый sub-pattern `consume` (на
   ОДНОМ биндинге внутри `Ok(consume x)`) — ортогонален, другая позиция
   в грамматике, НЕ конфликтует. D157-й пример `if ro consume Some(t) = opt`
   (топ-левел consume перед scrutinee в if-let) уже противоречит D184 —
   существовавшая нестыковка ДО этой волны, не трогаю (не входит в мандат).

5. **Инфраструктура для unwrap Ok/Some-inner типа УЖЕ ЕСТЬ**
   (D86-followup, 2026-07-14): `ConsumeRegistry::unwrapped_fn_return_types`
   / `unwrapped_method_return_types` + `ConsumeCtx::infer_unwrapped_call_type`
   — именно то, что нужно для rvalue-скрутини (`TcpStream.connect(...)`
   как `Call`). Планирую переиспользовать НАПРЯМУЮ.
   Для place-скрутини (голый `Ident`) — такой карты по var нет
   (`var_types` хранит только плоское имя типа без generic-аргументов) →
   добавляю новую параллельную карту `var_unwrapped_types: HashMap<String,String>`,
   заполняемую (a) из explicit-annotation `Option[T]`/`Result[T,E]` на
   let/param через уже существующий `unwrap_result_option_name()` helper
   (строка 25472, уже применим напрямую) и (b) из RHS через
   `infer_unwrapped_call_type` при declare.

## План изменений (реализация)

### A. AST (`compiler-codegen/src/ast/mod.rs`)
- `Pattern::Ident { name, span, is_mut, is_consume }`.
- Blast radius: 132 упоминаний `Pattern::Ident` в 17 файлах; construction
  sites (не `{ .. }`-match) — насчитано ~25 (callnorm.rs x2, emit_c.rs x3,
  may_gc.rs x2, desugar.rs x8, parser/mod.rs x3, protocols/auto_derive.rs x3,
  прочие). Все синтетические — `is_consume: false`.

### B. Парсер (`compiler-codegen/src/parser/mod.rs`)
- `parse_pattern()`: `let pat_is_consume = self.eat(&TokenKind::KwConsume).is_some();`
  симметрично `pat_is_mut`; `Ok(consume tcp)` — вложенный sub-pattern
  внутри `Variant::Tuple`. `consume`+`mut` на одном pattern-биндинге —
  parse error (симметрия D131 «взаимоисключающие», уже отражено для
  let-level).

### C. Чекер (`compiler-codegen/src/types/mod.rs`)
- Новое поле `ConsumeCtx::var_unwrapped_types: HashMap<String,String>`.
- Заполнение при Stmt::Let (explicit annotation + RHS unwrap inference)
  и при регистрации fn-параметров.
- Новый helper `scrutinee_unwrapped_type(ctx, scrutinee) -> Option<String>`
  (Ident → var_unwrapped_types; иначе → infer_unwrapped_call_type).
- `ExprKind::Match`/`ExprKind::IfLet` arm-обработка: для pattern
  `Variant{path: [.., "Ok"|"Some"], kind: Tuple{patterns:[Ident], rest:false}}`
  — если unwrapped-тип must-consume (`lin_reg.consume_types`):
  - `is_consume=false` → `E_CONSUME_PATTERN_REQUIRED` (machine-applicable
    suggestion insert `consume `), биндинг declare как обычно (НЕ
    obligated — симметрия существующего Rule1-паттерна: ошибка
    эмитится, cascade не создаём).
  - `is_consume=true` → `declare_consume_binding` + `local_mut=true`
    (consume уже mut-capable, симметрия D180).
  - НЕ must-consume payload — поведение как раньше (view), НО теперь
    var_types получает точный тип (было `None`) — попутно улучшает
    method-dispatch/diagnostics для payload-биндингов (не regression).
- Область: ТОЛЬКО Ok/Some (D156 явно про Option/Result); Err-payload
  consume (если E тоже must-consume) — honest defer, нет карты
  unwrapped-Err-типа сегодня; отметить в отчёте как follow-up.
- `mut X = consume_var` / `ro X = consume_var` — Rule 2 уже существует и
  сработает автоматически после фикса (а), отдельного кода не пишу,
  только проверяю тестом.

### D. Спека (`spec/decisions/05-memory.md`)
- D157-амендмент: rvalue/place-скрутини must-consume payload требует
  `consume`-sub-pattern; новый код `E_CONSUME_PATTERN_REQUIRED`.
- D180-амендмент: Rule 2 подтверждена для pattern-bound consume-значений;
  D156-пропагация через Option/Result enforced.
- Дата 2026-07-19, кросс-ссылки D131/D133/D156/D157/D180/D184,
  «решение владельца 2026-07-18 (вариант А; philosophy visible ownership
  transfer)».

## Дальше по плану (не начато на момент этого чекпоинта)
- Фикстуры pos/neg (spec_tests/conformance).
- Миграция examples/tls, examples/net, examples/flagship/aggregator,
  spec_tests/conformance, std/**.
- nova-tls / nova-http worktree + миграция + тесты.
- Финальная приёмка (standalone-CU 69/0, флагман --strict-effects).

## Сетевые обрывы
Несколько сетевых/авторизационных обрывов за сессию — по инструкции
коммичу чекпоинт-заметки мелкими шагами перед каждым логическим шагом.
Никакого кода ещё не менялось на момент ЭТОГО коммита (только чтение +
разведка) — это первый чекпоинт, фиксирующий план перед началом правок.
