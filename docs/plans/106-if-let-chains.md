\ SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 106 — `if let`/`while let` chains через запятую (let-chains)

> **Создан:** 2026-05-27. **Статус:** 📋 proposed, **P2**.
> **Источник:** drift discovery 2026-05-27 — spec задокументировал
> chain-форму, parser падает с `expected '{', got ','`.
> **Spec:** [03-syntax.md:1163-1182](../../spec/decisions/03-syntax.md#L1163-L1182)
> (грамматика `if-expr := "if" if-cond ("," if-cond)* block`),
> [syntax.md:771](../../spec/syntax.md#L771).
> **Зависит от:** ничего (изолированный фикс, симметричный
> [Plan 105](105-sum-type-explicit-base.md)).

---

## 1. Проблема

Спека утверждает что `if let` и `while let` поддерживают цепочку
условий через запятую (Rust let-chains):

```nova
\ несколько условий через запятую
if Some(user) = lookup(id), user.is_active {
    process(user)
}

\ смешанные ro + bool-cond
if Some(a) = lookup_a(), ro Some(b) = lookup_b(a.id), b.valid {
    handle(a, b)
}

\ while-форма
while Some(item) = queue.pop(), !item.poisoned {
    process(item)
}
```

Парсер видит запятую и обрывается:

```
tmp:11:35: error: expected `{`, got `,`
  11 |     if let Some(user) = lookup(id), user.is_active {
     |                                   ^
```

Это критично для idiomatic кода — типичный «get-and-validate» сейчас
требует вложенного `if`:

```nova
\ Workaround сейчас (некрасиво)
if Some(user) = lookup(id) {
    if user.is_active {
        process(user)
    }
}
```

## 2. Решение

Расширить AST/parser/type-checker/codegen чтобы `if`/`while`
принимали **список if-cond'ов** вместо одного:

```rust
\ AST до (compiler-codegen/src/ast/mod.rs:1316-1327)
IfLet  { pattern: Pattern, scrutinee: Expr, then: Block, else_: ... }
While  { cond: Expr, body: Block, ... }
WhileLet { pattern: Pattern, scrutinee: Expr, body: Block, ... }

\ AST после — унифицированная chain-форма
enum IfCond {
    LetBinding { pattern: Pattern, scrutinee: Expr },
    BoolExpr(Expr),
}

If    { conds: Vec<IfCond>, then: Block, else_: ... }
While { conds: Vec<IfCond>, body: Block, ... }
```

`IfLet`/`WhileLet` AST-варианты **удаляются** — поглощаются унифицированными
`If`/`While` с одним let-cond'ом в `conds`. Это снимает дублирование
парсера/codegen'а в двух местах ([emit_c.rs:15263](../../compiler-codegen/src/codegen/emit_c.rs#L15263)
и [15325](../../compiler-codegen/src/codegen/emit_c.rs#L15325)).

Codegen: chain эмитится как **left-to-right short-circuit вложенные `if`'ы**:

```c
\ Nova: if let Some(u) = lookup(id), u.is_active { body }
{
    Option_User scr1 = Nova_lookup(id);
    if (scr1.tag == TAG_Some) {
        User u = scr1.value.Some;
        if (Nova_User_is_active(&u)) {
            /* body */
        }
    }
}
```

Биндинги из let-cond'а видны в **следующих** cond'ах chain'а **и** в
теле блока (как в Rust let-chains).

## 3. Декомпозиция (фазы)

| Ф. | Что | Acceptance |
|---|---|---|
| **Ф.0** | GATE: `nova_tests/syntax/if_let_chain_probe.nv` + `while_let_chain_probe.nv` — 6 форм (single let, let+bool, let+let, 3-cond mix, while-chain, else-if-chain). Baseline: 6/6 FAIL parse. | 6/6 FAIL зафиксировано |
| **Ф.1** | AST: ввести `enum IfCond { LetBinding{...}, BoolExpr(Expr) }`; заменить `IfLet`/`WhileLet` на унифицированные `If { conds: Vec<IfCond> }` / `While { conds: Vec<IfCond> }` ([ast/mod.rs:1316-1375](../../compiler-codegen/src/ast/mod.rs#L1316-L1375)). Все call-sites обновлены. | `cargo check` чистый |
| **Ф.2** | Parser: `parse_if()` ([parser/mod.rs:5459](../../compiler-codegen/src/parser/mod.rs#L5459)) — цикл по `,`-разделённым cond'ам через helper `parse_if_cond()`. То же для `parse_while()` ([5681](../../compiler-codegen/src/parser/mod.rs#L5681)). | Probe Ф.0 → 6/6 PASS parse; existing if-let-tests без регрессий |
| **Ф.3** | Type-checker: scope для каждого cond'а — биндинги let-cond'а видны последующим cond'ам и блоку. Обновить `walk_expr()` ([types/mod.rs:5343-5357](../../compiler-codegen/src/types/mod.rs#L5343-L5357)) и consume-analysis ([8710-8722](../../compiler-codegen/src/types/mod.rs#L8710-L8722)). Negative-фикстура `if let Some(x) = e1, let Some(y) = e2 { use(x_undefined_in_e1_scope) }` — error. | Биндинги корректно резолвятся в pos-кейсах; неправильное использование вне scope → loud err |
| **Ф.4** | Codegen: chain → nested `if` в [emit_c.rs](../../compiler-codegen/src/codegen/emit_c.rs). Удалить duplicated WhileLet path ([15325-15349](../../compiler-codegen/src/codegen/emit_c.rs#L15325-L15349)) — поглощается унифицированным While с `Vec<IfCond>`. | `nova test` зелёный; chain-тесты PASS; sizeof emitted-C проверяется через `expected_stdout` |
| **Ф.5** | Positive-фикстуры: 6 файлов в `nova_tests/syntax/` (формы из Ф.0 + два real-world examples из stdlib-стиля). Negative: `if_let_chain_scope_violation.nv` (биндинг из cond[1] used в cond[0]) + `if_let_chain_extra_comma.nv` (trailing comma). | 6 pos + 2 neg PASS; both backends (clang + MSVC) |
| **Ф.6** | Spec close + closure: marker `[M-if-let-chain-parser-gap]` ✅ ЗАКРЫТО в [docs/simplifications.md](../simplifications.md). Spec amend [03-syntax.md:1163-1182](../../spec/decisions/03-syntax.md#L1163-L1182) — добавить sub-section «Scope rules» (порядок binding visibility в chain). Обновить project-creation.txt + discussion-log.md. | Marker ✅; spec amend ссылается на Plan 106 |

**Total:** ~2 dev-day (изолированный, без зависимостей; AST-refactor немного дольше Plan 105).

## 4. Acceptance (master)

Plan 106 = ✅ ЗАКРЫТ когда:

- [ ] Все 3 формы из spec'а работают: single (existing), comma-chain `let, bool`, comma-chain `let, let, bool`.
- [ ] То же для `while let` (симметрия с `if let`).
- [ ] Биндинги корректно scope'ятся: cond[i] видит биндинги cond[0..i], body видит все.
- [ ] Short-circuit семантика: cond[i+1] не вычисляется если cond[i] провалился.
- [ ] Regression 0: все существующие if-let / while-let тесты без chain'а работают как раньше.
- [ ] AST-унификация: `IfLet`/`WhileLet` варианты удалены, заменены `If`/`While` с `Vec<IfCond>`.
- [ ] Spec ↔ impl drift ликвидирован: marker `[M-if-let-chain-parser-gap]` ✅.
- [ ] Memory: `project-plan106-status.md` создан.

## 5. Что вне scope

- **`match` guards через запятую** (`match x { Some(n) if n > 0, condition2 => ... }`) — match guard'ы уже есть с `if`-формой, comma-chain в match — отдельный вопрос (если вообще нужен).
- **Refutable let в обычном выражении** (`let Some(x) = e else { return }` Rust 1.65 let-else) — completely другой конструкт, отдельный план.
- **Pattern guards внутри cond'а** (`if let Some(n) if n > 0 = ...`) — overlap с match-arm-guard, специально не вводится для простоты.

## 6. Связь

- [spec/decisions/03-syntax.md §«if/while let»](../../spec/decisions/03-syntax.md#L1163-L1182) — формальная грамматика.
- [Plan 105](105-sum-type-explicit-base.md) — параллельный drift-fix (тот же class:
  spec задокументировал, parser silently бьёт).
- **Rust precedent:** [RFC 2497 let-chains](https://rust-lang.github.io/rfcs/2497-if-let-chains.html),
  стабилизировано в Rust 1.65. Nova повторяет семантику.
