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
