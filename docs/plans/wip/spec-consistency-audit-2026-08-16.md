# Аудит самосогласованности спеки — 2026-08-16

> Заказ владельца: «исследуй, что спека по Нова непротиворечива и самосогласована».
> Метод: 8 тематических чтецов по spec/decisions/*.md (415 D-блоков, ~67k строк),
> каждая находка — два дословных цитирования с file:line; затем три независимых
> скептика на находку (линзы: амендменты / два-механизма / минимальная программа).
> Прогон: workflow wf_d42e662d-740, два захода (63 + 132 агента), ~14.9M токенов.
> **Прогон ДВАЖДЫ оборван лимитом сессии** (второй сброс 21:30 МСК): все 8 тем
> прочитаны, верификация ~60% кандидатов завершена, синтез не выполнен. Ниже — что добыто, с
> честными пометками статуса. Возобновление: Workflow resumeFromRunId=wf_d42e662d-740
> (завершённые агенты вернутся из кэша, доработают только упавшие).

## Вердикт (предварительный)

Спека НЕ самосогласована в буквальном чтении: 98 кандидатов-расхождений на 7 тем, из них к моменту обрыва 3 подтверждены как настоящие противоречия (>=2 скептиков из 3), 14 сняты как «два механизма», 31 — как «старый текст, перекрытый амендментом». Главный класс — НЕ ошибки замысла, а **рост амендментами без сводных блоков**: старый D-блок остаётся нормативным по букве после того, как поздний блок его перекрыл (ровно класс, который сегодня закрыл D464 для бáундов).

## 0. ЧТО УЖЕ ЗАКРЫТО ПО ЭТОМУ АУДИТУ (обновление 2026-08-17)

**Три подтверждённых противоречия (раздел 1) — разобраны все три.**

* **D463 против D416** (два из трёх) — были МОИ, в блоке, написанном накануне:
  пример возвращал `Decision.Restart`, ретрактированный владельцем 2026-07-10,
  и проза утверждала, что паника приходит типом `Panic`, тогда как D416 говорит
  `str`-сообщение и типа `Panic` в прелюдии нет. Исправлено в тексте: пример
  ветвится по `report().kind == "panic"` — тому самому полю, что печатает
  JSON-рендерер D462, то есть источник истины о виде отказа один.
* **D72 против D145** (forward-ref в generic-списке) — решение владельца:
  правило D72 СНЯТО. Три факта против него: чекер его никогда не проверял,
  std сам его нарушает и компилируется, D145 даёт нарушение как валидный
  пример. Реестр №702.

**Семь мест из раздела 3 помечены (2026-08-17).** Отобраны по критерию
«перекрытие датировано, подтверждено скептиками, решений владельца не
требует» — то есть закрываются пометкой, а не выбором:

* **Группа «семантика `?`» → перекрыта D85** (3 пометки + 1 исправленный
  пример). D85 определяет `?` как return-стиль БЕЗ эффектов и энфорсит это
  диагностикой `E_TRY_IN_FAIL_FN`; четыре места описывали снятую семантику
  «`?` ⇒ throw, требует `Fail[E]`»: `04-effects.md` (D62 Rule 2, D61 §8,
  строка про `expr? ⇒ throw`) и нормативный пример D158 в `03-syntax.md:6995`,
  который под D85 просто НЕ СКОМПИЛИРУЕТСЯ — там `do_work()?` в функции с
  `Fail[Err]`. Пример исправлен на `!!`, три текста получили блок-цитату
  «ПЕРЕКРЫТО D85» и сохранены как история.
* **Группа «снятая форма `-> consume T`» → перекрыта D445 §616** (3 пометки).
  D445 говорит: return-позиция для `consume` не существует НИ В КАКОЙ форме,
  с двумя кодами ошибок. Помечены `02-types.md:3619`, `05-memory.md:488`,
  `06-concurrency.md:6141`. На класс уже есть страж
  `check-retracted-param-form` с храповиком осадка по зонам — то есть
  закрытие измеримо, а не на слово.

**ЧТО ЗНАЧИТ ГАЛОЧКА (вопрос владельца 2026-08-17: «ПОМЕЧЕНО — закрыто или
нет?»).** Не везде одно и то же, и это важно:

* ✅ **Группа B (`-> consume T`) — ЗАКРЫТА.** Не потому, что текст поправлен, а
  потому, что класс держит МАШИНА: `check-retracted-param-form` считает осадок
  снятой формы по зонам с храповиком вниз. Восьмое вхождение покраснеет само.
* ✅ **Группа A (семантика `?`) — ЗАКРЫТА 2026-08-17.** Держит
  `check-retracted-try-semantics` (реестр №713): ядро на питоне считает по
  зонам ДВА семейства — лексическое (проза «`?` — сахар над `throw`», СНЯТАЯ трактовка) и
  структурное (`?` внутри функции, объявившей `Fail`; grep такое не видит в
  принципе, признак разнесён между строкой подписи и строкой тела).
  Публикуемое руководство держится на НУЛЕ без храповика — и ноль там
  достигнут, а не объявлен: волной того же дня 14 мест в
  `tutorial-cleanup.{md,ru.md}` переведены на `!!`. Исторические зоны под
  храповиком вниз: spec 35, docs/plans 12, docs/dev 60. Самотест 12/12
  проверяет ОБЕ стороны, включая то, что страж НЕ ловит законную форму D196
  `consume X = expr? { body }`, оператор `??`, слово `desugar` и строки,
  которые сами помечают форму снятой.

**ПРОВЕРКА СОБСТВЕННОЙ РАЗМЕТКИ 2026-08-17 (инвентаризация к стражу).** Разметку
проверил отдельный агент-инвентаризатор, и она НЕ выдержала проверки на двух
местах из четырёх. Пишу здесь, а не молча правлю, потому что это ровно тот
класс, за который отчёт и заведён:

* **D86, таблица сравнения `?`/`!!`/`??` (`04-effects.md`).** Отчёт утверждал
  «ПОМЕЧЕНО 2026-08-17» — **маркера в файле не было вовсе.** Ячейка «Эффект» у
  `expr?` продолжала читаться «требует `Fail[E]` если `expr` это `Result`».
  Исправлено 2026-08-17: ячейка ПЕРЕПИСАНА по D85 (эффекта нет; требуется,
  чтобы enclosing fn возвращала `Result`/`Option`), под таблицей — врезка о
  том, что именно поменялось. Здесь текст правится, а не помечается: таблица
  нормативна и читается как справка, а пометка рядом с неверной ячейкой
  оставляет неверную ячейку.
* **D158 model-B (`03-syntax.md`).** Помечен ОДИН носитель из ВОСЬМИ: форма
  `do_work()?` в Fail-функции встречается в файле восемь раз (строки 5111,
  5305, 7031, 7219, 7466, 7484, 9565, 9569). Семь остаются. Пометка одного
  экземпляра и была тем самым «фиксом носителя», против которого написан
  весь этот отчёт.

**Вывод, который дороже обеих правок:** разметка руками не считается закрытием
даже для носителя — потому что руками же и промахивается. Поэтому группа A
закрывается не пометками, а стражем `check-retracted-try-semantics`
(реестр №713): он МЕРЯЕТ осадок по зонам, и его число, а не моё слово,
становится содержанием галочки.

**Сделано в тот же день.** Страж стоит в гейте (`gate.sh`, шаг
`retracted-try-semantics`), база заведена с летописью, самотест 12/12.
Дальше числа опускаются волнами по зонам — первый кандидат `docs/dev`
(60 мест, почти всё в `idioms/`: их читают как образец).

**Почему не все 31.** Остальные 24 «перекрытых» требуют либо чтения на месте
(какая сторона победила — не всегда очевидно из дат), либо решения владельца.
Отбирать их «пачкой» значило бы повторить ту же ошибку, от которой аудит и
лечит: объявить нормой то, что не проверено.

**56 недоверифицированных (раздел 4) — трогать нельзя по построению:** в этом
же прогоне больше половины проверенных кандидатов растворялись под линзами
(амендмент / два механизма). Их разбор — возобновление воркфлоу.

---

## 1. ПОДТВЕРЖДЁННЫЕ противоречия — чинить (3)

Каждое подтверждено минимум двумя независимыми скептиками с построением
минимальной программы, чей смысл/компилируемость зависит от того, какому тексту верить.

### ✅ ЗАКРЫТО — Generic-param forward reference in bound: error per D72, valid example per D145

> **Решение владельца 2026-08-16: правило D72 СНЯТО** (реестр №702). Имена одного
> generic-списка видны друг другу целиком (прецедент rustc). Пометка стоит в D72.

* **Род:** contradiction — подтверждено 2/3 скептиками
* **A** `spec/decisions/02-types.md:4191`:
  > fn func[T From[K], K](v K) -> T          // ОШИБКА: K используется до объявления
* **B** `spec/decisions/02-types.md:7418`:
  > fn[T From[K], K] T @construct_from(v K) -> T => T.from(v)   // parametric protocol
* **Почему конфликт:** D72 §«Порядок объявления параметров» (02-types.md:4182-4184) requires a name used in a bound to be declared earlier in the same `[...]` list and shows `[T From[K], K]` as an error. D145 §«Bound syntax (через D72)» presents the same ordering `fn[T From[K], K]` as a valid bound example. Both are normative; a program using this order compiles under one text and not the other.

  * скептик: Both texts govern the same mechanism and assign opposite validity to the same code. D72's ordering rule (02-types.md:4182-4184) is position-agnostic («Имя в bound'е должно быть уже объявлено — либо ранее в том же списке [...], либо в type-контексте») and marks `fn func[T From[K], K]` as ОШИБКА (4191). D145's example `fn[T From[K], K] T @construct_from(v K)` (7418) sits in a section literally titled «Bound syntax (через D72)», and D72's own fn[T]-prefix subsection (4168) says «Bound syntax из D72 применим в этой позиции» — so the two-mechanisms escape is foreclosed by both texts explicitly unif
  * скептик: The parameter list `[T From[K], K]` is byte-identical in both places. D72's normative ordering rule (02-types.md:4182-4184) requires a name used in a bound to be declared earlier in the same `[...]` list or in a type-context, and line 4191 explicitly marks `fn func[T From[K], K]` as ОШИБКА. D145's normative section «Bound syntax (через D72)» (02-types.md:7412-7419) presents `fn[T From[K], K] T @construct_from(v K) -> T` as a valid parametric-protocol example; the receiver is bare T, so no carrier or enclosing type declares K — the type-context escape does not apply. D145 explicitly imports D72

### ✅ ЗАКРЫТО — D463 example returns `Decision.Restart`; D416 §4 retracted the Restart family

> **Исправлено в D463 2026-08-16** (блок мой, написан накануне): пример возвращает
> `Decision.Escalate`/`Stop` — словарь D416 §4 полный.

* **Род:** stale-amendment — подтверждено 2/3 скептиками
* **A** `spec/decisions/08-runtime.md:9133`:
  > return if err is Panic { Decision.Stop } else { Decision.Restart }
* **B** `spec/decisions/06-concurrency.md:7823`:
  > Решением владельца 2026-07-10 семейство УДАЛЕНО из словаря целиком (мотив — §1): гейт-диагностика ретрактирована вместе с вариантами, ссылка на `Decision.Restart` — обычный unknown-variant
* **Почему конфликт:** D463 (accepted 2026-08-16, spec-first) shows the Supervisor handler idiom with `Decision.Restart`. D416 §1 (06-concurrency.md:7749 `type Decision enum Escalate | Stop`) and §4 say the vocabulary is exactly Escalate|Stop and `Decision.Restart` is a compile error. A user copying D463's canonical example gets an unknown-variant error.

  * скептик: The amendment sweep confirms the clash and finds nothing that resolves it. D416 §1 amendment (06-concurrency.md:7762) and §4 (06-concurrency.md:7819-7827, owner decision 2026-07-10) remove the Restart family entirely: 'type Decision enum Escalate | Stop' (06-concurrency.md:7749) is declared COMPLETE, 'ссылка на `Decision.Restart` — обычный unknown-variant', and even E_SUPERVISOR_RESTART_GATED plus the restart_gated_neg fixture were retracted with it. No later text reinstates Restart anywhere in spec/decisions (grep over all files), and 08-runtime.md ends at line 9213 so no amendment follows D4
  * скептик: Cannot refute. There is exactly one Decision type (std/src/prelude/effects.nv:277 `export type Decision enum Escalate | Stop`), and D463's example (08-runtime.md:9133 `return if err is Panic { Decision.Stop } else { Decision.Restart }`) is a handler of the exact D416 Supervisor effect — same enum, same use-site — so no two-mechanisms boundary exists. The runtime bridge's "any non-Stop tag maps to Escalate (defensive)" (06-concurrency.md:7825-7826) is not a rescuing second mechanism: the same sentence declares `Decision.Restart` an ordinary unknown-variant, i.e. a compile error that never reach

### ✅ ЗАКРЫТО — D463: panic as type `Panic` vs D416: panic arrives as `str`

> **Исправлено в D463 2026-08-16**: вид отказа читается `report().kind` — тем самым
> полем, что печатает JSON-рендерер D462; типа `Panic` в прелюдии нет, норма — D416.

* **Род:** contradiction — подтверждено 3/3 скептиками
* **A** `spec/decisions/08-runtime.md:9143`:
  > **Вид отказа нового API не требует.** Паника приезжает обработчику типом `Panic`, решение пишется существующим сужением ([D54](03-syntax.md#d54)): `if err is Panic`.
* **B** `spec/decisions/06-concurrency.md:7756`:
  > `err` — ошибка как `any`: typed-throw payload (сужается `err is T`), string-throw / panic — `str`-сообщение.
* **Почему конфликт:** D416 §1 fixes the runtime→handler payload for a panicked child as a plain `str` (so `err is Panic` is false / meaningless — `Panic` exists only as a variant of `ScopeOutcome`, 03-syntax.md:9658, and D65 says a sum variant is not a type for `is`). D463 builds its 'no new API needed' argument on `err is Panic` distinguishing panics. Which narrowing works in a Supervisor handler is user-visible.

  * скептик: Amendment sweep finds no supersession in either direction. D416 §1's panic payload contract (06-concurrency.md:7756-7757 "string-throw / panic — `str`-сообщение") has no amendment touching it — the only D416 amendments (2026-07-10, §1/§4) retract the Restart family, not the payload. D463 (2026-08-16) is later but is NOT an amendment of D416: this spec marks supersession explicitly when intended (D462: "Амендмент [D437] ... это он"; D416 §4: "superseded §3b-гейт"), while D463's header says only "Продолжение [D462]" and — decisively — its own text claims NO change: "Вид отказа нового API не треб
  * скептик: Both texts fix the SAME value in the SAME position — the `err any` parameter of Supervisor.on_child_fail, filled by one runtime bridge (D416 §2 boxes NovaChildError into `any`) — so no two-mechanisms split exists. D416 §1 (06-concurrency.md:7756, landed 2026-07-10, restated normatively in std/src/prelude/effects.nv: "for string-throw / panic — a `str` message") says a child panic arrives as a plain `str`. D463 (08-runtime.md:9143, accepted 2026-08-16, implementation pending №680) claims the panic arrives "типом `Panic`" and that `if err is Panic` decides via existing D54 narrowing. No amendmen
  * скептик: The program lens produces a concrete divergence, so refutation fails. Minimal program: a Supervisor handler `on_child_fail(idx int, err any)` returning `if err is Panic { Decision.Stop } else { Decision.Escalate }` over `supervised { spawn { panic("boom") } }`. Under D416 §1 (spec/decisions/06-concurrency.md:7756 — «string-throw / panic — `str`-сообщение», mirrored in std/src/prelude/effects.nv:286-287 and implemented in compiler-codegen/src/codegen/emit_c.rs `_nova_supervisor_decide_impl`, which boxes panics as `str`), `Panic` names no type — no `type Panic` exists anywhere in spec/ or std/ (

## 2. «Два механизма» — спека права, формулировка смазывает границу (14)

Не чинить поведение; добавить по одной фразе, называющей границу (как D464 для #impl).

* **Blanket-receiver bound: satisfied by #impl registration (D285 §2.2/§4) vs #impl unrelated to bound selection (D464, D268, D186)** — A `spec/decisions/10-overloading.md:726` / B `spec/decisions/10-overloading.md:833`. Граница: 
* **Cross-module overloading: forbidden by D84 locality rule vs explicitly permitted by D267** — A `spec/decisions/10-overloading.md:392` / B `spec/decisions/10-overloading.md:485`. Граница: 
* **Content of «structural conformance» check: name+arity only (D53 amend) vs also receiver_mut (D72 amend / D209)** — A `spec/decisions/02-types.md:1218` / B `spec/decisions/02-types.md:4418`. Граница: 
* **D122 acceptance criteria still say bound must be a protocol type; D72/D310 allow type-set bounds** — A `spec/decisions/02-types.md:4517` / B `spec/decisions/02-types.md:4122`. Граница: 
* **D65 Rule 4: missing handler THROWS `RuntimeError.NoHandler` (Fail); D61/D62/D65 Rule 2: missing handler is a runtime PANIC** — A `spec/decisions/04-effects.md:3517` / B `spec/decisions/04-effects.md:2332`. Граница: 
* **D118 'Правило' prescribes a codegen dispatch order (catch-all TLS slot checked first) that differs from D65 Rule 2's semantic lookup (exact Fail[E] first, then catch-all)** — A `spec/decisions/04-effects.md:5647` / B `spec/decisions/04-effects.md:3425`. Граница: 
* **Closure consuming a captured value: D131 says consume does not leak out, D157 says it does** — A `05-memory.md:356` / B `05-memory.md:960`. Граница: 
* **View-peek `Some(f)` on must-consume payload: D133 legal, D157 amendment requires `Some(consume f)`** — A `02-types.md:5876` / B `05-memory.md:1093`. Граница: 
* **Destructuring a consume record into per-field linear bindings: rejected in D133, implemented by D180 amendment №378** — A `02-types.md:5979` / B `05-memory.md:831`. Граница: 
* **`consume` in if-conditions: D184 blanket ban vs D157 amendment requiring `Some(consume x)` in `if let`** — A `03-syntax.md:7604` / B `05-memory.md:1097`. Граница: 
* **D180 amendment defines a «нормативное» rule by the best-effort behaviour of `infer_value_type`** — A `05-memory.md:708` / B `05-memory.md:480`. Граница: 
* **Fn-value call: checker heuristic treats view-style argument as consumed, contrary to D133 view-param semantics** — A `05-memory.md:1315` / B `02-types.md:5654`. Граница: 
* **D133 says value-consume zeroes fields after consume; #465 amendment makes zeroing opt-in and partial** — A `02-types.md:5949` / B `02-types.md:13922`. Граница: 
* **D78 layout principle and Plan-195 amendment still prescribe rev-1 full-path declaration `module std.encoding.base64`, which D29 rev-3 strict-removal makes a hard error** — A `spec/decisions/07-modules.md:302` / B `spec/decisions/07-modules.md:1395`. Граница: 

## 3. Перекрыто амендментом — старый текст пометить, не удалять (31)

Форма: блок-цитата «ПЕРЕКРЫТО DNNN» на старом месте (образец — сегодняшняя правка D285 §3).

* **Cross-module overloading: forbidden by D84 locality rule vs explicitly permitted by D267** — A `spec/decisions/10-overloading.md:392` / B `spec/decisions/10-overloading.md:485`. Что перекрывает: 
* **Protocol instance-method `@` prefix: optional/equivalent (D53) vs mandatory with parse error (D209)** — A `spec/decisions/02-types.md:1116` / B `spec/decisions/04-effects.md:6090`. Что перекрывает: 
* **Default-body synthesis on bare method call: never (D183 amend part 2) vs gated by #impl (D186)** — A `spec/decisions/02-types.md:8341` / B `spec/decisions/02-types.md:8411`. Что перекрывает: 
* **Type-set signedness: no mixing at all (D310) vs full SignedInt∪UnsignedInt union allowed (D423 R1) — D310 not amended in place** — A `spec/decisions/02-types.md:16486` / B `spec/decisions/04-effects.md:7706`. Что перекрывает: 
* **D122 acceptance criteria still say bound must be a protocol type; D72/D310 allow type-set bounds** — A `spec/decisions/02-types.md:4517` / B `spec/decisions/02-types.md:4122`. Что перекрывает: 
* ✅ **ЗАКРЫТО 2026-08-17 (класс держит страж)** — **D62 Rule 2 still makes `expr?` a throw that requires Fail[E]; D85 makes `?` return-only and forbids it in Fail-fns** — A `spec/decisions/04-effects.md:2668` / B `spec/decisions/04-effects.md:4641`. Что перекрывает: 
* ✅ **ЗАКРЫТО 2026-08-17 (класс держит страж)** — **D86 operator comparison table says `expr?` on Result requires Fail[E]; D85 says `?` no longer involves Fail** — A `spec/decisions/04-effects.md:5196` / B `spec/decisions/04-effects.md:4700`. Что перекрывает: 
* ✅ **ЗАКРЫТО 2026-08-17 (класс держит страж)** — **D61 §8 'Связь с ?' still defines `?` as sugar over throw; D85 defines it as `return Err(e)`** — A `spec/decisions/04-effects.md:2110` / B `spec/decisions/04-effects.md:4707`. Что перекрывает: 
* ✅ **ЗАКРЫТО 2026-08-17 (класс держит страж)** — **D158 model-B normative example uses `do_work()?` in a Fail[WorkErr] fn — rejected by D85's E_TRY_IN_FAIL_FN** — A `spec/decisions/03-syntax.md:6995` / B `spec/decisions/04-effects.md:4649`. Что перекрывает: 
* **D65 Rule 4: `a/b` and `arr[i]` THROW RuntimeError via Fail (D28 infers Fail[RuntimeError]); D427/D13/D325 R0: they PANIC and never appear in the signature** — A `spec/decisions/04-effects.md:3509` / B `spec/decisions/04-effects.md:7749`. Что перекрывает: 
* **D12 Level 1: a fn with an EXTRA effect is rejected by a typed queue; D448: fn-type compatibility is by inclusion, extra effects are fine** — A `spec/decisions/04-effects.md:604` / B `spec/decisions/04-effects.md:8025`. Что перекрывает: 
* **D31/D61: a Fail handler-lambda MUST `interrupt` (final expression invalid); D449 example (and D65 §1) uses `with Fail[..] = |_e| Err(ReadFailed)` with no interrupt** — A `spec/decisions/04-effects.md:1544` / B `spec/decisions/06-concurrency.md:7067`. Что перекрывает: 
* **D61 §9 canonical example returns `Effect[Fail[Error]]` from a handler that `interrupt`s; D87 makes `Effect[E]` ≡ `Effect[E, never]` and interrupt in it a compile error** — A `spec/decisions/04-effects.md:2209` / B `spec/decisions/04-effects.md:5265`. Что перекрывает: 
* **D61 §1/§2 (and D67/D85 handler examples) present generic effect ops `in_transaction[T]` as valid declarations; D456 makes the checker reject them by name** — A `spec/decisions/04-effects.md:1771` / B `spec/decisions/04-effects.md:8226`. Что перекрывает: 
* **D31 grammar (and D85/D90 examples) still use the `handler` keyword for handler literals; D61 Plan-97 amendment (D142) retired it in favour of `effect X { }`** — A `spec/decisions/04-effects.md:1621` / B `spec/decisions/04-effects.md:2544`. Что перекрывает: 
* **In-body view/mut alias of a consume binding: D133 allows, D180 Rule 2 forbids** — A `02-types.md:5909` / B `05-memory.md:501`. Что перекрывает: 
* **Closure consuming a captured value: D131 says consume does not leak out, D157 says it does** — A `05-memory.md:356` / B `05-memory.md:960`. Что перекрывает: 
* **View-peek `Some(f)` on must-consume payload: D133 legal, D157 amendment requires `Some(consume f)`** — A `02-types.md:5876` / B `05-memory.md:1093`. Что перекрывает: 
* **Destructuring a consume record into per-field linear bindings: rejected in D133, implemented by D180 amendment №378** — A `02-types.md:5979` / B `05-memory.md:831`. Что перекрывает: 
* **D180 Rule 5 uses a `view` parameter keyword that D157 declares non-existent (parse error)** — A `05-memory.md:539` / B `05-memory.md:893`. Что перекрывает: 
* ✅ **ЗАКРЫТО 2026-08-17 (класс держит страж)** — **`-> consume T` return form: D176 lists it as valid prefix form, D445 №616 retracts it entirely** — A `02-types.md:3619` / B `02-types.md:3730`. Что перекрывает: 
* ✅ **ЗАКРЫТО 2026-08-17 (класс держит страж)** — **D180/D133 examples still use retracted postfix `-> T consume` return syntax** — A `05-memory.md:488` / B `02-types.md:3730`. Что перекрывает: 
* ✅ **ЗАКРЫТО 2026-08-17 (класс держит страж)** — **D174 guard API tables keep `-> MutexGuard consume` signatures retracted by D445** — A `06-concurrency.md:6141` / B `02-types.md:3733`. Что перекрывает: 
* **Overload mode axis: rvalue argument selects only ro-version (rule 1) yet consume-version for owned rvalue (rule 3)** — A `10-overloading.md:293` / B `10-overloading.md:297`. Что перекрывает: 
* **Module declaration depth: D29 rev-3 says always 2 segments (parent.target); D78 rev-6 says nested folders get full root prefix (3+ segments)** — A `spec/decisions/07-modules.md:246` / B `spec/decisions/07-modules.md:1320`. Что перекрывает: 
* **D78 layout principle and Plan-195 amendment still prescribe rev-1 full-path declaration `module std.encoding.base64`, which D29 rev-3 strict-removal makes a hard error** — A `spec/decisions/07-modules.md:302` / B `spec/decisions/07-modules.md:1395`. Что перекрывает: 
* **nova_tests layout example: 'Module path = filesystem path. Первая компонента — package name' contradicts D29 rev-3 parent.target** — A `spec/decisions/07-modules.md:1955` / B `spec/decisions/07-modules.md:229`. Что перекрывает: 
* **D29 still forbids cyclic imports (compile error); D291 allows cross-module cycles** — A `spec/decisions/07-modules.md:549` / B `spec/decisions/07-modules.md:708`. Что перекрывает: 
* **Bare module import namespace: D29 says `import std.collections` → `std.collections.HashMap`; D289 says last segment only (`collections.HashMap`)** — A `spec/decisions/07-modules.md:480` / B `spec/decisions/07-modules.md:776`. Что перекрывает: 
* **Same syntax `import a.b.C` means 'import single item C' in D29 but 'import module a.b.C as namespace C' in D289** — A `spec/decisions/07-modules.md:472` / B `spec/decisions/07-modules.md:776`. Что перекрывает: 
* **D47 example re-exports a whole module (`export import std.duration`); D288 makes whole-module `export import` an error (E_REEXPORT_GLOB)** — A `spec/decisions/07-modules.md:974` / B `spec/decisions/07-modules.md:759`. Что перекрывает: 

## 4. НЕ ВЕРИФИЦИРОВАНО — кандидаты, оставшиеся без трёх скептиков (56)

Найдены чтецами с цитатами, но верификация оборвана лимитом. НЕ считать
подтверждёнными: по опыту этого же прогона больше половины кандидатов снимается
(амендмент/два-механизма). Разбирать при возобновлении.

### ambiguity (6)
* D61 §8 types a with-block as the least common supertype of body and interrupt types; D87 (and D61 §10) make mismatched body/interrupt types a compile error — `spec/decisions/04-effects.md:2143` vs `spec/decisions/04-effects.md:5319`
* When is a unit-typed trailing expression of `supervised {}` evaluated — post-join (D71) or eager/pre-join (D414 §4)? — `spec/decisions/06-concurrency.md:784-785` vs `spec/decisions/06-concurrency.md:7244-7246`
* Unsigned negation: binary `0 - x` traps (D423 R3) but unary `-x` on unsigned wraps silently (D427 R2), while `checked_neg` reports it as overflow — `spec/decisions/04-effects.md:7712` vs `spec/decisions/04-effects.md:7755`
* Non-exhaustive match on a sum: compile error (surface spec, D59) vs «non-exhaustive match warning» (D65) — `spec/syntax.ru.md:869-871` vs `spec/decisions/04-effects.md:3658-3659`
* D54 destructures a record-form variant positionally (`Circle(r)`) although it declared `Circle { radius f64 }` — `spec/decisions/03-syntax.md:3587` vs `spec/decisions/03-syntax.md:3707-3712`
* Intra-package import path: D29 example omits the package segment (`import admin.billing.{Invoice}`), while D369/D78 say the first segment is the package name — `spec/decisions/07-modules.md:182` vs `spec/decisions/07-modules.md:2565`

### contradiction (21)
* D415 §2: `#share` vouch is trusted 'без проверки'; D446 §1: the declaration 'становится проверяемым: компилятор обязан убедиться' — `spec/decisions/06-concurrency.md:7354-7356` vs `spec/decisions/06-concurrency.md:8563-8566`
* D416 §5: top-level `detach` stays on 'дефолтном Escalate-all'; D414 §2 / D92 rule 3: detach default is LogAndDrop, never escalates — `spec/decisions/06-concurrency.md:7834-7836` vs `spec/decisions/06-concurrency.md:7197-7198`
* D191 lists `await fut` for `Future[T]` as a permitted suspend operation; D50 §5 / D14 say there is no `await` marker or Future type in the language — `spec/decisions/03-syntax.md:9705-9710` vs `spec/decisions/06-concurrency.md:616-618`
* `int as uint` — D54 table says bit-pattern (wrap), D130 says saturate neg→0 — `spec/decisions/03-syntax.md:3381` vs `spec/decisions/02-types.md:5517`
* Type-set names: D310 declares `SignedInt`/`UnsignedInt`, D430 and std use `SignedInts`/`UnsignedInts` — `spec/decisions/02-types.md:16486` vs `spec/decisions/04-effects.md:7795`
* `try_` prefix rule: D325 R3 forbids `try_` without a same-named infallible sibling; D430 names checked narrowing `try_to_<T>` with no `to_<T>` sibling — `spec/decisions/04-effects.md:6705` vs `spec/decisions/04-effects.md:7783`
* int→char checked conversion: D54 (reaffirmed 2026-08-01) prescribes `char.try_from(n)?`, D325-amend/std prescribe `(cp int).to_char()` — `spec/decisions/03-syntax.md:3469` vs `spec/decisions/04-effects.md:6756`
* `T.to_str()` as an interpolation escape: D186 amend allows bare `${x}` via a user `@to_str()`; D422 makes `@to_str` Display-bounded and forbids a to_str route into Display — `spec/decisions/02-types.md:8554` vs `spec/decisions/02-types.md:17424`
* Auto-derive on demand (D422 §3.3/3.4) vs `#impl(Display)` gate on interpolation (D186 amend / E_INTERP_NO_DISPLAY) — `spec/decisions/02-types.md:17427` vs `spec/decisions/02-types.md:8549`
* `@to_str` blanket bound: D410 amend says bare-T `fn[T] T @to_str()`, D422 says `fn[T Display] T @to_str()` — `spec/decisions/03-syntax.md:11841` vs `spec/decisions/02-types.md:17424`
* D430 canonical example uses type-suffix literals (`300u32`) that D44/D227 Rule 4 declare a syntax error — `spec/decisions/03-syntax.md:10525` vs `spec/decisions/04-effects.md:7783`
* Single-variant sum: D52 forbids it, D406 allows it ("minimum one variant") — `spec/decisions/02-types.md:527` vs `spec/decisions/02-types.md:786-788`
* Variant namespace: per-type with qualified fallback (D30/D65) vs flat namespace where qualified value ICEs (D321/D358/D340) — `spec/decisions/03-syntax.md:1169-1176` vs `spec/decisions/04-effects.md:6979`
* D5 says exactly two visibility levels and 'no package-private'; D457 introduces `priv(package)` (package-private) on top of D307 `priv(file)` — `spec/decisions/07-modules.md:65` vs `spec/decisions/02-types.md:18423`
* Field-level explicit `priv`: D47 (07-modules) says own-type-methods only; D281 §1 says module-private — `spec/decisions/07-modules.md:922` vs `spec/decisions/02-types.md:15577`
* D220's own amendment note says field-level explicit `priv` stays type-private; D281 (the amending block) says explicit `priv` field is module-private — `spec/decisions/02-types.md:11480` vs `spec/decisions/02-types.md:15563`
* Module identity: D281 keys module-private access by declaration `[P,Q]`; D29 Plan-202 amendment says declaration is never identity and duplicate declarations from different physical modules are distinct, legal modules — `spec/decisions/02-types.md:15589` vs `spec/decisions/07-modules.md:273`
* Import/local name collision: D29 says compile error; D371 says user re-declaration of an explicitly imported name wins (full shadow) — `spec/decisions/07-modules.md:579` vs `spec/decisions/07-modules.md:2289`
* Header says package name = its directory name; Plan-192 amendment mandates repository `nova-tls` with package name `tls` — `spec/decisions/07-modules.md:21` vs `spec/decisions/07-modules.md:1436`
* Pointer-stability model: D6 fixes non-moving GC with stable (interior) pointers; D216 normatively asserts addresses change under GC compaction/relocation — `spec/decisions/05-memory.md:130` vs `spec/decisions/02-types.md:10158`
* Escape analysis: D6 promises non-escaping values stay on the stack without managed-heap allocation; the allocation contract makes plain records unconditionally heap-allocated — `spec/decisions/05-memory.md:114` vs `spec/decisions/06-concurrency.md:5700`

### implementation-as-norm (3)
* D381 describes a codegen heuristic (arity → enclosing return type → registry) as the rule for resolving an ambiguous bare variant constructor — `spec/decisions/08-runtime.md:8878-8882` vs `spec/decisions/03-syntax.md:1175-1176`
* D100 states `_module.nv` attributes are inherited by all peers, then describes the resolver algorithm under which the entry peer does NOT inherit them (and importers DO inherit imported modules' attrs) — `spec/decisions/07-modules.md:2089` vs `spec/decisions/07-modules.md:2120`
* D287 extension-method import rule is stated universally but 'implemented' only for the entry module (stdlib peers exempt) — implementation limitation stated as part of the decision — `spec/decisions/07-modules.md:742` vs `spec/decisions/07-modules.md:742`

### stale-amendment (26)
* runtime.init(n>0) 'wins' worker-count resolution (D136/D138 rule 2) vs D451: init(n) is a diagnosed no-op in every reachable user-code case — `spec/decisions/06-concurrency.md:4115-4118` vs `spec/decisions/06-concurrency.md:4286-4290`
* D138 rule 5 obliges FFI/syscalls to sit inside `blocking { … }`, a block-form D64/D172 say the parser rejects — `spec/decisions/06-concurrency.md:4136-4138` vs `spec/decisions/04-effects.md:4034-4035`
* D50 §1 / D14 still route the no-suspend guarantee through the `realtime { }` block that D64/D172 retracted — `spec/decisions/06-concurrency.md:307-308` vs `spec/decisions/04-effects.md:4034-4036`
* D71 §6 says `let mut acc = 0; spawn { acc += x }` shares one cell 'как ожидается'; D415 §2 makes exactly that capture E_CONCURRENT_MUT_CAPTURE — `spec/decisions/06-concurrency.md:1063-1064` vs `spec/decisions/06-concurrency.md:7348`
* D50 §2 idiom table recommends `mut`-captures inside `supervised` for heterogeneous results; D415 §2 forbids naked mut-capture — `spec/decisions/06-concurrency.md:332` vs `spec/decisions/06-concurrency.md:7348`
* D75 headline example (`mut results []Response` pushed from `spawn`) is rejected by D415 §2 — `spec/decisions/06-concurrency.md:1237-1240` vs `spec/decisions/06-concurrency.md:7348`
* D415 §2: `ro`-binding capture 'всегда ок'; D441 §2: ro-captured closure values are the exception — `spec/decisions/06-concurrency.md:7347` vs `spec/decisions/06-concurrency.md:8722-8725`
* D416 §5 names the retired env var `NOVA_NO_AUTOARM=1`; D138 rule 4 renamed it to `NOVA_AUTOARM=0` — `spec/decisions/06-concurrency.md:7834-7835` vs `spec/decisions/06-concurrency.md:4127-4131`
* D98 'Ограничение': fiber is pinned to the worker it parked on, migration deferred; D138 rule 9 / D173 §6: fibers migrate between workers — `spec/decisions/06-concurrency.md:3591-3594` vs `spec/decisions/08-runtime.md:4236-4237`
* D97 body: 4096 slots × 2 MB, 4 KB guard (Linux/macOS); D233: 16384 fibers/worker, 4MB stack, 16KB guard as builtin defaults — `spec/decisions/06-concurrency.md:3383-3386` vs `spec/decisions/08-runtime.md:8104-8105`
* D62 effect table still lists `Blocking` as a mockable effect; D50 header says the Blocking effect is gone from the compiler entirely — `spec/decisions/04-effects.md:2812` vs `spec/decisions/06-concurrency.md:243-246`
* `int` ≡ `i64` type identity: D129 AMEND says distinct types, D227 (and D315) say alias — `spec/decisions/02-types.md:5431` vs `spec/decisions/03-syntax.md:10481`
* Checked numeric narrowing form: D54 says `T.try_from(x)?` throwing `Fail[OutOfRangeError]`; D430 says `x.try_to_<T>() -> Result[T, RangeError]` and rejects the static form — `spec/decisions/03-syntax.md:3399` vs `spec/decisions/04-effects.md:7783`
* `int/f64/bool/char as str` alternative: D54 says `str.from(v)`, D410 amend retracts `str.from(x)` — `spec/decisions/03-syntax.md:3453` vs `spec/decisions/03-syntax.md:11846`
* str→f64 parsing entry: D74 keeps static `f64.try_parse(s) -> Option[f64]`; D54 amend / D310 amend say `s.to_f64()` (Result) and retract `f64.try_parse` — `spec/decisions/08-runtime.md:2357` vs `spec/decisions/03-syntax.md:3462`
* Interpolation gate: D44 says `${expr}` requires `Into[str]` (sugar over `str.from`), D422 says the gate is `Display.@display(mut f Fmt)` — `spec/decisions/03-syntax.md:2597` vs `spec/decisions/02-types.md:17429`
* Display default body: D183 amendment ships `Display` with a `str.from(@)` default; D422 makes `@display` REQUIRED with no default — `spec/decisions/02-types.md:8290` vs `spec/decisions/02-types.md:17416`
* Sized-int overflow: D272 still describes sized types as wrapping; D423 makes trap the default for all `Ints` — `spec/decisions/09-tooling.md:2777` vs `spec/decisions/04-effects.md:7712`
* Leading-`|` sum syntax (retired by D406) still used as live examples/tables in active blocks — `spec/decisions/02-types.md:525-526` vs `spec/decisions/03-syntax.md:4155-4159`
* D47: function visibility is two-level, 'третьего «совсем-приватного» уровня нет'; D307 defines a third narrower level `priv(file) fn` — `spec/decisions/07-modules.md:944` vs `spec/decisions/02-types.md:15668`
* D307 error table makes `priv(<other>)` on a top-level item E_PRIV_QUALIFIER; D457 makes `priv(package)` on top-level fn/type legal — `spec/decisions/02-types.md:15701` vs `spec/decisions/02-types.md:18412`
* Location/mechanism of char Unicode methods: D286/D287 say moved into prelude core as inherent (injection removed); D308 says moved back to std.unicode with resolver-injection restored — `spec/decisions/07-modules.md:727` vs `spec/decisions/07-modules.md:2412`
* D125 still specifies the inline `module X allow_prelude_shadow` clause; D371 says inline clauses were removed and are a hard error — `spec/decisions/08-runtime.md:3802` vs `spec/decisions/07-modules.md:2352`
* D6 (active) still prescribes the retracted `realtime nogc { }` block and explicitly denies the fn-signature mechanism that D172 made the only one — `spec/decisions/05-memory.md:71` vs `spec/decisions/06-concurrency.md:5749`
* D32 example uses the retired `Realtime` effect in a function signature — `spec/decisions/02-types.md:3141` vs `spec/decisions/05-memory.md:69`
* D63's capability-sandbox composition example prescribes the retracted `realtime nogc { }` block as current syntax — `spec/decisions/04-effects.md:3835` vs `spec/decisions/06-concurrency.md:5749`

## 5. Что аудит НЕ покрыл

* Все 8 тем прочитаны (memory-abi-layout — со второго захода).
* Верификация тем modules-visibility, concurrency-runtime, sums-match-variants,
  numbers-conversions, memory-abi-layout — частично не выполнена (лимит сессии,
  дважды); их кандидаты — в разделе 4. bounds-protocols, effects-fail,
  ownership-consume-ro — доверифицированы полностью.
* Обзорные файлы spec/*.md (кроме одного попадания в sums) и D-блоки вне
  тематических списков — не входили в периметр.
* Синтез-агент не отработал — этот файл собран интегратором из сырых результатов.

## 6. Классы причин (по подтверждённому и снятому)

1. **Рост амендментами без сводного блока** — стар. текст остаётся «нормативным по
   букве» (D62/D86 про `?` против D85; D54-семейство конверсий против D410/D422/D430;
   ретракты `handler`, leading-`|`, `-> consume T` живут в примерах). Лекарство —
   сводные блоки типа D464 + пометки «ПЕРЕКРЫТО» на старых местах + страж на
   снятые формы (образец: check-retracted-param-form).
2. **Пример противоречит правилу соседнего блока** — D145 `fn[T From[K], K]` против
   D72 «forward-ref запрещён» (подтверждено; чекер, к слову, порядок НЕ проверяет —
   отдельная дыра спека-против-реализации, std сам пишет forward-order).
3. **Два блока делят одну поверхность без ссылок друг на друга** — D12 против D448
   (совместимость fn-типов по эффектам), D463 против D416 (тип err в on_child_fail,
   Decision.Restart). Опасно: обе стороны молодые, обе живые.
4. **Алгоритм реализации записан как правило языка** — D118 (порядок диспетча),
   D381 (эвристика резолва варианта), D100/D287 (резолвер): класс D285 §3, снятый
   сегодня D464-перемаркировкой; та же операция нужна этим блокам.

## 7. Порядок работ (предложение)

1. После 17:20 МСК возобновить воркфлоу (resumeFromRunId=wf_d42e662d-740):
   дочитать memory-abi-layout, доверифицировать раздел 4, перегенерировать синтез.
2. Подтверждённые из раздела 1 — по одному D-амендменту на строку таблицы, каждое
   слияние со сводным блоком-победителем (образец D464). Приоритет — те, где обе
   стороны живые (D12/D448, D463/D416): там неясно, ЧТО реализовывать.
3. Раздел 3 — механическая волна «ПЕРЕКРЫТО»-блок-цитат; кандидат на opencode-бриф.
4. Завести страж «пример в D-блоке компилируется» хотя бы для новых блоков — класс 2
   целиком из непроверяемых примеров (родня №442, уже в реестре).

