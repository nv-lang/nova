# WriteBuffer.cancel CC-FAIL — диагноз (СТОП, ядро 196)

Задача: фикс pre-existing CC-FAIL `std/src/concurrency/supervised_deadline_test.nv`
(«WriteBuffer.cancel» класс). Worktree `nova-p173`, ветка `fix-writebuffer-cancel`
(база `eede02fb8`).

## Статус: диагноз завершён, ФИКС НЕ ДЕЛАЛСЯ (первопричина — ядро Plan 196)

Владелец заранее указал стоп-условие: если первопричина в легаси-зоне
`infer_call_ret_c` (ядро 196) — стоп после диагноза, не лезть в зону.
Диагноз подтвердил именно это.

## 1. Репро — точный текст C-ошибки

`nova test std/src/concurrency` (release, env-override на main vcpkg + libuv-копия):

```
CC-FAIL   std/src/concurrency/supervised_deadline_test
  supervised_deadline_test.c:10500:25: error: no member named 'cancel' in 'struct Nova_WriteBuffer'
  supervised_deadline_test.c:10606:25: error: no member named 'cancel' in 'struct Nova_WriteBuffer'
  2 errors generated.
```

δ=0 подтверждено (байт-в-байт то же на main до слияний 173-хвостов) — pre-existing.

Второй CC-FAIL в той же папке — `retry_test` (generic-mono `nova_str`↔`Nova_T*`) —
**вне объёма**, не трогал (ядро Plan 196, отдельный класс).

## 2. Первопричина

Источник в `.nv`: `std/src/concurrency/cancellation.nv` / `supervised_deadline_test.nv`:
```nova
ro tok = CancelToken.new()
...
tok.cancel()
```
`CancelToken` — компиляторный builtin (НЕТ настоящего `TypeDecl` в `.nv`; только имя
в hardcoded builtins HashSet, types/mod.rs:20636, чтобы не падало
«undefined identifier»). Codegen (`emit_c.rs:34197`) корректно эмитит ЗНАЧЕНИЕ
`nova_cancel_token_new()` — но выведенный **C-тип переменной** `tok` — НЕВЕРНЫЙ:

```c
/* SRC: ro tok = CancelToken.new() */
Nova_WriteBuffer* tok = nova_cancel_token_new();   // должно быть NovaCancelToken*
```

Дальше эта неверная `Nova_WriteBuffer*` type protекает в spawn-capture ctx struct
(`NovaSpawnCtx_nova_spawn_4.tok` типизируется как `Nova_WriteBuffer*` — capture
type берётся из `self.var_types["tok"]`, emit_c.rs:10955-10958), и на
`_c->tok->cancel()` codegen не распознаёт receiver как `NovaCancelToken*`
(диспатч special-case на `obj_ty == "NovaCancelToken*"`, emit_c.rs:33078) →
падает в generic member-call → `struct Nova_WriteBuffer` не имеет поля `cancel`.

### Почему тип переменной неверный — двухслойный баг

1. **Type-checker (types/mod.rs) не резолвит `CancelToken.new()` в Channel-1/2.**
   `CancelToken` не в `self.types` (нет `TypeDecl`) → весь блок
   `infer_expr_type`'s `ExprKind::Member{obj:Ident(tyname), name:ctor}` static-ctor
   arm (types/mod.rs:13741, гейт `self.types.get(tyname)...`) пропускается.
   Значит `resolved_types`/`resolved_callees` для этого call ExprId **не
   заполняются** — legacy codegen fallback остаётся единственным источником типа.

2. **Legacy `infer_call_ret_c` (emit_c.rs:48489, core-196 зона) угадывает НЕ ТО.**
   Изолированный минимальный репро (`ro tok = CancelToken.new(); tok.is_cancelled()`,
   без spawn) в ДРУГОМ файловом контексте даёт **другой** неверный тип —
   `Nova_StringBuilder*` вместо `Nova_WriteBuffer*`. Т.е. это не «CancelToken
   специально путается с WriteBuffer» — это общий permissive fallback внутри
   `infer_call_ret_c`, который для receiver-идентификатора без материализованного
   типа (`recv_c_type_materialized(obj) == None`) и без записи в `method_overloads`
   под ключом `("CancelToken", "new")` в итоге выбирает **произвольный** другой
   `.new()`-кандидат (зависит от того, что уже зарегистрировано/видимо в этом
   compile-unit — `ICR-HIT B06_method_overloads_candidates` /
   `B06b_method_overload_single_candidate` зажигаются в трейсе, `NOVA_TRACE_ICR=1`
   debug-сборка). WriteBuffer/StringBuilder ctors как раз ЛЕГАЛЬНО zero-arg
   (`WriteBuffer.new(cap int = INITIAL_CAPACITY)`) — то есть arity совпадает,
   и что-то в этой ветке путает receiver-контекст.

Комментарии в самом `infer_call_ret_c` (emit_c.rs:49908-49960, Plan 196.2 W1
[gate-1]) подтверждают: ветки `B11l_{stringbuilder,writebuffer,readbuffer}_static`
и `B11l_external_registry_static_ident` (в т.ч. `AtomicInt.new()`/`Mutex.new()`/
`WaitGroup.new()`) уже **сняты** как «structurally unreachable» — предполагается,
что checker материализует их возврат через Channel-2. `CancelToken.new()` —
ТАКОЙ ЖЕ builtin-конструктор (concurrency namespace, как `AtomicInt`/`Mutex`/
`WaitGroup`), но для него аналогичная Channel-2-материализация **не была
добавлена** — гэп в checker, не в legacy-функции как таковой. Legacy-функция лишь
проявляет симптом (угадывает не то), потому что Channel-1/2 её не опередили.

## 3. Рекомендация (для владельца / следующего агента, вне текущего объёма)

Вероятно правильное место фикса — **types/mod.rs**, по аналогии с уже
материализованными `AtomicInt.new()`/`Mutex.new()`/`WaitGroup.new()`
(см. комментарий emit_c.rs:49925-49932): добавить `CancelToken.new()` (и,
возможно, другие CancelToken-специфичные конструкции) в checker-side
Channel-2 материализацию возврата, чтобы `resolved_types`/`resolved_callees`
для этого call ExprId содержали `R::Named{"CancelToken"}` → `resolved_named_to_c`
уже умеет `"CancelToken" => "NovaCancelToken*"` (emit_c.rs:3923) — тогда legacy
fallback для этого случая станет структурно недостижим (тот же паттерн, что уже
применён к WriteBuffer/StringBuilder/Mutex/AtomicInt/WaitGroup).

Это ЯДРО Plan 196 (либо checker Channel-2 producer добавление, либо
дальнейшая работа с `infer_call_ret_c`) — согласно указанию владельца,
дальше не лез, коммитов с фиксом НЕ делал.

## 4. Состояние worktree на момент стопа

- Ветка `fix-writebuffer-cancel` @ `eede02fb8` (без новых коммитов — только
  чтение/диагноз, никаких изменений в .nv/.rs).
  Временный репро-файл `nova_tests/scratch/repro_ct.nv` создавался для
  диагностики и удалён (не коммитился).
- Release + debug сборки `nova-cli/target/{release,debug}/nova.exe` собраны
  локально в worktree (env-override NOVA_GC_LIB_DIR/INCLUDE_DIR на main vcpkg,
  libuv скопирован ранее — см. docs/promts/read-toolchain.md +
  memory project-worktree-nova-test-setup).
- Гейты (conformance full, std/src/collections) **НЕ гонялись** — задача
  остановлена на диагнозе до шага 4 (объём фикса не наступил).
