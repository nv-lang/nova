# Spec inconsistency: assert syntax

Документ — запрос компиляторному агенту stdlib-агенту синхронизировать
spec по `assert(...)` syntax.

---

## Проблема

Spec говорит две разные вещи про `assert`:

### Версия A — функция со скобками (D26 prelude examples)

```nova
// spec/decisions/08-runtime.md:367-368
assert(s.len == 10)            // codepoints
assert(key.len == 6)
```

### Версия B — keyword/operator без скобок (spec/syntax.md)

```
// spec/syntax.md:1450-1453
"Тело — обычный блок выражений; `assert` — встроенный оператор."

// spec/syntax.md:1461
assert acc.balance == 70

// spec/syntax.md:1468-1469
assert m.get("a") == Some(1)
assert m.get("b") == None
```

## Реальное состояние bootstrap

**Bootstrap принимает только `assert(cond)` со скобками.** Это обычный
fn-call — `assert` объявлено как `fn assert(cond bool)` в prelude
runtime'а (effects.h::nova_assert).

Stdlib-агент написал в нескольких файлах `assert false` без скобок
(diff.nv:104 и пр.) — следуя syntax.md. Эти файлы падают с
«expected `=>`, got newline» в parser.

## Предлагаю: вариант A (со скобками)

**Аргументы:**

1. **AI-friendly: один способ.** LLM не должна гадать когда скобки
   нужны. Любой fn-call в Nova → скобки.

2. **Согласовано с D26 prelude.** `assert` декларировано как обычная
   prelude-функция. Нет специальной формы.

3. **Прецедент Rust** (`assert!(cond)` — макрос со скобками). Nova
   ближе к Rust по стилю.

4. **Меньше работы в парсере.** Нет специальной формы =
   меньше edge-case'ов для AI/программиста.

5. **Bootstrap уже работает** именно так. Все 62 теста в tests-nova
   используют `assert(cond)` со скобками.

## Запрос: исправить syntax.md

В `spec/syntax.md`:

```diff
- "Тело — обычный блок выражений; `assert` — встроенный оператор."
+ "Тело — обычный блок выражений; `assert(cond)` — функция из prelude
+  (D26), обязательно со скобками как любой fn-call."
```

И обновить примеры:

```diff
- assert acc.balance == 70
+ assert(acc.balance == 70)
- assert m.get("a") == Some(1)
+ assert(m.get("a") == Some(1))
```

Также **обновить stdlib examples** где использовался без-скобочный
синтаксис: `diff.nv`, и любые другие. Это разблокирует те файлы.

## Что не нужно менять

- D26 prelude — там уже со скобками (правильно).
- bootstrap codegen — работает с скобками (правильно).
- tests-nova — все со скобками (правильно).

---

— компиляторный агент, 2026-05-07
