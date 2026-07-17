<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 196 — Зона CH, продюсеры второй оси (resolved_types_buf): чекпойнт
(sonnet, worktree `nova-196rtb`, ветка `p196-rtbuf-producers`)

**Родитель:** [196-campaign-map.md](196-campaign-map.md) §«Зона CH» + [196-ch-coverage2-notes.md](196-ch-coverage2-notes.md)
§4 (рекомендация: капстоун-разморозка требует Q1/Q2/Q5/Q6/Q9-продюсеров `resolved_types_buf`,
НЕ Q10 `node_substs`). **Задание этой волны:** продюсеры/расширения канала `resolved_types`
(Channel-1/2), закрывающие Q1-класс (static-return) и Q6-класс (elem) для веток
[196-capstone-notes.md](../196-capstone-notes.md) §3.4 (Core-32); плюс B07-исключение
(24 сайта с ch2=true, нужна emit_c-консьюмер-композиция).
**База:** main `cb409a927`.

---

## Итог одной строкой

4 новых продюсера в `types/mod.rs` (types/mod.rs-only, emit_c.rs НЕ тронут в финальном
диффе — фрозен-зону не правил): (1) Q6/elem — typed-pointer intrinsic-методы
(read/write/offset/dist/copy_*) через `ResolvedType::TypedPtr`; (2) Q1/static-return —
`[]T` slice-sugar static-ctor (`Member{obj: Path(["__array",elem])}`), ранее вообще не
видимый чекеру как канал-кандидат; (3) Q1/static-return — bare free-fn call declared
return (`self.sig.fn_decls`); (4) Q1/static-return — newtype/named-tuple constructor
(`TypeDeclKind::Newtype`/`NamedTuple`). Верифицировано на 4 корпусах (debug+release,
`nova test`): PASS δ0 везде (collections 13/0/6skip, time+encoding 14/0/8skip, math
3/0/2skip, флагман `aggregator --strict-effects` чисто). **B10f_user_fn_sigs и
B10m_ident_empty_fallback полностью закрыты** на collections+time+encoding (0 хитов);
**B10h/B10l закрыты** на std/src/math (их основной трафик по census). B01/B11a/B11d
остаются reachable, но с существенно сниженным трафиком (счётчики продюсеров: 2204
typed-ptr хита на collections, 1572 `[]T`-ctor хита на collections) — остаток
архитектурно того же класса, что уже задокументированный B1 (`copy_n_nonoverlapping`/
`T.deserialize` внутри ещё-generic тела, ch-coverage2-notes §2): abstract-T call-сайты
внутри `Vec[T]`/generic-тел, честно gs-гейтированы, НЕ закрываемы pre-mono чекером.
**B07-исключение:** карта собрана (§5), композиция НЕ реализована — обоснование ниже.

---

## 1. Producer Q6/elem — typed-pointer intrinsic methods

**Находка:** `B11d_typed_pointer_methods` (капстоун Core-32, КРУПНЕЙШИЙ трафик — 5897 хитов
по census) — обслуживает `*T`/`*mut T @read/write/read_at/write_at/read_unaligned/
write_unaligned/read_volatile/write_volatile/offset/dist/copy_from(_nonoverlapping)/
copy_to(_nonoverlapping)` (`is_raw_pointer_intrinsic_method`, `types/mod.rs:34183`,
D216 §21 table) — ноль Channel-2 покрытия: чекер никогда не аннотировал возврат этих
call-сайтов, легаси `infer_call_ret_c` ре-derive'ил через obj_ty C-строку.

**Фикс:** новый продюсер в `f1_expr`'s `ExprKind::Call` каскаде (сосед TurboFish-static-
ctor/module-qualified-static/serialize продюсеров): если ресивер резолвится в ЗАКРЫТЫЙ
(`rt_is_closed`-гейт, прецедент `137167c54`) `ResolvedType::TypedPtr(modifier, inner)`,
пойнти (`inner`) — само по себе искомое значение (read* → inner, write*/copy_* → Unit,
offset → тот же TypedPtr, dist → int). `*()`-пойнти и residual-generic-carrier (gs-гейт)
исключены — падают в легаси нетронутыми.

**Замер (debug, temp trace, снят перед коммитом):** 2204 хита на `std/src/collections`
solo. `nova test std/src/collections` PASS 13/0/6skip (без δ, debug+release);
`std/src/time`+`std/src/encoding` PASS 14/0/8skip (без δ). `B11d` остаётся reachable —
ожидаемо: остаток — generic-body abstract-T сайты (тот же класс, что B1), см. `rt_is_closed`
+ gs-гейт корректно отклоняет их, оставляя легаси единственно верным источником для НИХ.

**Коммит:** `bc61b5026`.

---

## 2. Producer Q1/static-return — `[]T` slice-sugar static ctor

**Находка:** `B01_turbofish_member_generic_type` (3926 хитов, campaign-map W2-группа)
срабатывает для ДВУХ разных AST-форм: (а) буквальный TurboFish-Member static call
(`HashMap[str,int].new()`) — УЖЕ покрыт существующим продюсером (`mo.kind` matches
`TurboFish`); (б) `[]T` slice-sugar (`[]u8.new(...)`/`.with_capacity`/`.from`/`.of`/...) —
парсится в СОВЕРШЕННО ДРУГУЮ форму: `Member{obj: Path(["__array", elem]), name}`
(`generic_static_receiver`'s doc). Для (б) чекер НИКОГДА не аннотировал канал вообще —
легаси `infer_call_ret_c` для НЕЁ пере-синтезирует эквивалентный `Vec[elem].method(...)`
TurboFish-вызов С `id: ExprId::UNSET` (свежий AST-узел на эмит-время) — Channel 1/2 ОБА
гейтированы на `expr.id.is_set()`, так что даже если бы чекер канализировал форму (б) НА
пере-синтезированном узле, у Channel 2 не было бы ключа для поиска. Решение — канализировать
на ОРИГИНАЛЬНОМ вызове (`e.id`, ещё set), ДО того как легаси вообще дойдёт до ре-синтеза.

**Фикс:** сиблинг-арм TurboFish-продюсера, гейт на `mo.kind` = `Path(["__array", elem])`.
Тот же `infer_expr_type(e, scope)`, что и TurboFish-арм — у него УЖЕ есть арм для ИМЕННО
этой формы (ctor-имена new/with_capacity/from/default/filled/of → `Array(Named(elem))`),
просто никогда не канализировался.

**Замер:** 1572 хита на `std/src/collections` solo (temp trace, снят). PASS δ0 (13/0/6skip
debug+release; 14/0/8skip time+encoding). Побочный эффект: этот же продюсер закрывает
`B11a_array_static_method` для той же формы (`[]T.new()`/`.with_capacity()`, `emit_c.rs:
51620` — идентичный `Path(["__array",...])`-гейт) — B11a и B01 остаются reachable (тот же
generic-body-abstract-elem остаток, честно), но с тем же существенным снижением трафика.

**Коммит:** `5900ee5e5`.

---

## 3. Producer Q1/static-return — bare free-fn call declared return

**Находка:** `B10f_user_fn_sigs` (campaign-map W2-группа) — чекер канализировал только
`Some`/`println`/`print`/`assert` из `Ident(fname)`-каскада; ЛЮБОЙ другой bare free-fn call
(`name(args)`) пере-derive'ился на эмите через `user_fn_sigs`.

**Фикс:** сиблинг-арм, гейт `!scope.contains_key(fname)`. `self.sig.fn_decls.get(fname)` (тот
же free-fn реестр, что уже читает Producer D для turbofish-формы) → требует ОДИН
arity-matching non-generic оверлоад (та же single-candidate дисциплина, что Producer D) →
канализирует declared return (или `Unit` для `-> ()`), gs-гейт через `typeref_mentions_any`.

**Замер:** `std/src/collections` — **B10f_user_fn_sigs ПОЛНОСТЬЮ ОТСУТСТВУЕТ** в hit-сете
(13/0/6skip, без δ). `std/src/time`+`std/src/encoding` — **B10f И (побочно) B10m_ident_
empty_fallback** (его собственный doc: "хвост-fallback Ident-формы; уходит при сносе
E-кластера") **ОБА отсутствуют** (14/0/8skip, без δ). Не доказано 0 на полном conformance
(вне scope сессии), но 0 хитов на двух независимых нетривиальных корпусах без регрессии —
сильный сигнал капстоуну для ре-верификации на полном корпусе.

**Коммит:** `ba9a8a2f3`.

---

## 4. Producer Q1/static-return — newtype/named-tuple constructor

**Находка:** `B10h_newtype_constructor` + `B10l_named_tuple_constructor` — оба ключуются на
`self.type_aliases` (codegen-side C-строковая таблица) в легаси; у чекера НЕТ эквивалентного
канал-продюсера для bare `TypeName(args)` ctor-вызова, резолвящегося в один из этих двух
типов.

**Фикс:** проверяется ПЕРВЫМ в каскаде `!scope.contains_key(fname)` (перед free-fn lookup'ом
из §3): если `fname` — зарегистрированный тип с `TypeDeclKind::Newtype`/`NamedTuple`,
канализировать результат как голый номинальный тип (`Named{name, args:[]}`) —
`resolved_type_to_c`'s существующий by-name резолв уже умеет обе формы (тот же путь, что
любой другой Channel-2-покрытый ctor — record/sum/generic). Имя типа и имя fn никогда не
коллидируют в неймспейсе Nova — проверки взаимоисключающие по конструкции, не по порядку.

**Замер:** `std/src/math` (основной трафик B10h/B10l по census — math: 26/46 хитов
соответственно) — **НИ ОДНА ветка не появляется** в hit-сете (3/0/2skip, без δ).
collections+time+encoding — PASS без δ (эти ветки там и раньше не хитовали — не их трафик,
подтверждает отсутствие регрессии, не закрытие).

**Коммит:** `90328e908`.

---

## 5. B07-исключение — карта для капстоун-агента (композиция НЕ реализована)

**Задание:** 24 сайта `B07_generic_type_instance_method` (капстоун Core-32, 3072 общего
трафика) уже имеют `ch2=true` (census §2) — Channel-2 ЗАПИСЬ есть, но верхнее чтение её
пропускает. Нужна emit_c-консьюмер-композиция; если реализация ВНЕ frozen-зоны — сделать
самому; если ВНУТРИ — карта в отчёт.

### 5.1 Точная механика гейта (подтверждено чтением кода, `emit_c.rs`)

- `infer_expr_c_type` (52547+, **ВНЕ frozen-зоны**) — Channel-2 блок (~52737-52792) читает
  `self.resolved_types.get(&expr.id)`, лоуэрит через `resolved_type_to_c`. Для `Call`-выражений
  есть ЯВНЫЙ stub-skip-гейт (~52784-52789, `[M-into-raw-generic-stub-ret]`): если
  `debt_is_generic_stub_c(&ir_c)` — НЕ возвращает, падает в «substitution-aware inference
  below» (по факту — весь остаток каскада вплоть до `infer_call_ret_c`).
- `infer_call_ret_c` (50418-52546, **frozen-зона**, монополия капстоун-агента) — B07
  (~50906) резолвит через `generic_type_instance_info`/`generic_type_methods` +
  `resolve_instance_call_subst` (POST-mono, receiver-instance-aware) — это ИМЕННО тот путь,
  который stub-skip-гейт выше НАМЕРЕННО оставляет для «substitution-aware inference».

**Вывод:** 24 сайта — НЕ баг канала (запись есть, но её ЗНАЧЕНИЕ — erased stub,
`Nova_T*`-подобный, для метода на generic-type-instance ресивере, где own-generic'и метода
ещё не резолвлены на CHECK-время) — stub-skip-гейт РАБОТАЕТ КАК ЗАДУМАНО, откладывая на
подстановочно-осведомлённый B07. «Композиция» = не «доверять каналу вместо B07», а
**точечно воспроизвести НУЖНУЮ ЧАСТЬ B07's substitution-логики ПРЯМО В stub-skip-точке**
(она формально вне frozen-зоны).

### 5.2 Почему НЕ реализовано в эту сессию (обоснование, не отговорка)

1. **Побочный эффект, не просто строка типа.** B07r (~50973) вызывает
   `self.register_generic_instances_in_typeref(ret_ty, &subst)` — регистрирует НОВЫЙ
   generic-instance в worklist для эмита. Собственный комментарий кода документирует
   исторический баг: пропуск этой регистрации → `.collect()`/`.next()` на temporary
   впоследствии падает в erased NULL-stub → **сегфолт**. Композиция, которая читает ТОЛЬКО
   тип (а не воспроизводит регистрацию), тихо вносит РОВНО этот класс регрессии.
2. **Полная логика B07/B07r — ~100 строк** (overload-arity disambiguation по
   `generic_type_methods`, `resolve_instance_call_subst`, Self-спецкейс через
   `compute_generic_type_c_name`, value-aware wrapping через `value_aware_subst_to_ref`,
   регистрация инстансов). Воспроизвести НАДЁЖНО вне frozen-зоны без дублирования — тот
   самый anti-pattern «три hand-duplicated inference engines», прямо запрещённый ловушкой
   карты §4.2 («приёмка (2): разбросанное сведено в одно, НЕ наоборот»).
3. **Прецедент Stage-C2** (`196.5-stage-c2-notes.md`, B1/B2) показывает ПРАВИЛЬНУЮ
   дисциплину для такой композиции: propose-then-verify С ЯВНОЙ проверкой byte-identical
   против легаси (`legacy_pairs`, вычисляется безусловно) — НЕ слепое доверие. Построить
   такой гейт для B07 требует вызвать легаси-B07 ПАРАЛЛЕЛЬНО (для сравнения), что означает
   частичный доступ/дублирование самой frozen-функции — граница «читать можно, править
   нельзя» становится зыбкой именно здесь.
4. **Малый blast radius, не найден конкретный репро в эту сессию.** 24 сайта = 0.8% B07's
   трафика; census нашёл их на ПОЛНОМ conformance mega-CU (4040 входов), которого задание
   явно просит не гонять. Мой точечный поиск (temp-трейс на `vec_iter.nv`/`vec_lazy.nv` —
   census's же указанный основной B07-трафик — 200 REACHED-сайтов, ВСЕ `ch2=None`, НЕ stub;
   `std/src/collections` целиком — 1516 REACHED, 0 stub-skip; `d119_method_level_type_params.nv`
   — 0/0) НЕ нашёл ни одного из 24 ch2=true сайтов в доступном мне бюджете времени/корпуса —
   вероятно, они живут в файлах вне моей выборки (encoding/serde-глубже? std/os,fs? другой
   d-файл?). Без конкретного repro безопасная propose-then-verify композиция невозможна
   (сверяться не с чем).

### 5.3 Рекомендация капстоун-агенту (конкретный план действия)

1. **Локализовать 24 сайта.** Временный диагностик (снят мной, паттерн для повтора):
   в `infer_expr_c_type`'s stub-skip блоке (~52784) залогировать `expr.id`/`ir_c`/method
   при `debt_is_generic_stub_c`-срабатывании на `Call`; в B07 (~50906, ВНУТРИ frozen-зоны,
   но ЧТЕНИЕ инструментации разрешено) залогировать `self.resolved_types.get(&expr.id)`
   рядом с `icr_trace`. Прогнать НА ПОЛНОМ conformance (капстоун имеет мандат на это) —
   найти ПЕРЕСЕЧЕНИЕ (одинаковый `expr.id` в обоих логах = искомые 24).
2. **Композиция — ТОЛЬКО как продолжение B07's уже вычисленного `subst`/`ret_ty`, НЕ
   отдельная реализация.** Правильное место — ВНУТРИ B07 (frozen-зона, капстоун-монополия):
   если `resolved_types.get(&expr.id)` даёт stub — это ПОДТВЕРЖДЕНИЕ, что B07 — правильный
   путь (не альтернатива), просто добавить (внутри B07, gated) быстрый предварительный ПУТЬ,
   который считает то же самое ДЕШЕВЛЕ, если структура позволяет — но это по-прежнему
   правка frozen-зоны, вне моего мандата.
3. **Альтернатива вне frozen-зоны (безопаснее, но НЕ исследована мной):** если 24 сайта
   имеют общую УЗКУЮ форму (напр., все — `Self`-возврат fluent-методов на generic-instance
   ресивере — пересекается с B-fluent-generic продюсером `0fd827412` из ch-coverage2-notes,
   который УЖЕ решает соседнюю проблему для `node_substs`), возможно РАСШИРЕНИЕ ТОГО ЖЕ
   producer-класса в `types/mod.rs` (не читать/дублировать B07, а дать ЧЕКЕРУ материализовать
   НЕ-stub ответ С САМОГО НАЧАЛА для этих 24, тем самым устраняя самим stub, а не читая
   вокруг него) — это было бы Zone CH work (мой периметр), но требует сначала выполнить п.1
   (найти конкретные сайты), иначе нечего гейтировать/верифицировать.
4. **Не пытаться «прочитать канал напрямую в обход stub-skip-гейта»** — сам гейт правильный
   (защищает от РЕАЛЬНОГО erased-stub, не ложная тревога); снятие гейта регрессирует ВСЮ
   `[M-into-raw-generic-stub-ret]`-защиту (документированный исторический баг), не только
   эти 24 сайта.

---

## 6. Что НЕ тронуто / вне периметра

- **B10e_fn_param_sigs_first** (closure/fn-value LOCAL variable call, `ro get = pair.1;
  get()`) — НЕ атакован в эту сессию (требует scope-based Func-TypeRef lookup, отдельный
  класс от bare free-fn; кандидат для следующей волны, оценка: небольшая, тот же паттерн,
  что §3, но источник — `scope.get(name)` вместо `sig.fn_decls`).
- **B10j_generic_fn_mono_resolve / B10j_generic_fn_value_aware_return** (generic free-fn
  call, требует arg-based type-inference для связывания T из аргументов — Producer-A-класс
  работы, значительно больше, чем §3's non-generic случай) — НЕ атакован, требует отдельной
  сессии (unify_type-подобная инференс-логика, gs-гейт на turbofish-less generic calls).
- **frozen-зона `infer_call_ret_c` (50418-52546)** — НЕ правил (ни строки в финальном
  диффе — `git diff cb409a927..HEAD -- compiler-codegen/src/codegen/emit_c.rs` пуст).
  Диагностические trace-вставки (both в stub-skip и в B07) были ВРЕМЕННЫМИ, сняты
  (`git checkout`) до финализации коммитов.
- **Полный `spec_tests/conformance` gate** — не гонял (по заданию). Все замеры — debug+
  release на collections/time/encoding/math (13+14+3=30 PASS, 0 FAIL, 6+8+2=16 SKIP) +
  флагман `aggregator --strict-effects` (release, чисто).

---

## 7. Коммиты (ветка `p196-rtbuf-producers`, worktree `nova-196rtb`)

1. `bc61b5026` — Q6/elem producer: typed-pointer intrinsic methods → `resolved_types_buf`.
2. `5900ee5e5` — Q1/static-return producer: `[]T` slice-sugar static ctor → `resolved_types_buf`.
3. `ba9a8a2f3` — Q1/static-return producer: bare free-fn call declared return → `resolved_types_buf`.
4. `90328e908` — Q1/static-return producer: newtype/named-tuple constructor → `resolved_types_buf`.
5. (этот коммит) — docs: чекпойнт-заметки волны.

**Файлы:** `compiler-codegen/src/types/mod.rs` только (все 4 продюсер-коммита).
`emit_c.rs` — 0 строк диффа от базы (диагностика временная, снята). **В main НЕ мёржено.
Push запрещён по заданию.**
