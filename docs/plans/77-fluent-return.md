# Plan 77: Fluent-return — точное «метод возвращает receiver»

> **Создан 2026-05-21.** Выделен из обсуждения Plan 73 (D131 `consume`).
>
> **✅ ЗАКРЫТ 2026-05-21 — выбран вариант B (`-> @`).** Реализовано:
> parser (`-> @` → `return_type = Self` + `FnDecl.returns_receiver`,
> instance-only check), check-pass `check_fluent_return` (тело
> non-external `-> @`-метода обязано завершаться `@` — делает гарантию
> проверяемой), consume-checker (`let x = recv.fluent()` → `x` алиас
> `recv` — закрывает builder-chain `[M-consume-method-result-alias]`),
> `runtime_registry::render_nv` (~22 stdlib builder'а `mut`-instance-Self
> → `-> @`; `@plus` с nova_body остаётся `-> Self`), spec D132. Codegen —
> `-> @` ≡ `-> Self` (тело возвращает `@`); авто-return-@ НЕ делался
> (codegen имеет несколько method-emission путей — риск; явный `@`
> консистентен с Rust `{ ...; self }`). 5 фикстур `nova_tests/plan77/`.
>
> Историческая часть документа (варианты A/B/C, сравнение) — ниже,
> сохранена как rationale.
>
> **Цель:** дать языку точный способ выразить «метод возвращает сам
> receiver» (а не просто значение того же типа). Нужно для:
> 1. sound alias-tracking в consume-checker (Plan 73 — builder-chain
>    `let sb2 = sb.append("x")`);
> 2. самодокументируемых fluent / builder API.

---

## Проблема

`Self` отвечает на вопрос «какой **ТИП**». Builder-методу (`append`,
`write_u32`, ...) нужно выразить «возвращает **тот же ОБЪЕКТ**, что
receiver» — для chaining (`sb.append("a").append("b")`).

Сейчас `mut @append(s str) -> Self` означает только «возвращает
StringBuilder». Что это **тот же** StringBuilder — лишь конвенция
builder-паттерна, не гарантия. Поэтому:

- consume-checker (D131) не может soundly считать `let sb2 = sb.append(...)`
  алиасом `sb` (если бы какой-то `-> Self`-метод вернул свежий объект —
  это был бы false positive). См. `simplifications.md`
  `[M-consume-method-result-alias]`.
- читатель / LLM не видит из сигнатуры, fluent ли метод.

## Как решено у других

| Язык | Механизм | Точность |
|---|---|---|
| **Rust** | `&mut self -> &mut Self` (заём self) ИЛИ `self -> Self` (owned builder, move) | ✅ точно — через borrow / move |
| **Go** | stdlib-билдеры **не** возвращают self (`b.WriteString(...)` — statement'ами) | вопрос обойдён, chaining'а нет |
| **TS** | тип `this` (`append(): this`) | ❌ «тот же тип», не объект (как наш `Self`) |

Rust здесь **точнее нас** — но ценой borrow-checker / move-модели.
Go обошёл вопрос отказом от chaining. TS — та же неточность, что у нас.

## Варианты

### Вариант A — гарантировать: `mut @m(...) -> Self` возвращает receiver

Спек-правило: метод с `mut`-receiver и return type `Self` **обязан**
вернуть receiver; компилятор это проверяет (body заканчивается `@` /
`return @`, иначе compile error).

- ➕ ноль новой грамматики; разом покрывает все ~25 builder-методов
  runtime stdlib без миграции; «мутирующий метод, возвращающий свой
  тип» — и так единственный осмысленный вариант (Rust `&mut Self`,
  Java `StringBuilder`).
- ➖ `-> Self` начинает значить разное при `mut` и без него — лёгкая
  контекстная зависимость (Nova ценит локальность контекста).

### Вариант B — новый синтаксис `-> @`

Тип возврата `@` означает «возвращает receiver»:
`fn StringBuilder mut @append(s str) -> @`.

- ➕ явно и самодокументируемо (важно для AI-first — fluent виден из
  сигнатуры); `Self` остаётся одним чистым понятием «тот же тип» без
  перегрузки; `@` уже значит receiver — расширение консистентно.
- ➖ новая грамматика (Nova ценит стабильность синтаксиса); миграция
  ~25 stdlib-методов; opt-in — забытый `-> Self` не покрыт.

### Вариант C — оставить как есть

- ➖ не production-grade: consume-checker неполон (builder-chain alias —
  честный пропуск); fluent держится на конвенции.

## Сравнительная таблица

| Критерий | A (гарантия) | B (`-> @`) | C (оставить) |
|---|---|---|---|
| Новая грамматика | нет | да | нет |
| Покрытие builder-методов | все (авто) | opt-in | — |
| Миграция stdlib | нет | ~25 методов | нет |
| `Self` остаётся одним понятием | нет | да | да |
| Fluent виден из сигнатуры | косвенно | да | нет |
| consume-checker builder-chain | sound | sound (где `-> @`) | пропуск |

## Что Nova может сделать лучше Go/Rust/TS

Точное «возвращает receiver» (A или B) даёт **безопасность уровня Rust
без borrows / lifetimes / move-модели**: компилятор знает, что
`sb.append("x")` — это `sb`. Rust получает это через borrow-checker
(когнитивный налог ownership), Go/TS — не получают вообще. Формула
Nova — **безопасность Rust + простота Go**.

**Потенциал «лучше Rust»:** `consume` (D131) сейчас *affine* (≤1 раз).
В паре с fluent-return открывается *linear* «must-consume» — значение,
которое ОБЯЗАНО быть consumed до конца scope (например `Transaction`
обязан `.commit()` или `.rollback()`). Rust такое не enforce'ит
(`#[must_use]` — лишь warning). Оформлено как [Plan 100](100-linear-must-consume.md)
(D133, proposed 2026-05-23).

## Рекомендация

Авторский lean (на обсуждении Plan 73) — **вариант B (`-> @`)**: для
AI-first языка «видно из сигнатуры» важнее экономии токена, и `Self`
не перегружается. Вариант A — легитимная лёгкая альтернатива. Решение
за автором языка.

## Что разблокирует

- Plan 73 `[M-consume-method-result-alias]` — sound alias через
  builder-chain.
- `nova doc` — пометка fluent-методов.
- [Plan 100](100-linear-must-consume.md) — linear `must-consume`
  (D133, proposed 2026-05-23; см. §«Потенциал лучше Rust» выше).

## Фазы (после выбора варианта; ориентир для B)

- **Ф.1** — spec: D-блок (резерв **D132**) — `-> @` / fluent-return,
  rationale, граница с `Self`.
- **Ф.2** — lexer/parser: `@` в позиции return-type (`-> @`); AST —
  пометка `FnDecl.returns_receiver` (bool).
- **Ф.3** — type-checker: `-> @` валиден только на instance-receiver;
  тип результата = receiver-тип; проверка тела (для Nova-impl методов).
- **Ф.4** — codegen: метод возвращает receiver-pointer (для external —
  уже так; для Nova-impl — авто-`return @`).
- **Ф.5** — runtime_registry: флаг `returns_receiver`, render `-> @`;
  миграция ~25 stdlib builder-методов `-> Self` → `-> @`.
- **Ф.6** — consume-checker: `let x = recv.method()` где метод —
  `-> @` → `x` alias `recv` (закрывает `[M-consume-method-result-alias]`).
- **Ф.7** — тесты (позитивные fluent + негативные) + spec sync.

## Ссылки

- Plan 73 (`73-consume-qualifier.md`) — D131 `consume`, источник задачи.
- `simplifications.md` → `[M-consume-method-result-alias]`.
- spec/decisions/02-types.md → D66 (`Self` — referential type).
