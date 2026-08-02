<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Nova terminology glossary (RU ↔ EN)

> **Purpose.** This is a **working dictionary for translators**, not a
> specification page — it is **informative**, not normative. It exists so
> that the parallel translation batches of Plan 241 (`spec/*.en.md`
> ru→en, `docs/guide/*.ru.md` en→ru) use **one consistent vocabulary**
> instead of each batch inventing its own English or Russian phrasing for
> the same Nova concept.
>
> **Sourcing rule.** Russian terms come from the normative spec pages
> (`spec/overview.md`, `spec/paradigm.md`, `spec/syntax.md`,
> `spec/effects.md`, `spec/conversions.md`, `spec/revolutionary.md`) and
> from `spec/decisions/README.md` section headings. English equivalents
> come **first** from actual usage in `docs/guide/*.md` (the English
> guide files already have settled vocabulary — *fiber*, *effect row*,
> *record*, *protocol*, and so on) — the glossary must match how the
> guide already talks, not invent new phrasing. Where no English
> precedent exists anywhere in the repo, a term is proposed and marked
> **`[proposed]`** for the owner to confirm or correct.
>
> Nova keyword/identifier tokens (`consume`, `ro`, `use`, `spawn`,
> `requires`, …) are **never translated** — they are code, not prose —
> and are listed separately with a Russian explanation of what the token
> means, not a translation of the token itself.
>
> Open disagreements and gaps are collected in
> [Open questions for owner review](#open-questions-for-owner-review) at
> the end of this file.

---

## 1. Philosophy and effects paradigm · Философия и парадигма эффектов

| Русский | English | Example (en) | Note |
|---|---|---|---|
| алгебраические эффекты | algebraic effects | "Algebraic effects + handlers (Koka, Effekt, Eff)" | central language idea; see spec/overview.md "Что заимствует у кого" |
| эффект | effect | "Network, disk, the clock … in Nova these are all **effects**." | language-tour.md §6 |
| хендлер / обработчик эффекта | handler | "Each effect has a **handler** that intercepts its operations" | language-tour.md §6; also the literal value produced by `effect X { ... }` |
| AI-first дизайн | AI-first design | "Nova — first language explicitly optimized for the pair 'LLM writes, human reviews'" | spec/overview.md "Killer use-case" |
| killer use-case | killer use-case | "**Killer use-case.** AI-first programming." | spec/overview.md heading, borrowed English term used as-is in the Russian original |
| одна дверь (единственный канонический путь) | single canonical path / "no second door" `[proposed]` | "not a second door to `?`, but an independent niche" (paraphrase of the retraction rationale) | idiom used repeatedly in spec/decisions (e.g. D86 amend: "она была второй дверью к `?`") to reject a duplicate way of doing something already covered; no settled English phrase exists yet in guide/spec — owner to confirm wording |
| скрутини (объект сопоставления в match) | scrutinee `[proposed]` | "the scrutinee of a `match` expression is the value being matched against its arms" | standard PL term (Rust/Haskell usage); not yet attested anywhere in Nova's own docs — guide/spec just say "the value being matched" |
| эффект-строка | effect row | "`Fail[E]`, `Fail` — стандартный эффект — **в effect-row сигнатуры**" | spec/overview.md; English term is already borrowed as-is into the Russian original, no translation needed |
| структурная типизация | structural typing | "структурная типизация + вывод типов везде" (spec/overview.md, поддерживающие решения) | contrasted with nominal typing throughout `protocol` discussion |
| capability security | capability security | "Capability security" — spec/overview.md "Что заимствует у кого" table (source: E, Pony) | borrowed English term, used as-is |

---

## 2. Keywords and identifiers — not translated · Ключевые слова — код, не переводить

These are language tokens, not prose — every translation keeps them verbatim
(same spelling in the en and ru text) and, where useful, glosses the meaning
in the surrounding sentence. The "Note" column below is the Russian gloss of
what the token does, not a translation of the token.

| Token (code, unchanged in both languages) | Example (en) | Note (что значит по-русски) |
|---|---|---|
| `consume` | "A `consume`-typed binding is ownership-tracked." | параметр/binding, привязка владения ресурсом; исчерпывается ровно один раз (или через `@cleanup`, D432) |
| `ro` | "`ro` declares a read-only binding (never reassigned)" | read-only связывание/параметр; синоним `readonly` для параметров |
| `mut` | "`mut` declares a reassignable one [binding]" | разрешает мутацию/переприсваивание — на binding, параметре или поле |
| `use` (embed) | "`use account Account` — embed: field + auto-proxy methods" | встраивание типа как поля с автопрокси методов (композиция, не наследование) |
| `spawn` | "`spawn` inside a `supervised` block starts a fire-and-forget fiber" | запуск нового fiber'а внутри structured-concurrency скоупа |
| `supervised` | "`supervised(deadline:)` gives that block a shared deadline" | скоуп, собирающий свои `spawn`-файберы и их падения/дедлайн |
| `detach` | "`detach { body }` — fire-and-forget task surviving the caller" | файбер, переживающий вызывающий скоуп (вне structured-дисциплины) |
| `parallel for` | "`parallel for` fans out homogeneous work and collects results into a `[]T`" | параллельный fan-out цикла с ожиданием всех и отменой хвоста при ошибке |
| `with` | "`with Db = postgres_handler { ... }`" | установка handler'а эффекта на скоуп |
| `effect` (keyword) | "`type X effect { ... }`" (declaration) / "`ro console = effect Logger { ... }`" (literal) | и kind-токен объявления эффект-типа, и ключевое слово handler-литерала |
| `protocol` | "`protocol` declares a structural interface" | структурный контракт для значений (в отличие от `effect` — контракт для операций) |
| `requires` / `ensures` / `invariant` / `decreases` | "`requires amount > 0`", "`ensures result >= 0`" | контрактные клозы: предусловие / постусловие / инвариант цикла / метрика терминации |
| `defer` | "`defer { ... }` runs at scope exit, LIFO" | отложенный вызов при выходе из скоупа |
| `forbid` | "`forbid Net, Fs, Db { eval(code) }`" | capability-режим: запрет вызова функций с перечисленными эффектами внутри блока |

---

## 3. Types and data · Типы и данные

| Русский | English | Example (en) | Note |
|---|---|---|---|
| record | record | "`type X { ... }` declares a **record** — a heap-allocated, GC-managed reference type." | language-tour.md §2; `{}` braces, reference semantics |
| sum-тип | sum type | "A **sum type** requires the `enum` marker (`type X enum A \| B \| C`)" | language-tour.md §2; D406 |
| enum-маркер | `enum` marker | "the `enum` marker is mandatory (D406); leading `\|` alone is not valid syntax anymore" | language-tour.md §2 |
| позиционный кортеж | positional tuple | "`type X(T1, T2)` — positional tuple — stack — value (copy on pass)" | docs/guide/value-vs-reference.md bracket-rule table |
| именованный кортеж | named tuple | "`type Vec3(x f64, y f64, z f64)` — .x / .y / .z access" | docs/guide/value-vs-reference.md |
| value-запись | value record | "iterator value-records: `VecIter[T] value`" | spec/decisions/02-types.md D228/D290; `type X value { ... }` — stack-allocated record, structural `==` |
| value-семантика | value semantics | "Geometric primitives (Point, Rect, AABB) — named tuple — value semantics" | docs/guide/value-vs-reference.md "When to use which" |
| newtype | newtype | "Newtype (`type X Y`, without `alias`) is a **separate** type from the source" | spec/conversions.md "Newtype ↔ underlying" |
| alias | alias | "`type X alias Y` — там `X` и `Y` взаимозаменяемы без всякого cast'а" | spec/conversions.md; alias = same type, not a distinct one |
| protocol | protocol | "`protocol` declares a structural interface; `#impl(...)` opts a type into one explicitly" | language-tour.md §3; structural by default, nominal on demand |
| дженерик / параметр типа | generic type parameter | "`[T]` on a function introduces a generic type parameter." | language-tour.md §2 |
| generic bound | generic bound | "`fn dedup[T Hash](xs []T) -> []T`" | spec/syntax.md "Generic bounds — `[T Protocol]` или `[T TypeSet]`" |
| type-set | type-set | "**Type-set** — a named set of concrete types, listed explicitly" (paraphrase of spec/syntax.md D310) | closed membership-list bound, opposite of a structural `protocol` bound |
| мономорфизация | monomorphization | "Performance, traits, мономорфизация" — spec/overview.md "Что заимствует у кого" (source: Rust) | default dispatch strategy; zero-cost, opposite of `dyn` |
| dyn-диспатч | dynamic dispatch (`dyn`) | "`dyn` — only when explicit runtime polymorphism is needed" (paraphrase of spec/paradigm.md "vtable-вызов") | vtable call, opt-in via `dyn Trait`/`dyn Protocol` |

---

## 4. Bindings, ownership and pattern matching · Связывания, владение, сопоставление с образцом

| Русский | English | Example (en) | Note |
|---|---|---|---|
| consume-тип | consume-type | "A *consume-type* is a type whose values represent ownership of a non-shareable resource" | docs/guide/consume-types.md |
| owner (владелец ресурса) | owner | "the owner must explicitly consume them [values]" | docs/guide/consume-types.md |
| линейная дисциплина | linear (discipline) | "it had to be consumed **exactly once**, or the compiler rejected the program — strictly linear" | language-tour.md §7, D133 |
| аффинная дисциплина | affine (discipline) | "**D432** lets a `consume` type opt into an **affine** discipline instead" | language-tour.md §7; may-forget instead of must-consume |
| view-заём | view-borrow | "Function parameters of consume-type without the `consume` keyword are *views* — bounded by the callee's scope" | docs/guide/consume-types.md Rule 4 |
| перемещение (move) | move | "`consume b = a` — move — a dead, b owns" | docs/guide/consume-types.md Rule 3 |
| use-after-consume | use-after-consume | "using it [a consumed binding] triggers a `use-after-consume` diagnostic" | docs/guide/consume-types.md |
| fluent-цепочка | fluent chain | "Fluent chains compose mutators: `sb.append(\"a\").append(\"b\").as_str()`" | docs/guide/consume-types.md "Fluent-return chains" |
| ресивер | receiver | "a method call (`obj.method()` …), receiver as the first argument" | docs/guide/contracts.md "Pure function composition" |
| property-по-арности | property by arity | "**Properties by arity** (D84/D409) let one name serve as both getter and setter" | language-tour.md §3 |
| pattern matching / сопоставление с образцом | pattern matching | "`match` supports literal patterns, guards, and sum-variant destructuring." | language-tour.md §4 |
| guard | guard | "guards (`n if n > 0`)" | language-tour.md §4; extra boolean condition on a match arm |
| ветвь (match arm) | match arm | "Each arm has the form `pattern => result`" (paraphrase of spec/syntax.md "Каждая arm имеет форму") | spec/syntax.md "Pattern matching" |
| исчерпывающая проверка | exhaustiveness check | "**Exhaustiveness check.** The compiler checks that `match` covers all possible cases." (paraphrase of spec/syntax.md) | spec/syntax.md "Pattern matching" |
| if-let форма | if-let form | "`if <Pattern> = expr { } else { }` is Nova's if-let form" | language-tour.md §4; no separate `if let` keyword, same `if pattern = …` shape |
| встраивание (embed) | embed | "embed: имя поля обязательно (D39)" → "`use` — это **поле + автопрокси методов**" (spec/syntax.md) | composition via `use Type`, not inheritance |
| делегирование | delegation | "`use Account` is **delegation**, not inheritance: the compiler generates proxy methods" (paraphrase of spec/paradigm.md) | spec/paradigm.md "Вместо наследования — embed + delegate" |

---

## 5. Effects and error handling · Эффекты и обработка ошибок

| Русский | English | Example (en) | Note |
|---|---|---|---|
| подмена handler'а (через with) | handler substitution | "Each effect has a **handler** that intercepts its operations, substituted via `with Handler = ...`" | language-tour.md §6 |
| прямой эффект | direct effect | "A function declares in its signature exactly which effects **it itself** performs" (paraphrase, language-tour.md §6) | spec/effects.md "Прямые эффекты, не транзитивные" (D28) |
| транзитивный эффект | transitive effect | "calling another function does not pull that function's effects up into the caller's signature" | language-tour.md §6; warning by default, hard error under `--strict-effects` |
| строгий режим эффектов | `--strict-effects` (strict-effects mode) | "programs (`examples/**`) build under `--strict-effects` … an experimental flag that promotes undeclared-transitive-effect … warnings to hard errors" | language-tour.md §6; Plan 197 |
| стандартный эффект | standard effect | "`Io`, `Net`, `Db`, `Fs`, `Time`, `Random`, `Log`, `Trace` … — стандартные эффекты" | spec/overview.md "Зарезервированные identifier'ы" table |
| эффект Fail | `Fail` effect | "`Fail[E]` — эффект-контракт для перехвата и обработки ошибки" | spec/effects.md "Роли — throw / Fail[E] / handler" |
| бросить ошибку (throw) | throw | "`throw err` — language syntax, raises an error" (paraphrase of spec/effects.md "Роли") | never resumes at the throw point; `never` operation type |
| паника | panic | "**panic** is for a broken caller contract … and is never recoverable" | language-tour.md §5 |
| return-стиль (`?`) | return-style (`?`) | "`expr?` — return-style: 'didn't work — wrap it upward as a value'" (paraphrase of spec/effects.md) | spec/effects.md "Операторы `?` и `!!`" |
| throw-стиль (`!!`) | throw-style (`!!`) | "`expr!!` — throw-style: 'didn't work — throw via `Fail`'" (paraphrase of spec/effects.md) | spec/effects.md "Операторы `?` и `!!`" |
| capability-режим | capability mode | "Capability-режим для безопасной композиции" → `forbid Net, Fs, Db { ... }` | spec/revolutionary.md R6 heading |
| дефолтный handler | default handler | "Some effects (`Time` is the canonical example) work **without an explicit `with`**" (paraphrase, D431) | spec/effects.md "Дефолтный handler без with" |

---

## 6. Memory and performance · Память и производительность

| Русский | English | Example (en) | Note |
|---|---|---|---|
| managed heap (управляемая куча) | managed heap | "`o is a pointer to managed heap; GC-tracked`" (paraphrase, docs/guide/value-vs-reference.md) | GC-tracked reference-type storage, default for records/sum types |
| escape-анализ | escape analysis | "Go — escape analysis decides" (docs/guide/value-vs-reference.md comparison table); "не утекающие значения остаются на стеке" (spec/overview.md) | compiler decides stack vs heap automatically, no programmer annotation |
| регион (arena) | region | "Arena-allocations через `region { }` — проектируемая форма (D6), ⚠ в текущем компиляторе не реализована" | spec/syntax.md "Производительность"; opt-in real-time memory, **not yet implemented** |
| real-time зона / `#realtime nogc` | real-time zone / `#realtime nogc` | "For real-time зон (звук, торговля, embedded) — атрибут `#realtime nogc fn`" (spec/overview.md, paraphrased) | marks a function as GC-forbidden for hard real-time code paths |
| стек-аллокация | stack allocation | "positional tuple — **stack** — value (copy on pass)" | docs/guide/value-vs-reference.md bracket-rule table |

---

## 7. Concurrency — Vela runtime · Конкурентность — рантайм Vela

| Русский | English | Example (en) | Note |
|---|---|---|---|
| файбер (fiber) | fiber | "Under the hood — **fiber-based scheduler** (like Go/OCaml 5)." | spec/effects.md "Async — невидимая инфраструктура"; ~4-8 KB stack, millions per machine |
| structured concurrency | structured concurrency | "concurrency is structured, not a separate async dialect" | language-tour.md §8 |
| supervision (супервизия) | supervision | "Supervision of failures is an ordinary effect `Supervisor`" (paraphrase, spec/overview.md D416) | Erlang/OTP-style child-failure policy: `escalate()` / `stop()` |
| дедлайн скоупа | (scope) deadline | "`supervised(deadline:)` gives that block a shared deadline, and a spawn that misses it is genuinely cancelled" | language-tour.md §8 |
| отмена (cancellation) | cancellation | "Cancellation — structured" (docs/guide comparison, spec/revolutionary.md R7 table row) | vs. manual cancellation in classic async runtimes |
| capability-split (канал) | capability-split | "The model is **capability-split** (Rust mpsc-style): `Channel.new(cap)` returns a **pair**" | docs/guide/channels.md; separates send-only/receive-only capabilities |
| select | `select` | "`select { ... }` is multiplexed channel operations: it waits on several recv/send operations at once" | docs/guide/channels.md |
| gorутина-эквивалент (fan-out) | fan-out | "`parallel for` fans out homogeneous work and collects results into a `[]T` in order." | language-tour.md §8 |
| data race freedom (свобода от гонок) | data race freedom | heading of spec/decisions/06-concurrency.md D415: "Data race freedom — `#share`-атрибут, capture-check, consume в spawn" | compiler-enforced boundary rules for `mut` captures crossing fiber boundaries |
| планировщик work-stealing | work-stealing scheduler | listed in spec/decisions/README.md §06 topic summary: "…work-stealing scheduler, preemption" | M:N scheduler backing `spawn`/`supervised`/`detach` |

---
