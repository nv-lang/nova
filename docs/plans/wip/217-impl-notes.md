<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 217 — заметки реализации (гибрид C, авто-`@cleanup`)

> Рабочий чекпоинт волны. Модель sonnet. Ветка `p217-auto-cleanup`,
> worktree `d:/Sources/nv-lang/nova-217`. Удаляется при закрытии волны
> (история в git) — dev-workflow.md §4.

## Статус по фазам (2026-07-20)

- **Ф.0 Спека** — ✅ ЗАВЕРШЕНА. Новый D-блок [D432](../../../spec/decisions/02-types.md#d432)
  (`spec/decisions/02-types.md`, сразу после D133) + амендменты: D133
  §«Что отвергнуто» (Drop-method auto-cleanup смягчён), D180 Rule 6
  (`05-memory.md`, не применяется к `@cleanup`-типам), D314 exit-таблица
  (`03-syntax.md`, добавлена строка `break`/`continue` → `Success`).
- **Ф.1 Чекер** — ✅ ЗАВЕРШЕНА (keystone-скоуп). `types/mod.rs`:
  `LinearityRegistry.cleanup_pure_types` + `has_pure_cleanup()`;
  `check_obligations_at_exit` больше не эмитит `D133-not-consumed`/
  `D156-strict-forget` для `Live`/`MaybeConsumed` квалифицирующихся типов.
- **Ф.2 Codegen** — ✅ ЗАВЕРШЕНА (keystone-скоуп, включая честно
  задокументированный пробел — см. «Известные пробелы» ниже).
- **Ф.3 Фикстуры** — ✅ ЗАВЕРШЕНА (keystone-матрица; см. таблицу ниже).
- **Ф.4 std (`@cleanup` 8 ресурсам)** — НЕ моя, отдельный подплан
  [217.1](../217.1-cleanup-resource-rollout.md) (план §7 Ф.4, владелец
  §8а п.4).

## Ф.1 — чекер (details)

Файл: `compiler-codegen/src/types/mod.rs`.

- `LinearityRegistry` (структура, ~строка 26100): новое поле
  `cleanup_pure_types: HashSet<String>`. Populated в `build()`: для
  каждого `Item::Fn` с `recv.consume && recv.kind==Instance && fd.name==
  "cleanup" && fd.effects.is_empty()` — `recv.type_name` вставляется.
  Метод `has_pure_cleanup(&self, ty: &str) -> bool`.
- `check_obligations_at_exit` (~строка 27639): добавлен
  `is_auto_cleanup_eligible = !is_strict_generic &&
  self.lin_reg.has_pure_cleanup(&ty)`; match расширен guard-рукавами
  `Some(VarState::Live) if is_auto_cleanup_eligible => {}` /
  `Some(VarState::MaybeConsumed(_)) if is_auto_cleanup_eligible => {}`
  ПЕРЕД существующими error-рукавами (Rust match — порядок решает).
  Никакой АСТ-мутации в чекере нет — codegen НЕЗАВИСИМО (см. ниже)
  переоткрывает то же условие эффект-чистоты своим собственным pre-pass'ом
  (см. риск divergence ниже).

## Ф.2 — codegen (архитектура; ключевая находка волны)

Файл: `compiler-codegen/src/codegen/emit_c.rs`.

### Ключевая находка: механизм уже существовал наполовину

`DeferEntry` (struct, ~строка 1877) уже несёт поле
`consume_policy: Option<ConsumePolicy>` — предназначенное для
consume-flavored entries. До 217 оно заполнялось ТОЛЬКО
`enter_consume_defer_scope` (собственный, ИЗОЛИРОВАННЫЙ push на
`self.defer_scopes`, используемый исключительно explicit
`consume X = e { body }` формой). Но ТРИ из четырёх run-сайтов уже умели
generic `consume_policy`-ветку ДО 217: `leave_defer_scope` (normal exit),
`emit_early_exit_cleanup` (return/break/continue). Четвёртая пара
run-сайтов — ДВА inline-блока ВНУТРИ `enter_defer_scope` самого (FAIL —
тело блока throw'ит, перехватывается СОБСТВЕННЫМ setjmp; INTERRUPT —
`with Fail = |e| interrupt ()`-конверсия) — НЕ умела, это найдено ТОЛЬКО
эмпирически (throw-path фикстура, см. коммит `33ca6eb77`).

### Keystone-реализация (что сделано)

1. `Emitter` (struct) — новые поля: `auto_cleanup_types: HashSet<String>`
   (зеркало `LinearityRegistry.cleanup_pure_types`, СВОЙ pre-pass —
   см. «Риск divergence»), `auto_cleanup_arm_sites: HashMap<Span,
   (usize,usize,String)>`, `auto_cleanup_active: Vec<(String,usize,usize)>`,
   `consume_receiver_methods: HashMap<String,HashSet<String>>` (зеркало
   checker's `consume_methods`, гейтит receiver-дизарм — см. ниже).
2. `enter_defer_scope`'s prologue-скан (~строка 25811) расширен: для
   bare `Stmt::Let(consume, Ident-pattern)` квалифицирующегося типа —
   создаёт `DeferEntry{consume_policy: Some(...)}` В ТОМ ЖЕ per-block
   `entries`-векторе, что и обычные `Stmt::Defer`. `c_binding` ВЫЧИСЛЯЕТСЯ
   ПРЯМО ЗДЕСЬ (не откладывается — критичный урок, см. «Баги, найденные и
   починенные» #2), переменная ХОИСТится (pre-declared `T name = NULL/0/
   {0};` перед setjmp-обвязкой + `hoisted_let_vars`) — та же дисциплина,
   что существующий `errdefer_refs`-механизм, но для СВОЕГО же биндинга.
3. `emit_stmt` переименован в `emit_stmt_inner` + тонкая обёртка
   `emit_stmt`: после `Stmt::Let` — ищет `auto_cleanup_arm_sites` по
   `decl.span`, если есть — `emit_auto_cleanup_arm` (shield-enter +
   `_active=1`, ResourceTrace-enter).
4. `emit_expr` переименован в `emit_expr_inner` + тонкая обёртка
   `emit_expr`: ЦЕНТРАЛЬНЫЙ choke-point для receiver-дизарма
   (`disarm_auto_cleanup_receiver_call`) — см. «Баги» #3 почему это
   понадобилось (block.trailing choke-points ~20+ мест).
5. `enter_defer_scope`'s ДВА inline run-сайта (FAIL/INTERRUPT) — добавлены
   `consume_policy`-ветки, зеркалящие `enter_consume_defer_scope`'s
   собственные копии (`DeferOutcome::FromFrame`/`Interrupt`,
   `ConsumeTail::FailChain`/`Swallow`).
6. `leave_defer_scope` — добавлен `auto_cleanup_active.retain(|(_,bid,_)|
   *bid != block_id)` (единственная точка очистки — каждый блок проходит
   через `leave_defer_scope` ровно один раз).

### Drop-флаг (§8а п.6а) — как реализован

Владелец выбрал (а) рантайм drop-флаг. Реализация НЕ добавляет отдельный
"drop-флаг" концептуально — переиспользует СУЩЕСТВУЮЩИЙ `_active`-флаг
паттерн (тот же, что `defer`/`consume{}` уже используют для «add armed at
declaration, disarm at consuming operation, read at exit»). Флаг взводится
ПОСЛЕ захвата ресурса (partial-init safety), сбрасывается на
доказанно-безопасных дизарм-точках (см. ниже), читается ТОЛЬКО на exit —
если один branch дизармил, другой — нет, при join'е поведение КОРРЕКТНО
per-branch (это и есть MaybeConsumed-безопасность, без отдельной
статической классификации Live/MaybeConsumed на стороне codegen).

### Дизарм-точки — ЧТО ДОКАЗАНО безопасно, что НЕТ

**Безопасные (реализованы):**
1. `return X` (голый идентификатор) — D133 «Returned»-способ, безусловный.
2. Receiver-вызов `X.method()` (bare statement ИЛИ block-trailing ИЛИ
   nested — покрыто центральным `emit_expr`-choke-point'ом), ГДЕ `method`
   зарегистрирован в `consume_receiver_methods[type_of(X)]` — зеркало
   checker's `is_consume_method`. Гейт ОБЯЗАТЕЛЕН: без него ЛЮБОЙ метод
   (в т.ч. read-only getter) ложно дизармил бы → утечка (нашёл сам себя
   при разборе `consume_walk_expr`/`consume_args`, до написания кода).

**Известный пробел (НЕ реализовано, честно задокументирован в D432 §4 +
здесь):** передача биндинга как ПРЯМОГО аргумента вызова (`foo(g)`, НЕ
receiver-позиция) НЕ дизармится. Причина: checker's `consume_args`
консюмит арг ТОЛЬКО если callee's параметр РЕАЛЬНО consume-режима
(`consume_idxs`) — codegen НЕ переоткрывает этот анализ. Слепой
"disarm on any direct Ident arg" (как исторически делает
`reconsume_scopes`-механизм для re-consume `consume X { body }` блока)
БЕЗОПАСЕН ТАМ только благодаря checker's `block_guards`
(E_CONSUME_BLOCK_MOVE_OUT отвергает любое несанкционированное появление
ДО кодгена) — для bare auto-cleanup биндинга такого ограждения нет,
слепой дизарм дал бы ТИХУЮ УТЕЧКУ. Явно НЕ реализовано, а не забыто.
Маркер `[M-217-consume-param-transfer-disarm]` — завести в
backlog-followups.md при закрытии волны. Follow-up: вычислить
per-callee `consume_idxs` в codegen (зеркало checker), включить путь.

**Также НЕ реализовано (secondary, не блокирует keystone):** watchdog-
таймаут (D188 R3, `threshold_var` жёстко "0" — disarmed watchdog, но
`nv_consume_enter_shield`/`leave_shield` ЕСТЬ, cancel-mask корректен).
ResourceTrace (D185 R1) — наоборот, ПОДКЛЮЧЁН (дёшево, `has_resource_trace:
self.effect_schemas.contains_key("ResourceTrace")`) — паритет с блок-формой.

### Риск divergence чекер/codegen (зафиксирован, не устранён)

Чекер (`LinearityRegistry.cleanup_pure_types`) и codegen
(`auto_cleanup_types`) НЕЗАВИСИМО сканируют `module.items`+`peer_files` по
ОДИНАКОВОМУ критерию (`f.name=="cleanup" && recv.consume && Instance &&
effects.is_empty()`). Оба должны СОВПАДАТЬ на любом реальном модуле (то
же исходное дерево). Если они разойдутся (напр. будущий рефакторинг
поменяет один скан, забыв другой) — ЧЕКЕР МОЛЧИТ (не D133-ошибка), А
CODEGEN НЕ ВСТАВЛЯЕТ CLEANUP → тихая утечка. Это ТОТ ЖЕ паттерн риска,
что уже существовал ДО 217 между `LinearityRegistry` и
`consume_cleanup_types`/`record_consume_fields` (codegen's собственные
независимые пре-пассы) — не новый прецедент, но стоит зафиксировать:
если 217.1 (std-раскатка `@cleanup` на 8 типов) когда-то потребует менять
критерий (напр. добавить non-Instance receiver kind), ОБА скана
(types/mod.rs + emit_c.rs) должны меняться синхронно.

## Баги, найденные и починенные в ходе волны (честная хронология)

1. **Bare-statement receiver-call disarm не срабатывал вовсе** —
   `Stmt::Expr`-хук ловил только ПРЯМОЙ `Stmt::Expr`, но `X.method()` как
   ПОСЛЕДНЕЕ выражение блока (`if c { g.close() }`, `with EFFECT {...}
   { g.close() }`) — это `block.trailing`, отдельный choke-point. Таких
   мест в emit_c.rs ~20+ (if/match/loop/supervised/with/spawn — каждый со
   своей копипастой). Точечные патчи 5 мест не поймали `with`-тело. Фикс:
   централизация в `emit_expr` (переименован в `emit_expr_inner` + тонкая
   обёртка) — дизарм безопасен на ЛЮБОМ узле `Call{Member}` независимо от
   синтаксической позиции (чистая запись флага, не влияет на значение).
2. **`c_binding` пуст на FAIL/INTERRUPT run-сайтах** — они эмитятся В
   ПРОЛОГЕ `enter_defer_scope` (ДО того как `Stmt::Let` реально дошёл до
   объявления C-переменной); откладывание `c_binding` на
   `emit_auto_cleanup_arm` (заполняется позже) бесполезно для кода,
   сгенерённого РАНЬШЕ. Фикс: вычислять `c_binding` СРАЗУ в прологе (имя +
   `&`-wrap решение уже известны из AST+`init_c_type`).
3. **`use of undeclared identifier 'r'`** (побочный эффект фикса #2) —
   FAIL/INTERRUPT run-сайты ссылаются на C-переменную, которая ЕЩЁ НЕ
   объявлена (та же временная проблема, что #2, но для самого имени, не
   только для его использования в c_binding). Фикс: hoist pre-declaration
   (`T name = NULL/0/{0};` + `hoisted_let_vars`) — зеркалит
   СУЩЕСТВУЮЩИЙ `errdefer_refs`-механизм (Plan 100.8 D166), который решал
   ТУ ЖЕ проблему для ДРУГИХ defer-тел, ссылающихся на биндинг.
4. **Слепой arg-scan дизарм чуть не попал в код** (пойман ДО коммита, при
   собственном код-ревью, не через тест) — изначально скопировал
   `reconsume_scopes`'ный arg-scan-дизарм (для `return f(g)` / `foo(g)`
   bare statement) на `auto_cleanup_active` без гейта на callee's
   param-mode. Откатил ПЕРЕД первым коммитом этой логики, задокументировал
   как известный пробел (см. выше) вместо тихого бага.

## Ф.3 — фикстуры (счётчики + расположение)

- **Pos:** `spec_tests/conformance/d432_auto_cleanup_hybrid_c.nv` — 9
  `test`-блоков (module `spec_tests.conformance`, часть единого
  folder-CU): normal-exit-untouched, return-дизарм, receiver-call-дизарм,
  MaybeConsumed×2 (обе ветки), LIFO (2 типа), цикл×5-итераций, throw-путь.
- **Neg:** `spec_tests/conformance/neg/d432_no_cleanup_type_still_strict_neg.nv`
  (тип БЕЗ `@cleanup` — D133-not-consumed по-прежнему), `spec_tests/
  conformance/neg/d432_effectful_cleanup_not_auto_neg.nv` (`@cleanup` с
  непустым effect-row — D133-not-consumed по-прежнему, ограждение 1).
- **Стандалон-проверка** (до folder-CU интеграции): все 3 pos-пробы +
  2 neg — PASS/FAIL как ожидалось (см. отчёт агента). D432-именованные
  файлы в `spec_tests/conformance/` — финальная версия для гейта.
- **Consume{}-block coexistence:** НЕ добавлена отдельная НОВАЯ фикстура —
  существующие `d188_*`/`d196_*` фикстуры в том же folder-CU УЖЕ проверяют
  блок-форму; их регресс = гейт coexistence (§3a — блок не тронут кодом
  217 вообще, только bare-форма новая).
- **Discarded-temporary neg (ограждение 4):** ПОПЫТКА написать фикстуру
  вскрыла, что D133 СЕГОДНЯ (до и после 217, не регрессия) НЕ ловит
  анонимный discard `p217_make(1);` как отдельную ошибку ни для
  `@cleanup`-типов, ни для обычных consume-типов без `@cleanup` — это
  ПРЕСУЩЕСТВУЮЩИЙ пробел в D133-диагностике вне периметра 217 (217 трогает
  СТРОГО именованные `Stmt::Let`, анонимные значения не задевает вовсе).
  Задокументировано в D432 §3 + маркер `[M-d133-discarded-temporary-diagnostic]`
  — завести в backlog-followups.md при закрытии волны (не блокирует 217).

## Регрессии, найденные и починенные folder-CU-гейтом (2026-07-20/21)

Folder-CU регресс (весь `spec_tests/conformance` одним CU) поймал ДВА
реальных бага, которых standalone d432-фикстуры НЕ ловили (взаимодействие
с СУЩЕСТВУЮЩИМИ фикстурами — именно почему конвенция требует
folder-CU, не только новые тесты):

1. **`guard_cross_scope_transfer.nv` — двойной `unlock`.** `consume g =
   mu.lock(); do_work_under_lock(g, counter)` — `g` передаётся ПРЯМЫМ
   аргументом в free-fn с `consume`-параметром. Это ровно
   «известный пробел keystone» (задокументированный ДО гейта в D432 §4
   как гипотетический) — но гейт показал, что он РЕАЛЬНО ломает
   существующий std-паттерн (MutexGuard, обычный "передать guard в
   хелпер"), не только гипотетика. Фикс: `free_fn_consume_param_
   positions`/`method_consume_param_positions` (новые pre-pass реестры,
   зеркало `Self::fn_param_modes`) + расширение
   `disarm_auto_cleanup_receiver_call` на call-arg-позиции. Пробел
   ЗАКРЫТ (не осталось «известным», см. обновление D432 §4 ниже).
2. **`d188_reconsume_block.nv` — двойной cleanup / паника сторожа
   D201Boom.** Bare auto-cleanup-биндинг (`consume b = d201_boom(7)`),
   ПОЗЖЕ используемый в RE-CONSUME БЛОКЕ (`consume b { ... }`, D188/201
   амендмент) — блок берёт cleanup «на себя» (exactly-once, свой
   tail/return-дизарм), но OUTER auto-cleanup флаг (от bare-let) не знал
   об этом и оставался armed → cleanup срабатывал ДВАЖДЫ (once via блока,
   once via outer auto-cleanup) на КАЖДОМ из четырёх сценариев (tail-вынос
   значения, return-вынос, tail record-литерал с consume-полем, цикл с
   блоком целиком внутри итерации). Фикс: вход в ЛЮБОЙ re-consume блок
   (`Stmt::ConsumeScope` с `re_consume=true`) теперь БЕЗУСЛОВНО дизармит
   OUTER auto-cleanup флаг связанного биндинга (если есть) ДО эмиссии тела
   блока — ownership management передаётся блоку целиком на время его
   исполнения, зеркалит checker's `owned`-проверку + eventual
   `mark_consumed_bypass_guard`.

**Урок:** ЛЮБОЙ существующий механизм, который «сам знает, как потребить
consume-биндинг» (re-consume блок, consume-param transfer, receiver
consume-метод) должен быть explicit disarm-точкой для НОВОГО auto-cleanup
слоя — простого «работает на моих собственных фикстурах» недостаточно,
нужен ПОЛНЫЙ folder-CU regression против ВСЕГО существующего consume-
корпуса (d133/d174/d188/d201/M-178 семейство) прежде чем считать keystone
готовым.

## Гейты (см. финальный отчёт агента за фактическими результатами прогона)

Порядок: standalone d432 pos+neg → folder-CU `spec_tests/conformance`
(регресс существующих d133/d180/d188/d196 фикстур) → std (runtime/net/fs,
sync-гарды используют `consume{}`) → флагман `examples/flagship/aggregator`
`--strict-effects`.

## Что осталось (честно, не моё)

- **217.1** (`@cleanup` восьми ресурсам: File/TcpListener/halves/UdpSocket/
  OnceGuard/Body/ffi) — отдельный подплан, владелец решил (§8а п.4).
- **`[M-217-consume-param-transfer-disarm]`** — consume-param call-arg
  transfer disarm (см. выше).
- **`[M-d133-discarded-temporary-diagnostic]`** — pre-existing gap, не
  217-регрессия, но стоит закрыть отдельно.
- **Watchdog-таймаут (D188 R3) для bare auto-cleanup** — secondary, не
  подключён (`threshold_var="0"`, harmless).
