<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 196 — B11q/B11r остаток (карта задания «B11q/B11r»), чекпойнт

**Worktree:** `nova-196b11`, ветка `p196-b11q`. **База:** main `5c775de3b`
(merge `p196-closeout`). **Модель:** sonnet.

---

## Задача

Остаток B11q/B11r из реестра. Прошлая волна (196-closeout, см.
`docs/plans/wip/196-closeout-notes.md` П2) поймала живой хит на ПЕРВОМ репро
(`Option[T Debug]@debug`, `std/src/prelude/protocols.nv:732`) — опровергла
«B11q мёртв», но не объяснила ПОЧЕМУ основной канал (`resolved_types`) не
берёт этот тривиальный concrete-return метод. Эта волна — доводит диагноз до
корня и решает: чинить основной путь или честно задокументировать класс.

## Методология

Восстановлен ICR-трейс (`NOVA_TRACE_ICR=1`, уже был в дереве, не потребовалось
воссоздавать panic-код) + НОВЫЙ точечный env-gated зонд
(`NOVA_196_B11Q_SITE_TRACE`, `#[cfg(debug_assertions)]`, нулевая цена в
release) — печатает span + `current_fn_name` + `ExprId` В ТОЧКЕ входа в
B11q/B11r bucket. Оставлен в дереве (тот же паттерн, что
`NOVA_B5_MEANINGFUL_TRACE` прошлой волны) — комментарий `[M-196-b11q-root-cause]`
на обеих ветках (B11q ~emit_c.rs:53661, B11r ~emit_c.rs:53779).

Изолированный repro (НЕ conformance-фикстура — см. блокер ниже):
`Some(Some(42))` / `"${x:?}" == "Some(Some(42))"`, скомпилирован через реальный
пайплайн (`nova test <file>.nv`, resolve_imports_inline).

## Корень найден

**Трейс:** `[B11Q-SITE] method=debug obj_ty=NovaOpt_nova_int span=... current_fn=Some("debug") expr_id=ExprId(11856) expr_id_set=true`.

`current_fn=debug` + `expr_id_set=true` — ключевые факты: паника срабатывает
**ИЗНУТРИ ТЕЛА `Option[T Debug]@debug` САМОГО**, не из пользовательского кода;
`ExprId` УЖЕ присвоен (не проблема нумерации, `number_exprs.rs` нумерует
`Stmt::Expr` наравне со всеми). Прочитан сгенерированный `.c`
(`Nova_Option_method_debug_NovaOpt_nova_int` — моно для T=Option[int], т.е.
внешний Option обёрнут вокруг ВНУТРЕННЕГО Option[int]): тело содержит
`Nova_Option_method_debug_nova_int(v, f)` — рекурсивный вызов `v.debug(f)`, где
`v: T` внутри ДЕКЛАРАЦИИ метода — это ГОЛЫЙ generic type-param, связанный
протоколом `Debug` (`fn Option[T Debug] @debug(...)`), НЕ конкретный тип.

**Почему Channel 2 (`resolve_instance_method_return_arity` /
`infer_method_call_channel_type`, `types/mod.rs`) не резолвит `v.debug(f)`:**
рецептор — голое имя `T` (`TypeRef::Named{path:["T"]}`). Функция пробует по
очереди:
1. `method_overloads("T", "debug")` — None (нет типа "T").
2. "protocol-ресивер" ветка (~16138): `self.types.get("T")` — None ("T" не
   зарегистрированное имя протокола; ветка предназначена для параметров,
   ЯВНО типизированных именем протокола, `w Writer`, не для generic-параметра
   С БАУНДОМ на протокол).
3. `resolve_prefix_generic_method_return` — сканирует `method_table` в поисках
   ЛЮБОЙ ДРУГОЙ декларации с ТЕМ ЖЕ буквальным именем generic-параметра ("T")
   как bare-typevar ресивером — совпадение СЛУЧАЙНОЕ (по имени, не по
   баунду), порядок недетерминирован (`HashMap::iter()`).

Ни одна ветка не знает: «T» — это ИМЯ ТЕКУЩЕГО ГЕНЕРИКА В СКОУПЕ, чей БАУНД —
протокол `Debug`, и вызов должен диспетчеризоваться на `Debug`'s декларацию
метода. Причина отсутствия: `gs` (генерики в скоупе, нить через 31 вызов в
`types/mod.rs`) имеет тип `&HashSet<String>` — **только имена, БАУНДЫ
потеряны** до момента резолва вызова. Чтобы резолвить корректно, нужен
параллельный «имя → баунды» контекст (текущий проверяемый `FnDecl.generics`,
которые ДЕЙСТВИТЕЛЬНО несут баунд `Debug` для `T`) — новое cross-cutting
состояние, не локальный патч.

**Вывод:** это НЕ баг резолв-каналов/fallback-движков как таковых (моя зона),
а структурный ПРОБЕЛ в protocol-BOUND method dispatch для generic-receiver —
концептуально смежно с «protocol-resolution» (зона ДРУГОГО агента этой волны
по брифу). Фикс потребовал бы:
- либо расширить `gs` (и все 31 call-site) до «имя → баунды» — широкий blast
  radius, конфликт с параллельной работой в `types/mod.rs`;
- либо добавить НОВОЕ состояние «текущие generics-с-баундами проверяемой
  fn» — новая архитектура, не точечный патч.

**НЕ предпринято в этой волне** (риск half-baked + вне зоны). Задокументировано
для следующей волны — маркер `[M-196-b11q-root-cause]`, полный текст в
комментарии над B11q (`emit_c.rs`).

## Снос B11q/B11r: ❌ НЕ ВЫПОЛНЕН (подтверждено, живой корень найден)

Тот же вердикт, что и прошлая волна («живой»), но теперь с ТОЧНЫМ механизмом:
**ЛЮБОЙ вызов `.debug()`/`.display()` НА ВНУТРЕННЕМ значении ИЗНУТРИ
`Option[T Debug]@debug`/`Result[T,E Debug]@debug` (и их `@display`-близнецов,
`protocols.nv:768+`) бьёт в этот пробел** — т.е. это не «какой-то редкий
случай», а СТРУКТУРНО КАЖДЫЙ Debug/Display вызов на Option/Result ЛЮБОГО
внутреннего типа проходит через B11q/B11r legacy (легаси корректно отвечает —
`infer_method_level_return_for_sum` резолвит объявленный `()` — просто НЕ
Channel 2). Снос физически НЕВОЗМОЖЕН без закрытия protocol-bound-dispatch
пробела.

## ⚠ БЛОКЕР (найден инцидентально, ВНЕ моей зоны, СРОЧНО для владельца)

При попытке прогнать авторитетный `spec_tests/conformance` мега-CU гейт —
**ЛЮБОЙ** `nova test spec_tests/conformance/<любой-файл>.nv` (single-file ИЛИ
folder) падает internal error:
```
nova: internal error at emit_c.rs:54073(pristine)/54132(patched):
[P67-LEGACY] method call `.write_at` return type unknown — checker must
annotate (compiler-conventions.md §0); obj_ty="" obj=Ident(q) expr span=...
```
**Корень изолирован:** `spec_tests/conformance/d216_ptr_methods_174_5.nv:17-18`
```nova
mut q = buf.ptr()
q.write_at(1, 99)
```
`buf.ptr()` на `mut buf []int` резолвится по-разному в зависимости от
BINDING-модификатора локальной переменной: `ro p = buf.ptr()` (строка 12,
тот же файл) РАБОТАЕТ (`.read_at` резолвится), но `mut q = buf.ptr()` — та же
RHS-экспрессия — НЕ резолвится для `.write_at` (checker's write-cap gate
~types/mod.rs:7901 молча no-op'ит на `infer_expr_type(obj,scope)==None`, и
downstream return-type резолв ТОЖЕ терпит неудачу, `obj_ty=""` в codegen —
пустая строка, не просто "неизвестный примитив").

**Почему это блокирует ВСЁ:** `spec_tests/conformance` — ОДИН модуль
(`module spec_tests.conformance`, папка = один модуль из co-equal файлов,
см. `reference-nova-module-model-folder`). Значит **любой** файл в этой
папке при компиляции линкует ВСЕ co-equal файлы модуля В ОДИН compile-unit —
включая `d216_ptr_methods_174_5.nv`. Отсюда: `nova test
spec_tests/conformance/d30_try_op_unwrap_pair.nv` (единственный файл-аргумент,
НИКАК не про pointers) падает С ТЕМ ЖЕ P67 — потому что модуль целиком
утягивает d216.

**Верификация «не моя правка»:** A/B на pristine (git stash моей правки
emit_c.rs, чистый `cargo build --release`, БЕЗ RUSTFLAGS) vs patched — ОБА
падают ИДЕНТИЧНО (тот же `obj_ty=""`, `obj=Ident(q)`, тот же span/tag,
отличается только номер строки панического `panic!()` — сдвинут ровно на
количество добавленных мной строк комментария). Регрессия НЕ из этой волны.

**Верификация «не универсальная поломка компилятора»:** флагман
(`nova check --strict-effects examples/flagship/aggregator/src/main.nv`,
`NOVA_OFFLINE=1`) — **PASS: 1 FAIL: 0 WARN: 33** НА ОБОИХ (pristine и patched)
— компилятор в целом здоров, поломан именно `spec_tests/conformance`
folder-CU через d216.

**История фикстуры:** `d216_ptr_methods_174_5.nv` не менялась недавно
(последний коммит — `c6c4a7af0`, Plan 174.5 original). Регрессия пришла
СНАРУЖИ — между `196-closeout`'s baseline (main `58804953d`, гейт был зелёным
126/0/16 В ТОТ ДЕНЬ) и текущим main `5c775de3b` слились
`p208-sh4-teardown` (`4ad3c8d10` — снос `conv.h nova_fmt_*`/`*_display_spec`
рефактор) и другие; наиболее вероятный подозреваемый —
`p208-sh4-teardown`'s rich-spec/Debug-refactor коммит, но НЕ подтверждено
git-bisect'ом (вне бюджета этой волны — не моя зона, D216/pointer-typing).

**Действие:** НЕ чинил (out-of-zone: pointer-intrinsic typing, Plan
174.5/D216, а не resolve-channel/fallback-engine зона этого задания; замечено
`nova-216tails` worktree активен параллельно — вероятно уже покрывается
другим агентом). Задокументировано здесь + в реестре максимально точно
(файл/строки/repro/A-B-верификация) для владельца/следующей волны —
**это блокирует ЛЮБОЙ будущий мега-CU гейт по spec_tests/conformance**, не
только эту волну.

## Гейты этой волны (то, что БЫЛО возможно прогнать)

- `cargo build --release` (compiler-codegen + nova-cli, ОБЫЧНЫЙ профиль, БЕЗ
  RUSTFLAGS) — 0 errors, ОБА (pristine И patched).
- Изолированный repro (`Some(Some(42))` / `${x:?}`, ВНЕ spec_tests/conformance
  — свой module-namespace, не утягивает d216) — **PASS: 1 FAIL: 0** на
  patched-бинаре, идентично pristine.
- Флагман (`nova check --strict-effects examples/flagship/aggregator/src/main.nv`,
  `NOVA_OFFLINE=1`) — **PASS: 1 FAIL: 0 WARN: 33** на pristine И patched
  (идентичные warning-списки).
- **Полный `spec_tests/conformance` мега-CU гейт — НЕ ПОЛУЧЕН** (блокер выше,
  не моя правка, детально задокументирован). Честно: НЕ заявляю зелёный гейт,
  которого не было.
- Точечные 10-15 фикстур из карты прошлой волны — НЕ получены по той же
  причине (все внутри `spec_tests/conformance`, все утягивают d216).

## Снесено физически: 0 (не менялось с прошлой волны)

B11q/B11r остаются ЖИВЫМИ — корень найден (protocol-bound generic-receiver
dispatch пробел в Channel 2), фикс НЕ предпринят (вне зоны/риск
half-baked/широкий blast radius на 31 `gs`-сайт), задокументирован
(`[M-196-b11q-root-cause]`) для следующей волны — фикс потребует либо
расширения `gs` до «имя→баунды» через ВСЕ 31 сайт, либо нового cross-cutting
«текущие generics проверяемой fn» состояния в чекере — оценка: отдельная
волна, соседняя с protocol-resolution зоной.

## Коммиты (ветка `p196-b11q`, база main `5c775de3b`)

- Комментарии + env-gated site-probe на B11q/B11r (`emit_c.rs`) — 0
  поведенческих изменений (verified A/B), root-cause diagnosis.
- Этот файл + обновление `docs/plans/196-one-truth-closeout.md`.

Модель: sonnet.
