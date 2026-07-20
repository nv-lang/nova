<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 196 — Producer B (instance-method node_substs), чекпойнт

**Worktree:** `nova-196prodb`, ветка `p196-producer-b`. **База:** main `2d9a15acc`.
**Модель:** sonnet. **Зона:** `compiler-codegen/src/types/mod.rs` (чекер-продюсер) — `emit_c`
только read-only (кроме, возможно, финального read-канала за shadow-гейтом).

---

## 0. Статус: диагностика ЗАВЕРШЕНА (по коду+трейсу), гэп локализован. Реализация — в процессе.

## 1. Находка №1: Producer B УЖЕ ШИРОКО СУЩЕСТВУЕТ (не с нуля)

В отличие от того, что подразумевала карта задания («node_substs не пишется для
instance-method»), по коду выяснилось: `resolve_return_channel` (`types/mod.rs` ~10346,
constraint-solver-based, возвращает `Option<(ResolvedType, Vec<(String,ResolvedType)>)>` —
ИМЕННО node_substs-шейп) **уже существует и уже пишет `self.node_substs`** в ТРЁХ местах:

- `~16098` producer=B-fluent-generic (`-> @` echo + method-own generics).
- `~16232` producer=B-method-residual (return residual, args-driven через solver).
- `~16322` producer=B-carrier (pure-carrier path, receiver-generic residual).

Все три — внутри `resolve_instance_method_return_arity` (~15861), вызываемой ИСКЛЮЧИТЕЛЬНО
из `infer_method_call_channel_type` (~16486), единственной точки входа Producer B.
Codegen-потребитель `rt_slots_from_call` (`emit_c.rs` ~2951, channel-first twin
`rt_slots_from_args`, читает `node_substs[call_id]`) уже развёрнут на 6 сайтах mono-
диспатча (свободные/static/array-ext/user-record instance-method). `shadow_check_node_substs`
уже вызывается (debug_assertions) на нескольких из этих сайтов. `resolve_method_level_subst`
(`emit_c.rs` ~21235, потребитель ~90% instance-method-с-method-generics) уже
channel-first: читает `node_substs[call_id]` ДО легаси Steps 1/2/2f/3.

**Вывод:** «инференс из аргументов» (closure-return-peek, non-closure arg unify) для
instance-method-generic УЖЕ покрыт продюсером с прошлой волны (196.5 Stage-A/B2/B4/B5).
Гэп — уже, чем карта предполагала.

## 2. Находка №2 (ФАКТИЧЕСКИЙ гэп, эмпирически подтверждён трейсом)

`infer_method_call_channel_type` (~16491) требует ТОЧНОГО совпадения `func.kind ==
ExprKind::Member{obj,name}`:
```rust
let ExprKind::Member { obj, name } = &func.kind else { return None; };
```
Для **явного turbofish на INSTANCE-method-вызове** — `obj.method[U](args)` (валидный Nova-
синтаксис, парсер подтверждает: `parser/mod.rs` ~8230 «`[T](args)` remains available to ANY
base — `req.body.parse[T]()` etc.») — AST-форма ЭТОГО call — `func.kind ==
ExprKind::TurboFish{ base: Member{obj,name}, type_args }`, НЕ `Member`. Деструктуризация
падает → `infer_method_call_channel_type` возвращает `None` СРАЗУ, `resolve_return_channel`
даже не вызывается — explicit `type_args` НИКОГДА не доходят до Producer B.

Тот же гейт дублируется в вызывающем коде (`f1_block`, ~8008: `else if let
ExprKind::Member{obj:mo,...} = &func.kind` — тоже требует literal Member, не TurboFish) —
Channel-2 (`resolved_types_buf`, ВОЗВРАТ вызова) для этого шейпа ТОЖЕ не пишется, отдельно
от node_substs.

### Эмпирическое доказательство (репро + трейс)

Репро (`scratchpad/cmin/repro_turbofish_method.nv`, `Box[T] @map[U](f fn(T)->U) -> Box[U]`,
вызов `w.map[str](|x| "five")`):
```
[NODE_SUBSTS] consumer=resolve_method_level_subst call_id=ExprId(16) ctx=Box____nova_int.map fallback reason=miss
[NODE_SUBSTS] consumer=resolve_method_level_subst call_id=ExprId(16) ctx=Box____nova_int.map hit-composed n=1
[B5] fallback call=ExprId(16) names=["T", "U"] channel=None
```
`channel=None` — `node_substs[16]` НЕ существует (Producer B никогда не вызывался для этого
call_id). `resolve_method_level_subst` компенсирует через ЛЕГАСИ `explicit_tf`
(`self.current_method_turbofish`, codegen-side structural extraction of the turbofish AST,
`M-91.1-method-turbofish-dispatch`, ~21340) — компиляция УСПЕШНА (byte-correct), но идёт в
обход канала — ровно тот класс `rt_slots_from_args`-подобного MISS-fallback, который карта
задания называет «ЖИВ для instance-method класса».

## 3. Что доставляю (план реализации)

Цель: чтобы `obj.method[U](args)` (explicit turbofish, INSTANCE receiver) писал
`node_substs[call.id]` ТЕМ ЖЕ протоколом (`resolve_return_channel`), а не только
инферился из args/closures.

Механизм (mirror следующих прецедентов в этом же файле):
1. `f1_check_call`'s free-fn/static-ctor overlay explicit turbofish поверх unify-subst
   (~11691-11697, «D310 — explicit annotation is ground truth»).
2. `resolve_method_level_subst`'s legacy `explicit_tf` positional seed (emit_c ~21347-21357)
   — ПОЗИЦИОННО поверх `fn_decl.generics` (method-own, НЕ carrier — turbofish в Nova на
   INSTANCE-методе задаёт только METHOD-level generics, carrier уже известен из ресивера).

Шаги:
- Расширить точку входа: в месте, где строится `func`/`base` для инстанс-метод-колла
  (аналог `f1_check_call`'s `explicit_type_args` extraction ~11227), добавить разбор
  `ExprKind::TurboFish{base: Member{obj,name}, type_args}` РЯДОМ с обычным
  `ExprKind::Member{obj,name}` — как в `infer_method_call_channel_type` (~16491), так и в
  вызывающем коде `f1_block` (~7982-8008, Channel-2 write) и (при необходимости)
  `check_instance_overload`.
- Прокинуть `explicit_type_args: Option<&[TypeRef]>` через
  `resolve_instance_method_return_arity` → `resolve_return_channel` — позиционно
  unify'ить `method_names_ordered[i]` с `explicit_type_args[i]` НА СВЕЖИХ solver-vars
  (`solver.unify(Ty::Var(vars[name]), Ty::from_resolved(ResolvedType::from_type_ref(ta)))`)
  ДО args-derived `extra_eqs` (ground truth overlay, mirrors D310).
- SHADOW: `shadow_check_node_substs` уже существует и уже вызывается на потребительских
  сайтах — новый продюсер автоматически покрывается тем же гейтом (byte-identity
  channel↔legacy per-key). Явно перепроверить на репро + корпусе.
- emit_c: НЕ трогать (задание разрешает read-only изучение `rt_slots_from_args`/consumer'ов
  — сделано в §1/§2 выше). Потребитель уже channel-first (`rt_slots_from_call`,
  `resolve_method_level_subst`) — как только продюсер пишет `node_substs[call_id]`, канал
  автоматически становится HIT без единой правки emit_c.

## 4. Гейт-фикстуры (byte-parity, план проверки)

`d119_method_level_type_params`, `d122_bound_method_mono_dispatch`,
`d122_generic_bound_forwarding`, `d30_try_op_unwrap_pair`, `d408_option_chain_sized_width`
— НИ ОДНА из них НЕ использует explicit turbofish на instance-method (проверено чтением:
d119/d122 — только inferred method-generics). Ожидаемый диф = 0 (продюсер untouched-путей
byte-parity-safe: новый код активируется ТОЛЬКО на `TurboFish{base:Member}` AST-форме,
которой в этих фикстурах нет вообще — чисто additive). Byte-parity доказывается反 ревёрт-
циклом (сохранить правку → checkout baseline → собрать → .c → восстановить → пересобрать →
.c → diff) — то же, что CH делал для static-ctor.

## 5. Реализация ЗАВЕРШЕНА (types/mod.rs, коммиты 142f81b1b + 4343b48c3)

- `resolve_return_channel` (~10346): новый параметр `explicit_method_type_args`, overlay
  ПЕРЕД `extra_eqs` — `solver.unify(Var(v), leaf)` с `rt_respells_names`-poison-гейтом.
  Три существующих вызова (fluent-generic/method-residual/carrier) прокинуты.
- `resolve_instance_method_return_arity`: новый параметр `explicit_type_args: Option<&[TypeRef]>`
  (0-arity wrapper передаёт `None`).
- `infer_method_call_channel_type` (единственная точка входа Producer B): гейт входа расширен
  с `Member{obj,name}` до `Member{..} | TurboFish{base: Member{..}, type_args}` — вторая форма
  = explicit turbofish на instance-ресивере (`obj.method[U](args)`). Closure-arg fallback
  пропускается, когда turbofish уже дан.
- **ВТОРОЙ баг, найден РЕАЛЬНЫМ корпусом** (не синтетикой): `spec_tests/conformance/
  standalone/m176_method_return_turbofish.nv` (`fn Reg @empty[T]() -> Vec[T]`, ВЫЗОВ БЕЗ
  параметров — `T` только в позиции возврата) вскрыл, что solver-overlay в
  `resolve_return_channel` НЕДОСТАТОЧЕН: method-residual ветка строит `full_subst`/`out_full`
  ТОЛЬКО из args/closure unify ДО вызова solver'а — при 0 параметрах `out_full` остаётся
  unresolved и функция бейлится (`return None`) РАНЬШЕ, чем solver вообще вызывается. Фикс:
  `full_subst` дополнительно сеется `explicit_type_args` (positional по `f.generics`,
  `entry().or_insert_with`) ДО args-loop — `unify_type` никогда не перезаписывает already-
  bound имя (подтверждено чтением `const_fn_trampoline.rs:1073` — «already bound — must
  match»), так что сид безопасен.

## 6. Реальные корпусные сайты этого класса (не только синтетика)

- `spec_tests/conformance/standalone/m176_method_return_turbofish.nv`: `r.empty[str]()`,
  `r.into[str]()` (consume-метод) — ОБА теперь `producer=B-method-residual` (было
  `channel=None`/`[B5] fallback`). `nova test spec_tests/conformance` (release, single CU):
  **PASS**.
- `spec_tests/conformance/any_is/box_is_downcast.nv` (`n.try_as[int]()` и т.п.) — ЛОЖНОЕ
  срабатывание грепа: `try_as` — компиляторный ИНТРИНЗИК на `any` (спец-кейс в `emit_c.rs`
  ~33318/~53003, ВООБЩЕ не проходит через `types/mod.rs`/`method_overloads` — нет ни одного
  упоминания `try_as` в `types/mod.rs`). Продюсер B на него не распространяется и не должен
  (не instance-method вызов в обычном смысле, отдельный builtin-путь) — нулевое
  взаимодействие, нулевой риск.
- `spec_tests/conformance/m196_facetc_instance_collision_and_method_generic_default.nv`
  (комментарий ~20-36): `pb.wrap[str]("hi")` (explicit turbofish + default-arg backfill)
  ДОКУМЕНТИРОВАН как известный ICE в ДРУГОМ, независимом механизме —
  `callnorm.rs::try_normalize_call` (default-arg Block-rewrite) сознательно ОСТАВЛЯЕТ
  `TurboFish{base:Member}`-вызовы НЕТРОНУТЫМИ (var_types-ordering конфликт с frozen
  `infer_call_ret_c`/`resolve_instance_call_subst`, Plan 91.1/172.1 зона) — этот файл САМ НЕ
  вызывает эту форму (`pb.wrap("hi")` без turbofish, U инферится из аргумента). Producer B
  этот файл не трогает и не может задеть этот ICE (не работает с `callnorm.rs`); файл входит
  в mega-CU conformance-прогон — если он проходит, значит регрессии нет.

## 7. Гейты — статус

- Byte-parity standalone (4/5 гейт-фикстур, `nova-codegen compile` напрямую — d119 не
  компилируется standalone, нужен prelude, см. GEN notes): `d122_bound_method_mono_dispatch`,
  `d122_generic_bound_forwarding`, `d30_try_op_unwrap_pair`, `d408_option_chain_sized_width` —
  **diff=0** (ревёрт-цикл: baseline `2d9a15acc` mod.rs vs патч). Плюс синтетический репро
  (`Box[T] @map[U]`, explicit `w.map[str](...)`) — **diff=0** (легаси explicit_tf fallback и
  новый канал сходятся байт-в-байт, как и должно быть).
- SHADOW: standalone-корпус `std/src/{collections,time,encoding}` (104 файла) — 14 OK / 90
  FAIL, 32 паники — ВСЕ `[P67-LEGACY]` (`.append`/`.swap`/`.keys` return-type-unknown / `Ident
  X not in var_types`), идентичный класс/строки (52787/52930/53787), что задокументировала
  GEN-волна как ПРЕДСУЩЕСТВУЮЩИЙ standalone-tool артефакт (нехватка multi-file/prelude
  контекста у raw `nova-codegen compile`, не про Result/Option/generics). Grep по логу на
  `shadow`/`SHADOW` — 0 совпадений (0 расхождений shadow_check_node_substs). d119 конкретно
  падал на `.to_str()` (нужен prelude) — тоже pre-existing.
- Авторитетный гейт `nova test spec_tests/conformance --jobs 4` (release, single CU,
  `nova-cli` собран из ЭТОЙ ветки) — **PASS: 124  FAIL: 0  SKIP: 14**. Единым CU покрыты
  d119/d122×2/d30/d408 (все 5 гейт-фикстур задания), `standalone/m176_method_return_
  turbofish` (реальные turbofish-сайты), `m196_facetc_instance_collision_and_method_
  generic_default` (документированный ICE в ДРУГОМ механизме, файл САМ турбофиш не
  вызывает — см. §6 — прошёл чисто). Главный гейт (CLAUDE.md) — ЗЕЛЁНЫЙ.
- Флагман: `nova check --strict-effects examples/flagship/aggregator/src/main.nv` →
  **PASS: 1  FAIL: 0  WARN: 33** (все warning — unused-import/postfix-mut, косметика, не
  про эту правку). `nova build --strict-effects --mode release ... -o aggregator.exe` →
  **built (34.97s)**, 0 ошибок.
- Мега-CU (весь корпус разом, `nova test` без folder-фильтра) — НЕ гонялся, по заданию
  («Мега-CU НЕ гонять»).

## 8. Trace — N реальных corpus-сайтов (repo-wide sweep)

Repo-wide грep по AST-форме `value.method[Type](` (исключая declaration-строки `fn ...` и
comment-only строки), весь репозиторий (`std/`, `examples/`, `spec_tests/`, `nova_tests/`,
`detect172/`): **РОВНО 2 реальных call-сайта** — оба в
`spec_tests/conformance/standalone/m176_method_return_turbofish.nv`:
`r.empty[str]()` (ExprId 7) и `r.into[str]()` (ExprId 38, consume-метод). Единственный
другой похожий шейп в корпусе — `x.try_as[T]()` (10 сайтов, 3 файла) — компиляторный
ИНТРИНЗИК на `any`, НЕ проходит через `types/mod.rs`/`method_overloads` вообще (см. §6),
Producer B на него не распространяется (структурно другой путь, не instance-method call в
обычном смысле).

`NOVA_NODE_SUBSTS_TRACE=1` ДО правки (baseline `2d9a15acc`): оба ExprId(7)/ExprId(38) —
`channel=None`/`[B5] fallback reason=miss`, компенсация легаси `explicit_tf`. ПОСЛЕ правки:
`producer=B-method-residual call_id=ExprId(7) method=empty n=1`,
`producer=B-method-residual call_id=ExprId(38) method=into n=1` — оба HIT,
`resolve_method_level_subst`/`rt_slots_from_call` теперь читают канал, легаси fallback не
достигается на этих сайтах. **N=2** (было 0/2).

Малое N — честный факт (не пропуск покрытия): explicit turbofish на instance-methods —
редкий синтаксис в текущем корпусе (инференс из аргументов почти всегда достаточен,
поэтому Producer B с прошлой волны уже покрывал подавляющее большинство instance-method
generic-сайтов БЕЗ турбофиша). Продюсер доставлен ADDITIVELY/proactively — тот же паттерн,
что у D310 (free-fn/static-ctor turbofish overlay), который тоже не имел широкого корпуса
на момент своей правки.

## 9. Что разблокировано для GEN-сноса (следующая волна)

- **`rt_slots_from_call`'s per-arg structural fallback** (`emit_c.rs` ~2970-2978, внутри
  `rt_slots_from_call`) — теперь МЁРТВ для explicit-turbofish-instance-method сайтов
  (канал их покрывает). Полный снос fallback-тела ВСЁ ЕЩЁ гейтится покрытием ОСТАЛЬНЫХ
  instance-method классов (inferred-only — но те УЖЕ покрыты продюсером с прошлой волны
  196.5 Stage-A/B2/B4/B5, см. §1) — снос теперь технически безопасен для explicit-turbofish
  класса, но `rt_slots_from_call`/`resolve_method_level_subst` обслуживают ОБА класса одной
  функцией (channel-first с per-key completeness-гейтом), так что явный «detach» этого
  подкласса потребовал бы различителя на call-site (не отделим без правки frozen-диапазона —
  тот же паттерн, что GEN нашёл для `resolve_result_option_ret`/B06a/B10j). **Рекомендация
  GEN-волне:** снос легаси explicit_tf-ветки (`emit_c.rs` `M-91.1-method-turbofish-dispatch`,
  ~21347-21357, ПОСЛЕ канал-first блока в `resolve_method_level_subst`) — теперь безопасен
  как panic-detach ЗА debug-only trace-накоплением (мой SHADOW/trace уже показал 0
  расхождений на всём достижимом материале); `current_method_turbofish` state-поле,
  вероятно, тоже можно снести следом, если explicit_tf была его единственной ролью
  (не проверял — вне периметра этой волны, только чекер).
- **B11r/B11q (method-generic Result/Option в frozen `infer_call_ret_c`)** — карта задания
  относила их к «тому же классу» (instance-method без node_substs); ПО ФАКТУ они —
  METHOD-класс с INFERRED (не turbofish) generics на Result/Option-объектах
  (`.is_ok()`/`.map()`/`.unwrap_or()` и т.п.) — уже КАНАЛИЗИРОВАНЫ прошлой волной (Producer
  B существовал ДО этой волны для inferred-путей, см. §1 диагностики). Byte-parity
  фикстуры `d30_try_op_unwrap_pair`/`d408_option_chain_sized_width` (обе используют ИМЕННО
  B11r/B11q классы) прошли PASS в mega-CU — подтверждает, что канал уже отвечает раньше
  каскада для этих сайтов и B11r/B11q скорее всего уже МЁРТВЫ там же, где static-ctor
  `rt_slots_from_args` был мёртв у CH. Рекомендация: GEN-волне стоит ICR-трейснуть B11r/B11q
  ОТДЕЛЬНО (`NOVA_TRACE_ICR=1`) на этих двух фикстурах + std-корпусе, чтобы подтвердить
  0 достижимости ДО panic-detach — не проверено этой волной (вне периметра: «чекер, не
  emit_c»).

## 10. Коммиты (ветка `p196-producer-b`, база main `2d9a15acc`)

1. `b4961e650` docs(196): чекпойнт — диагностика гэпа (repro+трейс, explicit turbofish
   на instance-method — реальный оставшийся гэп).
2. `142f81b1b` feat(types,196-prodb): `[M-196-producer-b-turbofish]` node_substs для
   explicit turbofish на instance-method — `resolve_return_channel`/
   `resolve_instance_method_return_arity`/`infer_method_call_channel_type` расширены.
3. `4343b48c3` fix(types,196-prodb): seed method-residual `full_subst` от explicit
   turbofish (return-only-generic класс, найден РЕАЛЬНЫМ корпусом — m176).
4. `127a5c1c0` docs(196): чекпойнт-обновление (реализация завершена).

**В main НЕ мёржено. Push запрещён по заданию (worktree СВОЙ, не пушить).**

Модель: sonnet.
