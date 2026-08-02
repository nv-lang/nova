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
| AI-first дизайн / killer use-case | AI-first design / killer use-case | "Nova — first language explicitly optimized for the pair 'LLM writes, human reviews'" | spec/overview.md "Killer use-case" heading — "killer use-case" itself is a borrowed English term used as-is in the Russian original |
| одна дверь (единственный канонический путь) | single canonical path / "no second door" `[proposed]` | "not a second door to `?`, but an independent niche" (paraphrase of the retraction rationale) | idiom used repeatedly in spec/decisions (e.g. D86 amend: "она была второй дверью к `?`") to reject a duplicate way of doing something already covered; no settled English phrase exists yet in guide/spec — owner to confirm wording |
| скрутини (объект сопоставления в match) | scrutinee `[proposed]` | "the scrutinee of a `match` expression is the value being matched against its arms" | standard PL term (Rust/Haskell usage); not yet attested anywhere in Nova's own docs — guide/spec just say "the value being matched" |
| эффект-строка | effect row | "`Fail[E]`, `Fail` — стандартный эффект — **в effect-row сигнатуры**" | spec/overview.md; English term is already borrowed as-is into the Russian original, no translation needed |

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
| кортеж (позиционный / именованный) | tuple (positional / named) | "`type X(T1, T2)` — positional tuple" / "`type Vec3(x f64, y f64, z f64)` — named tuple, .x/.y/.z access" | docs/guide/value-vs-reference.md bracket-rule table; both stack-allocated, value semantics |
| value-запись | value record | "iterator value-records: `VecIter[T] value`" | spec/decisions/02-types.md D228/D290; `type X value { ... }` — stack-allocated record with structural `==`, value semantics |
| newtype | newtype | "Newtype (`type X Y`, without `alias`) is a **separate** type from the source" | spec/conversions.md "Newtype ↔ underlying" |
| alias | alias | "`type X alias Y` — там `X` и `Y` взаимозаменяемы без всякого cast'а" | spec/conversions.md; alias = same type, not a distinct one |
| protocol | protocol | "`protocol` declares a structural interface; `#impl(...)` opts a type into one explicitly" | language-tour.md §3; structural by default, nominal on demand |
| дженерик / параметр типа | generic type parameter | "`[T]` on a function introduces a generic type parameter." | language-tour.md §2 |
| generic bound (protocol / type-set) | generic bound | "`fn dedup[T Hash](xs []T) -> []T`" | spec/syntax.md "Generic bounds — `[T Protocol]` или `[T TypeSet]`"; a `type-set` bound is a closed list of concrete types (membership), not structural |
| мономорфизация / dyn-диспатч | monomorphization / dynamic dispatch (`dyn`) | "Performance, traits, мономорфизация" (spec/overview.md, source: Rust); "`dyn` — only when explicit runtime polymorphism is needed" (paraphrase of spec/paradigm.md "vtable-вызов") | monomorphization is the zero-cost default; `dyn Trait`/`dyn Protocol` opts into a vtable call |

---

## 4. Bindings, ownership and pattern matching · Связывания, владение, сопоставление с образцом

| Русский | English | Example (en) | Note |
|---|---|---|---|
| consume-тип | consume-type | "A *consume-type* is a type whose values represent ownership of a non-shareable resource" | docs/guide/consume-types.md; the owner must explicitly consume the value, no implicit drop |
| линейная дисциплина | linear (discipline) | "it had to be consumed **exactly once**, or the compiler rejected the program — strictly linear" | language-tour.md §7, D133 |
| аффинная дисциплина | affine (discipline) | "**D432** lets a `consume` type opt into an **affine** discipline instead" | language-tour.md §7; may-forget instead of must-consume |
| view-заём | view-borrow | "Function parameters of consume-type without the `consume` keyword are *views* — bounded by the callee's scope" | docs/guide/consume-types.md Rule 4 |
| перемещение (move) | move | "`consume b = a` — move — a dead, b owns; using `a` afterward triggers a `use-after-consume` diagnostic" | docs/guide/consume-types.md Rule 3 |
| fluent-цепочка / ресивер | fluent chain / receiver | "Fluent chains compose mutators: `sb.append(\"a\").append(\"b\").as_str()`" | docs/guide/consume-types.md "Fluent-return chains"; receiver = the `self`-like first argument a method is called on |
| property-по-арности | property by arity | "**Properties by arity** (D84/D409) let one name serve as both getter and setter" | language-tour.md §3 |
| pattern matching / сопоставление с образцом (+ guard) | pattern matching (+ guard) | "`match` supports literal patterns, guards (`n if n > 0`), and sum-variant destructuring." | language-tour.md §4; a guard is an extra boolean condition on a match arm; exhaustiveness is checked for sum types and `bool` |
| if-let форма | if-let form | "`if <Pattern> = expr { } else { }` is Nova's if-let form" | language-tour.md §4; no separate `if let` keyword, same `if pattern = …` shape |
| встраивание (embed) / делегирование | embed / delegation | "embed: имя поля обязательно (D39)" → "`use` — это **поле + автопрокси методов**" (spec/syntax.md) | composition via `use Type`, not inheritance — the compiler generates proxy methods (delegation), no virtual dispatch |

---

## 5. Effects and error handling · Эффекты и обработка ошибок

| Русский | English | Example (en) | Note |
|---|---|---|---|
| подмена handler'а (через with) | handler substitution | "Each effect has a **handler** that intercepts its operations, substituted via `with Handler = ...`" | language-tour.md §6 |
| прямой / транзитивный эффект | direct / transitive effect | "A function declares in its signature exactly which effects **it itself** performs; calling another function does not pull that function's effects up" | language-tour.md §6; spec/effects.md "Прямые эффекты, не транзитивные" (D28) — transitive is a warning by default, a hard error under `--strict-effects` |
| строгий режим эффектов | `--strict-effects` (strict-effects mode) | "programs (`examples/**`) build under `--strict-effects` … an experimental flag that promotes undeclared-transitive-effect … warnings to hard errors" | language-tour.md §6; Plan 197 |
| эффект Fail | `Fail` effect | "`Fail[E]` — эффект-контракт для перехвата и обработки ошибки" | spec/effects.md "Роли — throw / Fail[E] / handler" |
| бросить ошибку (throw) | throw | "`throw err` — language syntax, raises an error" (paraphrase of spec/effects.md "Роли") | never resumes at the throw point; `never` operation type |
| паника | panic | "**panic** is for a broken caller contract … and is never recoverable" | language-tour.md §5 |
| постфиксные операторы `?` / `!!` | postfix operators `?` / `!!` | "`expr?` — return-style … `expr!!` — throw-style: 'didn't work — throw via `Fail`'" (paraphrase of spec/effects.md) | spec/effects.md "Операторы `?` и `!!`" — programmer picks the handling style at the use site |

---

## 6. Memory and performance · Память и производительность

| Русский | English | Example (en) | Note |
|---|---|---|---|
| managed heap (управляемая куча) | managed heap | "`o is a pointer to managed heap; GC-tracked`" (paraphrase, docs/guide/value-vs-reference.md) | GC-tracked reference-type storage, default for records/sum types |
| escape-анализ | escape analysis | "Go — escape analysis decides" (docs/guide/value-vs-reference.md comparison table); "не утекающие значения остаются на стеке" (spec/overview.md) | compiler decides stack vs heap automatically, no programmer annotation |
| регион (arena) / real-time зона (`#realtime nogc`) | region / real-time zone (`#realtime nogc`) | "Arena-allocations через `region { }` — проектируемая форма (D6), ⚠ в текущем компиляторе не реализована"; "For real-time зон (звук, торговля, embedded) — атрибут `#realtime nogc fn`" | spec/syntax.md "Производительность"; `region` is opt-in arena memory, **not yet implemented**; `#realtime nogc` marks a function as GC-forbidden for hard real-time code paths |
| стек-аллокация | stack allocation | "positional tuple — **stack** — value (copy on pass)" | docs/guide/value-vs-reference.md bracket-rule table |

---

## 7. Concurrency — Vela runtime · Конкурентность — рантайм Vela

| Русский | English | Example (en) | Note |
|---|---|---|---|
| файбер (fiber) | fiber | "Under the hood — **fiber-based scheduler** (like Go/OCaml 5)." | spec/effects.md "Async — невидимая инфраструктура"; ~4-8 KB stack, millions per machine |
| structured concurrency | structured concurrency | "concurrency is structured, not a separate async dialect" | language-tour.md §8 |
| supervision (супервизия) | supervision | "Supervision of failures is an ordinary effect `Supervisor`" (paraphrase, spec/overview.md D416) | Erlang/OTP-style child-failure policy: `escalate()` / `stop()` |
| дедлайн скоупа / отмена | (scope) deadline / cancellation | "`supervised(deadline:)` gives that block a shared deadline, and a spawn that misses it is genuinely cancelled" | language-tour.md §8; structured cancellation, unlike manual cancellation in classic async runtimes |
| capability-split (канал) | capability-split | "The model is **capability-split** (Rust mpsc-style): `Channel.new(cap)` returns a **pair**" | docs/guide/channels.md; separates send-only/receive-only capabilities |
| select | `select` | "`select { ... }` is multiplexed channel operations: it waits on several recv/send operations at once" | docs/guide/channels.md |
| data race freedom (свобода от гонок) | data race freedom | heading of spec/decisions/06-concurrency.md D415: "Data race freedom — `#share`-атрибут, capture-check, consume в spawn" | compiler-enforced boundary rules for `mut` captures crossing fiber boundaries |

---

## 8. Modules and packages · Модули и пакеты

| Русский | English | Example (en) | Note |
|---|---|---|---|
| модуль | module | "A **module** is either a single file `X.nv` or a **folder** `X/`" | language-tour.md §11 |
| папка-модуль / peer-файлы | folder-module / peer files | "A **module** is either a single file `X.nv` or a **folder** `X/` whose **peer files** all declare the same `module` path and share one namespace" | language-tour.md §11 |
| пакет | package | "Every import path is fully qualified from the **package** root (the directory with `nova.toml`)" | language-tour.md §11 |
| workspace (воркспейс) | workspace | "Workspaces (`[workspace] members = [...]`) group several packages in a monorepo" | language-tour.md §11 |

---

## 9. Runtime, FFI and unsafe · Рантайм, FFI и unsafe

| Русский | English | Example (en) | Note |
|---|---|---|---|
| непрозрачный указатель | opaque pointer | "Nova's opaque-pointer type is `*()` (pointer to unit — `void*` in C)" | language-tour.md §12 |
| типизированный хэндл | typed handle | "Wrap a raw `*()` in a record for a **typed handle** so distinct native resources … aren't interchangeable at compile time" | language-tour.md §12 |
| внешняя функция | `external fn` | "`external fn name(args) -> ret` (D82) declares a binding to a C symbol" | language-tour.md §12 |
| unsafe-блок / модель мутабельности указателя | `unsafe` block / pointer-mutability model | heading of docs/guide/typed-pointers.md: "Typed pointers (`*T` family) + `unsafe` model", "Pointer-mutability model: 'arrow → box'" | `unsafe` is a scoped escape hatch for raw-pointer operations (Plan 138.5) |

---

## 10. Tooling and contracts · Тулинг и контракты

| Русский | English | Example (en) | Note |
|---|---|---|---|
| контракт (+ SMT-солвер) | contract (+ SMT solver) | "Nova's contract system lets you state what a function **requires** and **ensures**, then verifies those claims at compile time via an SMT solver." | docs/guide/contracts.md intro |
| enforce-с-elision (доказано → вырезано) | enforce-with-elision | "Nova uses **enforce-with-elision** (D24 / Plan 140), *not* debug-only asserts" | docs/guide/contracts.md intro |
| доказанный / недоказанный контракт | proven / unproven (contract) | "a **proven** contract is elided (zero runtime cost, even in debug); an **unproven** one is enforced at runtime in **both debug and release**" | docs/guide/contracts.md intro |
| лемма | lemma | "A **lemma** is a `#verify` function whose purpose is to establish a mathematical fact" | docs/guide/contracts.md "Lemmas and apply" |
| постусловие/предусловие | postcondition / precondition | "`requires` — A precondition." / "`ensures` and `result` — A postcondition." | docs/guide/contracts.md |
| клоз decreases (доказательство терминации) | `decreases` clause | "`decreases` — Proves termination of recursive functions." | docs/guide/contracts.md "decreases" |

---

## 11. Conversions and overloading · Конверсии и перегрузка

| Русский | English | Example (en) | Note |
|---|---|---|---|
| приведение as | `as` cast | "`as` — infallible numeric/newtype/sum cast, compile-time, no runtime code" (paraphrase of spec/conversions.md "Три механизма") | spec/conversions.md |
| расширение / сужение (widening / narrowing) | widening / narrowing | "Widening (no precision loss)" / "Narrowing (potential precision loss)" | spec/conversions.md "Numeric ↔ numeric" |
| проверяемое сужение | checked narrowing | "Checked narrowing — `try_to_*` (D430, 2026-07-20)" | spec/conversions.md heading |
| неявная конверсия #coerce | `#coerce` (zero-cost implicit conversion) | "`#coerce` on a **unary** function declares an **implicit** conversion `I → O`, inserted by the compiler in a position with a known expected type" (paraphrase of spec/conversions.md) | spec/conversions.md "Zero-cost неявные конверсии" (D429) |
| конвенция имени (from/try_from) | naming convention (`from`/`try_from`) | "these are three independent naming conventions, each an ordinary Nova function with no protocol behind it" (paraphrase of spec/conversions.md) | spec/conversions.md "Именование from/try_from — конвенция, не протокол"; `From`/`Into`/`TryFrom`/`TryInto` protocols retracted 2026-07-06 |
| потребляющая передача владения | consuming ownership transfer (`consume @into_*`) | "`consume @into_ЦЕЛЬ()` — a consuming transfer of ownership (a concrete name on the source)" (paraphrase of spec/conversions.md "Три механизма" table) | spec/conversions.md |

---

## Open questions for owner review

1. **«Одна дверь» (§1).** No settled English phrase exists anywhere in
   `docs/guide/*.md` or `spec/*.md` for this recurring design idiom
   ("don't add a second way to do something the language already covers
   one way"). Proposed: **"single canonical path"** or, closer to the
   Russian door-metaphor, **"no second door"**. Needs owner sign-off
   before `spec/*.en.md` translators start using it — it will recur
   often (D86 amend, D429 §"третья дверь", nv-coding-style "запрещённая
   пятая дверь", etc.).
2. **«Скрутини» (§1).** Not attested anywhere in Nova's own docs — the
   guide and spec both just say "the value being matched" / "паттерн
   совпал". Proposed **"scrutinee"** is standard Rust/Haskell PL jargon,
   which may be *more* precise than Nova's own house style wants for an
   AI-first, plain-language project. Owner should decide: adopt
   "scrutinee" as the technical term, or keep the descriptive phrasing
   and drop this glossary entry as unnecessary.
3. **`spec/paradigm.md` is stale.** Its own header (added 2026-xx) flags
   it as describing a pre-D18/D24/D31/D33-D42/D52/D53/D61-D66/D70/D73
   version of the language — it still talks about `trait`/`impl`,
   which are retired in favor of `protocol` + effect-via-kind-token.
   This glossary does **not** carry `trait`/`impl` forward as current
   terminology; if a translator hits `paradigm.md` directly (out of
   scope for Ф.1 per the plan's normative-file list, but flagging just
   in case), the file needs a rewrite pass before translation, not just
   a translation of stale content.
4. **Term count vs. plan target.** The plan asks for "~50-80 pairs";
   this glossary lands at exactly **80** table rows across 11 sections
   (including the 14-row keyword table, §2). Several closely related
   concepts were deliberately merged into one row (e.g. positional/named
   tuple, widening/narrowing, deadline/cancellation) to stay inside the
   range while still naming every concept from the plan's starter list
   plus the additional spec/guide sourcing pass. If the owner wants any
   of these split back into separate rows for clarity, that's a
   low-cost follow-up edit, not a re-sourcing effort.
5. **`ro` vs `readonly`.** `docs/guide/parameters.md` documents `readonly`
   as an explicit synonym for the default (no-modifier) parameter mode,
   while `ro` is the binding-declaration keyword (`ro x = ...`). Both
   ended up in §2's `ro` row's note as one explanation; flag if the
   owner wants them split into two distinct glossary rows since they are
   technically two different keywords with overlapping meaning.
