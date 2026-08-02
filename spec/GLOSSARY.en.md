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
> precedent existed anywhere in the repo, the term was proposed and
> decided by the owner on 2026-08-03 — all such rows now carry the
> decision in their Note column; no `[proposed]` rows remain.
>
> Nova keyword/identifier tokens (`consume`, `ro`, `use`, `spawn`,
> `requires`, …) are **never translated** — they are code, not prose —
> and are listed separately with a Russian explanation of what the token
> means, not a translation of the token itself.
>
> **Minimal-English-words norm (owner, 2026-08-03; doc-conventions
> `#language`).** Russian prose keeps anglicisms to a minimum: not
> «роутер/хендлер/консюмить/капчурить», but «маршрутизатор/обработчик/
> потреблять/захватывать». Every term row below carries a **Russian
> prose form** fit for ru-translations of the guide under this norm —
> not a transliterated calque. Where the Russian column below still
> shows an untranslated or transliterated English word, it is tagged
> **`[keep-en: reason]`** — one word for *why* it stays English: `код`
> (it names an actual Nova keyword/identifier embedded in the phrase),
> `аббревиатура` (an acronym with no Russian expansion in use — SMT, FFI,
> SMT-solver-as-name), `идиома` (a fixed English phrase the spec itself
> borrows verbatim, e.g. "killer use-case"), or `термин` (a specialized
> PL/CS phrase with no established, concise Russian translation in the
> field — e.g. "escape analysis" — distinct from an already-naturalized
> single loanword like «эффект», which needs no tag at all). Fully naturalized
> Russian loanwords that read as ordinary Russian vocabulary today
> («эффект», «протокол», «паника», «дисциплина», «кортеж») are **not**
> tagged — they are the Russian form already, same as «эффект» in
> «побочный эффект». A row with no `[keep-en]` tag means the Russian
> column is already the norm-compliant prose form to use in
> translations. Section 2
> (code keywords) is `[keep-en: код]` for every row by definition — see
> its own note instead of repeating the tag 14 times.
>
> **Status (2026-08-03): all owner questions are closed.** The section
> [Open questions for owner review](#open-questions-for-owner-review) is
> kept as the decision log — every entry records what the owner decided
> and why, so later translators do not reopen a settled term. New
> disagreements found during translation batches are appended there.

---

## 1. Philosophy and effects paradigm · Философия и парадигма эффектов

| Русский (норма #language) | English | Example (en) | Note |
|---|---|---|---|
| алгебраические эффекты | algebraic effects | "Algebraic effects + handlers (Koka, Effekt, Eff)" | central language idea; see spec/overview.md "Что заимствует у кого"; «алгебраические» — обычное русское прилагательное, не калька |
| эффект | effect | "Network, disk, the clock … in Nova these are all **effects**." | language-tour.md §6; «эффект» — давно натурализованное русское слово (как в «побочный эффект»), не тег-калька и не жаргон — тег `[keep-en]` не нужен |
| обработчик (эффекта) | handler | "Each effect has a **handler** that intercepts its operations" | language-tour.md §6; норма-форма — «обработчик», НЕ «хендлер» (жаргонная транслитерация, в прозу не пускать) |
| проектирование с приоритетом ИИ | AI-first design | "Nova — first language explicitly optimized for the pair 'LLM writes, human reviews'" | spec/overview.md "Killer use-case"; «AI-first» переведено полностью («с приоритетом ИИ»), калек не осталось |
| убойный сценарий использования `[keep-en: идиома]` (killer use-case) | killer use-case | "**Killer use-case.** AI-first programming." | spec/overview.md heading; «killer use-case» — устойчивая англ. идиома, спека сама заимствует её без перевода в норматив — keep-en; описательный русский вариант приведён для первого упоминания в прозе |
| одна дверь (единственный канонический путь) | "no second door" | "not a second door to `?`, but an independent niche" (paraphrase of the retraction rationale) | idiom used repeatedly in spec/decisions (e.g. D86 amend: "она была второй дверью к `?`"); **утверждено владельцем 2026-08-03** — en-форма "no second door" (дверная метафора сохранена); описательное "single canonical path" допустимо как пояснение при первом упоминании |
| сопоставляемое значение (объект `match`) | scrutinee | "the scrutinee of a `match` expression is the value being matched against its arms" | **утверждено владельцем 2026-08-03**: ru — «сопоставляемое значение» (транслит «скрутини» в новую прозу не пускать; существующие D-блоки не переписываем), en — scrutinee (стандартный PL-термин) |
| эффект-строка | effect row | "`Fail[E]`, `Fail` — стандартный эффект — **в effect-row сигнатуры**" | spec/overview.md; составное «эффект»+«строка» — оба слова русские, калька смысла (не транслитерация), уже кодифицирована в норме spec — оставляем как есть |

---

## 2. Keywords and identifiers — not translated · Ключевые слова — код, не переводить

These are language tokens, not prose — every translation keeps them verbatim
(same spelling in the en and ru text) and, where useful, glosses the meaning
in the surrounding sentence. The "Note" column below is the Russian gloss of
what the token does, not a translation of the token. **Every row in this
section is `[keep-en: код]` by definition** — a code keyword is not prose
and is never translated; the tag is not repeated per row. The Russian
glosses themselves also follow the `#language` norm (обработчик, не
хендлер; область видимости, не скоуп) except for «файбер», left as-is
pending the owner's call in Open questions.

| Token (code, unchanged in both languages) | Example (en) | Note (что значит по-русски) |
|---|---|---|
| `consume` | "A `consume`-typed binding is ownership-tracked." | параметр/связывание, привязка владения ресурсом; исчерпывается ровно один раз (или через `@cleanup`, D432) |
| `ro` | "`ro` declares a read-only binding (never reassigned)" | связывание/параметр только для чтения; **утверждено владельцем 2026-08-03: использовать `ro`** — это сокращение от `readonly` (полная форма остаётся синонимом для параметров); в прозе при первом упоминании: «`ro` (сокращение от readonly)» |
| `mut` | "`mut` declares a reassignable one [binding]" | разрешает мутацию/переприсваивание — на связывании, параметре или поле |
| `use` (embed) | "`use account Account` — embed: field + auto-proxy methods" | встраивание типа как поля с автопрокси методов (композиция, не наследование) |
| `spawn` | "`spawn` inside a `supervised` block starts a fire-and-forget fiber" | запуск нового файбера внутри области видимости structured concurrency |
| `supervised` | "`supervised(deadline:)` gives that block a shared deadline" | область видимости, собирающая свои `spawn`-файберы и их падения/дедлайн |
| `detach` | "`detach { body }` — fire-and-forget task surviving the caller" | файбер, переживающий вызывающую область видимости (вне дисциплины structured concurrency) |
| `parallel for` | "`parallel for` fans out homogeneous work and collects results into a `[]T`" | параллельный разлёт (fan-out) цикла с ожиданием всех и отменой хвоста при ошибке |
| `with` | "`with Db = postgres_handler { ... }`" | установка обработчика эффекта на область видимости |
| `effect` (keyword) | "`type X effect { ... }`" (declaration) / "`ro console = effect Logger { ... }`" (literal) | и kind-токен объявления эффект-типа, и ключевое слово литерала-обработчика |
| `protocol` | "`protocol` declares a structural interface" | структурный контракт для значений (в отличие от `effect` — контракт для операций) |
| `requires` | "`requires amount > 0`" | контрактный клоз-предусловие функции |
| `ensures` | "`ensures result >= 0`" | контрактный клоз-постусловие функции |
| `invariant` | "`invariant sum >= 0`" (inside a `while` loop) | контрактный клоз — инвариант цикла (условие, верное на каждом входе в тело) |
| `decreases` | "`decreases n`" | контрактный клоз — метрика терминации (доказательство, что рекурсия/цикл завершится) |
| `defer` | "`defer { ... }` runs at scope exit, LIFO" | отложенный вызов при выходе из области видимости |
| `forbid` | "`forbid Net, Fs, Db { eval(code) }`" | режим полномочий: запрет вызова функций с перечисленными эффектами внутри блока |

---

## 3. Types and data · Типы и данные

| Русский (норма #language) | English | Example (en) | Note |
|---|---|---|---|
| запись | record | "`type X { ... }` declares a **record** — a heap-allocated, GC-managed reference type." | language-tour.md §2; `{}` braces, reference semantics; «запись» — стандартный русский CS-термин (как Pascal `record` = «запись»), не калька |
| тип-сумма (sum-тип) | sum type | "A **sum type** requires the `enum` marker (`type X enum A \| B \| C`)" | language-tour.md §2; D406; **утверждено владельцем 2026-08-03** — в новой ru-прозе «тип-сумма»; гибрид «sum-тип» в существующей спеке не переписывается (прецедент «скрутини») |
| маркер `enum` `[keep-en: код]` | `enum` marker | "the `enum` marker is mandatory (D406); leading `\|` alone is not valid syntax anymore" | language-tour.md §2; `enum` — буквальное ключевое слово Nova внутри фразы |
| позиционный кортеж | positional tuple | "`type X(T1, T2)` — positional tuple — stack — value (copy on pass)" | docs/guide/value-vs-reference.md bracket-rule table; доступ к полям по индексу (`.0`/`.1`); «кортеж» — стандартный русский матем./CS-термин |
| именованный кортеж | named tuple | "`type Vec3(x f64, y f64, z f64)` — named tuple, .x/.y/.z access" | docs/guide/value-vs-reference.md; доступ к полям по имени, но то же представление в стеке, что и у позиционного кортежа |
| value-запись `[keep-en: код]` | value record | "iterator value-records: `VecIter[T] value`" | spec/decisions/02-types.md D228/D290; `value` — буквальное ключевое слово Nova (`type X value { ... }`), «запись» уже по-русски |
| тип-обёртка (newtype) | newtype | "Newtype (`type X Y`, without `alias`) is a **separate** type from the source" | spec/conversions.md "Newtype ↔ underlying"; **утверждено владельцем 2026-08-03** — «тип-обёртка», при первом упоминании на странице «тип-обёртка (newtype)»; в русских переводах Rust/Haskell термин обычно не переводят — Nova вводит русскую форму по норме #language |
| псевдоним (alias) | alias | "`type X alias Y` — там `X` и `Y` взаимозаменяемы без всякого cast'а" | spec/conversions.md; «псевдоним» — стандартный русский перевод (используется, напр., для `alias` в других языках), не калька |
| протокол | protocol | "`protocol` declares a structural interface; `#impl(...)` opts a type into one explicitly" | language-tour.md §3; structural by default, nominal on demand; «протокол» — натурализованное русское слово |
| параметр типа (обобщённый) | generic type parameter | "`[T]` on a function introduces a generic type parameter." | language-tour.md §2; норма-форма «обобщённый» вместо жаргонного «дженерик» |
| ограничение типового параметра через `protocol` `[keep-en: код]` | generic bound (protocol) | "`fn dedup[T Hash](xs []T) -> []T`" | spec/syntax.md "Generic bounds — `[T Protocol]`"; структурное ограничение — подходит любой тип с нужными методами; `protocol` — ключевое слово |
| ограничение типового параметра через `type-set` `[keep-en: код]` | generic bound (type-set) | "`[T TypeSet]`" (paraphrase of spec/syntax.md "Type-set — bound по членству, не по структуре") | spec/syntax.md; `type-set` = закрытый список конкретных типов (проверка по членству), не структурный, в отличие от `protocol`-ограничения; `type-set` — конкретная Nova-конструкция |
| мономорфизация | monomorphization | "Performance, traits, мономорфизация" (spec/overview.md, source: Rust) | «мономорфизация» — натурализованный CS-термин, дефолтная (нулевой стоимости) стратегия диспетчеризации в Nova |
| диспетчеризация `dyn` `[keep-en: код]` | dynamic dispatch (`dyn`) | "`dyn` — only when explicit runtime polymorphism is needed" (paraphrase of spec/paradigm.md "vtable-вызов") | норма-форма «диспетчеризация» вместо жаргонного «диспатч»; `dyn` — ключевое слово Nova (`dyn Trait`/`dyn Protocol`), опциональная альтернатива мономорфизации |

---

## 4. Bindings, ownership and pattern matching · Связывания, владение, сопоставление с образцом

| Русский (норма #language) | English | Example (en) | Note |
|---|---|---|---|
| потребляемый тип `[keep-en: код]` (consume-тип) | consume-type | "A *consume-type* is a type whose values represent ownership of a non-shareable resource" | docs/guide/consume-types.md; норма-глагол «потреблять» (не «консьюмить») даёт «потребляемый тип»; `consume` оставлен в скобках как код-имя declaration-модификатора |
| линейная дисциплина | linear (discipline) | "it had to be consumed **exactly once**, or the compiler rejected the program — strictly linear" | language-tour.md §7, D133; натурализованный термин теории типов, не калька |
| аффинная дисциплина | affine (discipline) | "**D432** lets a `consume` type opt into an **affine** discipline instead" | language-tour.md §7; may-forget instead of must-consume; натурализованный термин теории типов |
| заём-представление | view-borrow | "Function parameters of consume-type without the `consume` keyword are *views* — bounded by the callee's scope" | docs/guide/consume-types.md Rule 4; «заём» — устоявшийся русский перевод Rust-термина «borrow», «представление» — для «view»; составное — не калька, оба слова русские |
| перемещение | move | "`consume b = a` — move — a dead, b owns; using `a` afterward triggers a `use-after-consume` diagnostic" | docs/guide/consume-types.md Rule 3; «перемещение» — устоявшийся русский перевод move-семантики (Rust-литература) |
| цепочка вызовов | fluent chain | "Fluent chains compose mutators: `sb.append(\"a\").append(\"b\").as_str()`" | docs/guide/consume-types.md "Fluent-return chains"; последовательность вызовов, каждый из которых возвращает получателя (`-> @`) |
| получатель | receiver | "receiver as the first argument a method is called on" (paraphrase, docs/guide/contracts.md) | «получатель» вместо жаргонного «ресивер» — объект вызова метода (аналог `self`) |
| свойство по арности | property by arity | "**Properties by arity** (D84/D409) let one name serve as both getter and setter" | language-tour.md §3; «арность» — стандартный русский матем./CS-термин, не калька |
| сопоставление с образцом | pattern matching | "`match` supports literal patterns, guards, and sum-variant destructuring." | language-tour.md §4; «сопоставление с образцом» — устоявшийся русский перевод в переводной ФП-литературе (Haskell/OCaml) |
| охранное условие | guard | "guards (`n if n > 0`)" | language-tour.md §4; норма-форма вместо непереведённого «guard» — дополнительное булево условие на ветви `match` |
| условная форма сопоставления | if-let form | "`if <Pattern> = expr { } else { }` is Nova's if-let form" | language-tour.md §4; описательная замена англ. идиомы «if-let» (в Nova нет отдельного ключевого слова `if let` — это форма обычного `if <паттерн> = выражение`) |
| встраивание | embed | "embed: имя поля обязательно (D39)" → "`use` — это **поле + автопрокси методов**" (spec/syntax.md) | composition via `use Type` (см. `use` в §2), not inheritance; «встраивание» натурализовано в русской OOP-литературе |
| делегирование | delegation | "the compiler generates proxy methods (delegation), no virtual dispatch" (paraphrase, spec/paradigm.md) | результат встраивания — компилятор генерирует прокси-методы без виртуального диспетчера; «делегирование» натурализовано в русской OOP-литературе |

---

## 5. Effects and error handling · Эффекты и обработка ошибок

| Русский (норма #language) | English | Example (en) | Note |
|---|---|---|---|
| подмена обработчика (через `with`) `[keep-en: код]` | handler substitution | "Each effect has a **handler** that intercepts its operations, substituted via `with Handler = ...`" | language-tour.md §6; «обработчик», не «хендлер»; `with` — ключевое слово (см. §2) |
| прямой / транзитивный эффект | direct / transitive effect | "A function declares in its signature exactly which effects **it itself** performs; calling another function does not pull that function's effects up" | language-tour.md §6; spec/effects.md "Прямые эффекты, не транзитивные" (D28) — transitive is a warning by default, a hard error under `--strict-effects` |
| строгий режим эффектов (`--strict-effects`) `[keep-en: код]` | `--strict-effects` (strict-effects mode) | "programs (`examples/**`) build under `--strict-effects` … an experimental flag that promotes undeclared-transitive-effect … warnings to hard errors" | language-tour.md §6; Plan 197; `--strict-effects` — буквальный CLI-флаг |
| эффект `Fail` `[keep-en: код]` | `Fail` effect | "`Fail[E]` — эффект-контракт для перехвата и обработки ошибки" | spec/effects.md "Роли — throw / Fail[E] / handler"; `Fail` — имя типа-эффекта в prelude |
| выбросить ошибку (`throw`) `[keep-en: код]` | throw | "`throw err` — language syntax, raises an error" (paraphrase of spec/effects.md "Роли") | never resumes at the throw point; `never` operation type; `throw` — ключевое слово |
| паника | panic | "**panic** is for a broken caller contract … and is never recoverable" | language-tour.md §5; натурализованный термин, стандартен в переводной PL-литературе |
| постфиксные операторы `?` / `!!` | postfix operators `?` / `!!` | "`expr?` — return-style … `expr!!` — throw-style: 'didn't work — throw via `Fail`'" (paraphrase of spec/effects.md) | spec/effects.md "Операторы `?` и `!!`" — programmer picks the handling style at the use site; символьные операторы, не слова |

---

## 6. Memory and performance · Память и производительность

| Русский (норма #language) | English | Example (en) | Note |
|---|---|---|---|
| управляемая куча | managed heap | "`o is a pointer to managed heap; GC-tracked`" (paraphrase, docs/guide/value-vs-reference.md) | GC-tracked reference-type storage, default for records/sum types; уже полностью переведено, «managed heap» — только в англ. колонке |
| анализ выхода за пределы области видимости (escape-анализ) `[keep-en: термин]` | escape analysis | "Go — escape analysis decides" (docs/guide/value-vs-reference.md comparison table); "не утекающие значения остаются на стеке" (spec/overview.md) | compiler decides stack vs heap automatically, no programmer annotation; устоявшегося краткого русского термина нет — «escape-анализ» встречается в компиляторной литературе как есть |
| регион `[keep-en: код]` / зона реального времени (`#realtime nogc`) `[keep-en: код]` | region / real-time zone (`#realtime nogc`) | "Arena-allocations через `region { }` — проектируемая форма (D6), ⚠ в текущем компиляторе не реализована"; "For real-time зон (звук, торговля, embedded) — атрибут `#realtime nogc fn`" | spec/syntax.md "Производительность"; `region`/`#realtime nogc` — буквальные Nova-конструкции (ключевое слово и атрибут); `region` — **пока не реализован в компиляторе** |
| стек-аллокация | stack allocation | "positional tuple — **stack** — value (copy on pass)" | docs/guide/value-vs-reference.md bracket-rule table; «стек» и «аллокация» — натурализованные CS-термины, не жаргон |

---

## 7. Concurrency — Vela runtime · Конкурентность — рантайм Vela

| Русский (норма #language) | English | Example (en) | Note |
|---|---|---|---|
| файбер | fiber | "Under the hood — **fiber-based scheduler** (like Go/OCaml 5)." | spec/effects.md "Async — невидимая инфраструктура"; ~4-8 KB stack, millions per machine; **утверждено владельцем 2026-08-03** — «файбер» остаётся русской формой (не «волокно»), употребление по всей базе доков не меняется |
| структурированная конкурентность | structured concurrency | "concurrency is structured, not a separate async dialect" | language-tour.md §8; **утверждено владельцем 2026-08-03**; сырое англ. «Structured concurrency» в русском тексте spec/overview.md — кандидат микро-правки норматива (заведено интегратору отдельно) |
| супервизия | supervision | "Supervision of failures is an ordinary effect `Supervisor`" (paraphrase, spec/overview.md D416) | Erlang/OTP-style child-failure policy: `escalate()` / `stop()`; «супервизия» — натурализованный термин (используется вне IT, напр. в психотерапии/менеджменте), не жаргон |
| дедлайн области видимости / отмена | (scope) deadline / cancellation | "`supervised(deadline:)` gives that block a shared deadline, and a spawn that misses it is genuinely cancelled" | language-tour.md §8; «дедлайн» — полностью натурализованное слово повседневного русского; «скоуп» заменён на «область видимости» |
| канал с разделёнными правами (send/recv) | capability-split | "The model is **capability-split** (Rust mpsc-style): `Channel.new(cap)` returns a **pair**" | docs/guide/channels.md; описательный перевод вместо хайбридного «capability-split»; `send`/`recv` — имена методов, keep-en по коду |
| выбор (`select`) `[keep-en: код]` | `select` | "`select { ... }` is multiplexed channel operations: it waits on several recv/send operations at once" | docs/guide/channels.md; `select` — ключевое слово Nova |
| свобода от гонок (данных) | data race freedom | heading of spec/decisions/06-concurrency.md D415: "Data race freedom — `#share`-атрибут, capture-check, consume в spawn" | compiler-enforced boundary rules for `mut` captures crossing fiber boundaries; «гонка (данных)» — стандартный русский CS-термин |

---

## 8. Modules and packages · Модули и пакеты

| Русский (норма #language) | English | Example (en) | Note |
|---|---|---|---|
| модуль | module | "A **module** is either a single file `X.nv` or a **folder** `X/`" | language-tour.md §11; натурализованный термин |
| папка-модуль / равноправные файлы | folder-module / peer files | "A **module** is either a single file `X.nv` or a **folder** `X/` whose **peer files** all declare the same `module` path and share one namespace" | language-tour.md §11; **утверждено владельцем 2026-08-03** — «равноправные файлы» (файлы одного пространства имён без взаимного импорта; ближайший аналог в литературе — Go-шное «файлы одного пакета») |
| пакет | package | "Every import path is fully qualified from the **package** root (the directory with `nova.toml`)" | language-tour.md §11; натурализованный термин |
| рабочее пространство (`[workspace]`) `[keep-en: код]` | workspace | "Workspaces (`[workspace] members = [...]`) group several packages in a monorepo" | language-tour.md §11; норма-форма «рабочее пространство» вместо транслитерации «воркспейс»; `[workspace]` — буквальное имя секции в `nova.toml` |

---

## 9. Runtime, FFI and unsafe · Рантайм, FFI и unsafe

| Русский (норма #language) | English | Example (en) | Note |
|---|---|---|---|
| непрозрачный указатель | opaque pointer | "Nova's opaque-pointer type is `*()` (pointer to unit — `void*` in C)" | language-tour.md §12; полностью русская форма, без калек |
| типизированный дескриптор | typed handle | "Wrap a raw `*()` in a record for a **typed handle** so distinct native resources … aren't interchangeable at compile time" | language-tour.md §12; норма-форма «дескриптор» (стандартный русский CS-термин, как «дескриптор файла») вместо транслитерации «хэндл» |
| внешняя функция (`external fn`) `[keep-en: код]` | `external fn` | "`external fn name(args) -> ret` (D82) declares a binding to a C symbol" | language-tour.md §12; `external fn` — буквальная конструкция Nova |
| блок `unsafe` `[keep-en: код]` / модель мутабельности указателя | `unsafe` block / pointer-mutability model | heading of docs/guide/typed-pointers.md: "Typed pointers (`*T` family) + `unsafe` model", "Pointer-mutability model: 'arrow → box'" | `unsafe` is a scoped escape hatch for raw-pointer operations (Plan 138.5); `unsafe` — ключевое слово, «мутабельность» — натурализованный CS-термин |

---

## 10. Tooling and contracts · Тулинг и контракты

| Русский (норма #language) | English | Example (en) | Note |
|---|---|---|---|
| контракт (+ SMT-решатель) `[keep-en: аббревиатура]` | contract (+ SMT solver) | "Nova's contract system lets you state what a function **requires** and **ensures**, then verifies those claims at compile time via an SMT solver." | docs/guide/contracts.md intro; норма-форма «решатель» вместо транслитерации «солвер»; SMT — аббревиатура без русской расшифровки в обиходе |
| enforce-with-elision `[keep-en: термин]` | enforce-with-elision | "Nova uses **enforce-with-elision** (D24 / Plan 140), *not* debug-only asserts" | docs/guide/contracts.md intro; **утверждено владельцем 2026-08-03** — keep-en как имя собственное механизма (D24/Plan 140, аналогично CI/REST); при первом упоминании — русское пояснение: «контракты проверяются всегда, компилятор вырезает лишь доказанные проверки» |
| доказанный / недоказанный контракт | proven / unproven (contract) | "a **proven** contract is elided (zero runtime cost, even in debug); an **unproven** one is enforced at runtime in **both debug and release**" | docs/guide/contracts.md intro; полностью по-русски |
| лемма | lemma | "A **lemma** is a `#verify` function whose purpose is to establish a mathematical fact" | docs/guide/contracts.md "Lemmas and apply"; натурализованный матем. термин |
| постусловие / предусловие | postcondition / precondition | "`requires` — A precondition." / "`ensures` and `result` — A postcondition." | docs/guide/contracts.md; стандартные русские матем./CS-термины |
| условие `decreases` (доказательство терминации) `[keep-en: код]` | `decreases` clause | "`decreases` — Proves termination of recursive functions." | docs/guide/contracts.md "decreases"; норма-форма «условие» вместо транслитерации «клоз»; `decreases` — ключевое слово |

---

## 11. Conversions and overloading · Конверсии и перегрузка

| Русский (норма #language) | English | Example (en) | Note |
|---|---|---|---|
| приведение (`as`) `[keep-en: код]` | `as` cast | "`as` — infallible numeric/newtype/sum cast, compile-time, no runtime code" (paraphrase of spec/conversions.md "Три механизма") | spec/conversions.md; «приведение» — стандартный русский термин («приведение типа»), `as` — ключевое слово |
| расширение / сужение | widening / narrowing | "Widening (no precision loss)" / "Narrowing (potential precision loss)" | spec/conversions.md "Numeric ↔ numeric"; полностью по-русски |
| проверяемое сужение | checked narrowing | "Checked narrowing — `try_to_*` (D430, 2026-07-20)" | spec/conversions.md heading; полностью по-русски |
| неявная конверсия (`#coerce`) `[keep-en: код]` | `#coerce` (zero-cost implicit conversion) | "`#coerce` on a **unary** function declares an **implicit** conversion `I → O`, inserted by the compiler in a position with a known expected type" (paraphrase of spec/conversions.md) | spec/conversions.md "Zero-cost неявные конверсии" (D429); «конверсия» — натурализованный термин, `#coerce` — буквальный атрибут |
| конвенция имени (`from`/`try_from`) `[keep-en: код]` | naming convention (`from`/`try_from`) | "these are three independent naming conventions, each an ordinary Nova function with no protocol behind it" (paraphrase of spec/conversions.md) | spec/conversions.md "Именование from/try_from — конвенция, не протокол"; `From`/`Into`/`TryFrom`/`TryInto` protocols retracted 2026-07-06; `from`/`try_from` — буквальные имена методов |
| потребляющая передача владения (`consume @into_*`) `[keep-en: код]` | consuming ownership transfer (`consume @into_*`) | "`consume @into_ЦЕЛЬ()` — a consuming transfer of ownership (a concrete name on the source)" (paraphrase of spec/conversions.md "Три механизма" table) | spec/conversions.md; уже норма-форма («потребляющая» от глагола «потреблять»), `consume @into_*` — буквальный код-паттерн |

---

## Open questions for owner review

1. **«Одна дверь» (§1). ✅ РЕШЕНО (владелец 2026-08-03):** en-форма —
   **"no second door"** (дверная метафора сохранена); "single canonical
   path" — допустимое пояснение при первом упоминании. Строка §1
   обновлена.
2. **«Скрутини» (§1). ✅ РЕШЕНО (владелец 2026-08-03):** ru —
   **«сопоставляемое значение»**, транслит «скрутини» в новую прозу не
   пускать (существующие D-блоки не переписываем); en — **scrutinee**
   принят как термин. Строка §1 обновлена.
3. **`spec/paradigm.md` устарел. ✅ РЕШЕНО (владелец 2026-08-03):**
   сначала **актуализировать** русский текст (снять `trait`/`impl` →
   `protocol` + эффект-через-kind-токен, сверить с действующими D-блоками),
   затем переводить. Порядок в Ф.1: 5 актуальных файлов переводятся сразу,
   `paradigm.md` — после ревизии норматива (отдельная микро-задача, не
   переводческая).
4. **Счёт терминов (80 строк) — ✅ решено:** оставить как есть, слитые
   строки не разбивать; разбиение — по факту надобности переводчиков
   (дешёвая правка в любой момент).
5. **`ro` vs `readonly`. ✅ РЕШЕНО (владелец 2026-08-03):** использовать
   **`ro`**, указывая, что это сокращение от `readonly`. Отдельной строки
   для `readonly` не заводим — примечание в строке `ro` (§2) обновлено.

### Русские формы — спорные, решает владелец (добор 2026-08-02, норма #language)

6. **«Файбер» vs «волокно» (§7). ✅ РЕШЕНО (владелец 2026-08-03):**
   **«файбер» остаётся** русской формой (сознательно сохранённая
   транслитерация — термин насквозь пронизывает доки, спеку и
   dev-конвенции; массовая замена не производится). Строка §7 обновлена,
   пометка `[CONTESTED]` снята.
7. **«Тип-сумма» vs «sum-тип» (§3). ✅ РЕШЕНО (владелец 2026-08-03):**
   в новой ru-прозе — **«тип-сумма»**; гибрид «sum-тип» в существующей
   спеке не переписываем (прецедент «скрутини»).
8. **newtype по-русски (§3). ✅ РЕШЕНО (владелец 2026-08-03):**
   **«тип-обёртка»**, при первом упоминании на странице — «тип-обёртка
   (newtype)»; в коде и сигнатурах остаётся `newtype`.
9. **«Структурированная конкурентность» (§7). ✅ РЕШЕНО (владелец
   2026-08-03):** принято. Отдельно заведена микро-правка норматива:
   в `spec/overview.md` сырое англ. «Structured concurrency» посреди
   русской фразы → русская форма (правка языковая, смысл не меняется).
10. **enforce-with-elision (§10). ✅ РЕШЕНО (владелец 2026-08-03):**
    **keep-en** — имя собственное механизма (D24/Plan 140), как CI/REST;
    при первом упоминании — краткое русское пояснение рядом.
11. **peer files (§8). ✅ РЕШЕНО (владелец 2026-08-03):**
    **«равноправные файлы»** (не «peer-файлы», не «файлы-соседи»).
