# Spec inconsistency: `assert expr` vs `assert(expr)`

Поднял по ходу твоей просьбы поправить `assert false` в diff.nv.
Не блокер — фикс уже в `89119709a`. Но заметил **расхождение в spec'е**,
которое стоит закрыть до того, как stdlib-агент будет массово править
существующие .nv-файлы.

---

## Что нашёл

| Источник | Форма | Пример |
|---|---|---|
| `spec/revolutionary.md:65` | без скобок | `assert buf == ["processing 42"]` |
| `spec/syntax.md:1050,1461,1468,1469` | без скобок | `assert m.get("a") == Some(1)` |
| `spec/decisions/06-concurrency.md:1551,1554,1557` | **со скобками** | `assert(Time.now() == 42)` |
| `spec/decisions/08-runtime.md:367` | **со скобками** | `assert(s.len == 10)` |
| `tests-nova/**/*.nv` (bootstrap) | **со скобками** | `assert(ch.capacity() == 8)` |
| `examples/stdlib/**/*.nv` | **без скобок** (большинство) | `assert m.len() == 100` |
| Парсер bootstrap | **требует скобки** | `assert false` → parse error |

То есть:
- **Старая часть spec'и** (revolutionary, syntax) — без скобок (как Rust
  `assert!`).
- **Новая часть spec'и** (decisions/06, 08) и **реализация** — со скобками
  (как функция).
- **examples/stdlib** написан в старом стиле (без скобок) — отвалится
  массово как только stdlib-suite будет компилироваться.

## Вопрос

Какая форма канонична?

### Вариант A. `assert(expr)` со скобками (текущая реализация)

- Pro: однозначно — это **обычный вызов функции** в prelude
  (`fn assert(cond bool) Fail[AssertFailed]`).
- Pro: согласовано с парсером — не нужны спецправила.
- Pro: совпадает с D26 prelude tail (если бы `assert` был обычной
  функцией).
- Con: ~150+ asserts в `examples/stdlib/` придётся переписать.

### Вариант B. `assert expr` без скобок (старый spec)

- Pro: keyword-style как в Rust (`assert!`) — короче, читабельнее в
  тестах.
- Pro: `examples/stdlib/` уже написан так.
- Con: парсер придётся расширить (assert как keyword, не как call).
- Con: спецслучай — для одного оператора отдельная грамматика.

### Вариант C. Поддерживать обе формы

- Con: «два способа сделать одно и то же» нарушает D9/D40.
- Con: в spec'е тогда нужно явно обозначить эквивалентность.

## Предложение

**Канонизировать вариант A** (`assert(expr)` со скобками — функция в
prelude, не keyword). Это согласуется с реализацией компилятора и с
D40 «один путь». Старый spec (revolutionary, syntax) переписать
точечно. examples/stdlib — мой sweep.

Если согласен — я делаю двумя коммитами:
1. spec sweep (`revolutionary.md`, `syntax.md`) → `assert(expr)`.
2. `examples/stdlib/**/*.nv` sweep — все asserts со скобками.

Если хочешь вариант B (keyword-форма) — скажи, я подстрою stdlib и
тебе уйдёт парсерская задача (extend assert как keyword).

Не критично — diff.nv разблокирован. Решение нужно до полного sweep'а
stdlib (которого пока не делаю).

—

— stdlib-агент, 2026-05-07
