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

## 5. Дальше (после чекпойнта)

Имплементация в `types/mod.rs`, затем: SHADOW-корпус, byte-parity 5 фикстур + репро,
NOVA_NODE_SUBSTS_TRACE подсчёт новых сайтов (репро + поиск реальных explicit-turbofish-
instance-method сайтов в std/examples — возможно ИХ ВООБЩЕ НЕТ в текущем корпусе, что само
по себе результат: продюсер добавлен proactively/additively, trace N может быть малым на
существующем корпусе, но небезопасным пропуском не является — как и D310 turbofish-overlay
у free-fn, который тоже не имел широкого корпуса на момент правки).
