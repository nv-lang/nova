# PROGRESS — p403-linux-runfail (дефект №403)

Окно: p403-linux-runfail. Модель: **sonnet**. Worktree: `d:/Sources/nv-lang/nova-p403`
(ветка `p403-linux-runfail`). Воспроизведение — WSL2 Ubuntu, клон в `~/nova-p403`
(домашний каталог, НЕ `/mnt/d`), toolchain rustup 1.85.0 (см. `docs/guide/linux-build.md`).

## Шаг 1 — что именно падает (не «ассерт»)

Мега-CU командой из `.github/workflows/nova-gate.yml`:

```
./nova-cli/target/release/nova test --positive --compile-error --timeout 300 --jobs 4 spec_tests/conformance
```

На чистом Linux-клоне (тот же коммит, что и Windows) воспроизвёл близко к отчёту CI, но
**не идентично**: два прогона подряд дали `PASS: 693 FAIL: 3` (не 4), и упавший набор —
`a_q3_println_debug_record` (RUN-FAIL), `m2217_26_generic_static_method_value_arg_addr_mismatch`
(RUN-FAIL), И **новый, не названный в реестре** `standalone/m2211_108_main_fiber_accept`
(NEG-WRONG-STDOUT) — а `m2217_16_detach_ctx_capture_value_ptr_mismatch` в ОБОИХ моих прогонах
был зелёным. Это ровно закрывает исходное «PASS: 685 FAIL: 4» — реестр назвал только 3 из 4,
четвёртый (`m2211_108`) вообще не был идентифицирован. См. «Шаг 2» — детерминизм проверен отдельно
для каждого имени.

Для `m2217_26` (`spec_tests/conformance/standalone/m2217_26_generic_static_method_value_arg_addr_mismatch.nv`) —
диагноз получен через `NOVA_DEBUG_RUN_DUMP=1 NOVA_DIAG_SEGV=1` + `--keep-artifacts` + `gdb`
на сохранённом бинаре:

```
Program received signal SIGSEGV, Segmentation fault.
Vec____nova_byte_method_len (nova_self=0x0) at .../m2217_26....c:9568
#1 Nova_Wrap____nova_int_static_from_req (req=0x7feff7dfdb60)
#2 nova_lambda_2_body (_env_ptr=..., r=140737353632496)   <- garbage, not a Req*
#3 nova_fn_...11via_closure ()
```

**Это не ассерт** — оба `test`-блока печатают `PASS`, третий тест (`via_closure`) падает
внутри самого предиката. Причина — в СГЕНЕРИРОВАННОМ C: замыкание `|r| { ... }`, объявленное
как `fn(Req) -> bool` в `let`-аннотации (`Req` — 8-полевая value-record, за 16-байтным
порогом «by-ref», Plan 172.14), эмитило собственную сигнатуру как

```c
static nova_bool nova_lambda_2_body(void* _env_ptr, nova_int r);   /* WRONG */
```

а место вызова (тот же файл, другая, УЖЕ ПРАВИЛЬНАЯ генерация) звало через каст на

```c
nova_bool(*)(void*, NovaValue_Req)   /* передаёт структуру ПО ЗНАЧЕНИЮ */
```

— caller/callee расходятся в C ABI. Подтверждено МИНИМАЛЬНЫМ, НЕ-generic репро
(без `Wrap[T]`/`FromReq`, просто `ro f fn(Req) -> int = |r| { use_req(r) }`) — генеричность
из исходного описания бага была НЕ причиной, только 8-полевой `Req` за byref-порогом.

## Шаг 2 — один корень или три

**Не три. Фактически НЕСКОЛЬКО независимых дефектов**, один из которых я нашёл и исправил
(общий чекер-канал), и два — отдельная, более глубокая материя Vela-рантайма, оставленная
не тронутой (см. «Не исправлено» ниже) — не масштаб задачи «closure byref value-record».

### Корень A (исправлен) — closure-параметр падает на checker-channel

`compiler-codegen/src/codegen/emit_c.rs::closure_channel_param_tys` вызывает
`resolved_type_to_typeref_named`, который конвертирует `ResolvedType → TypeRef` для
Func-сигнатуры замыкания. Этот конвертер (написан для узкой цели — HOF-биндинг
`fn_newtype`/free-fn сигнатур) умел разворачивать ТОЛЬКО `Named`/`Func`/`Unit`/`Readonly` —
для ЛЮБОГО параметра/возврата типа `Scalar`/`Bool`/`Float`/`Str` (int/bool/f64/str — САМЫЙ
частый случай для замыкания) он проваливался в `_ => None`, и через `?` роняло ВЕСЬ Func
целиком, даже если остальные части были в порядке. Замыкание падало на дефолт
`nova_int`-для-каждого-параметра (легаси bootstrap) — а место вызова (СОВЕРШЕННО ОТДЕЛЬНАЯ,
уже корректная ветка в `Stmt::Let`-обработке, ~L31749) строило C правильно из ТОЙ ЖЕ
let-аннотации. Рассинхрон caller/callee.

Починка (checker-канал, НЕ легаси emit_c рост типовой материи):
`compiler-codegen/src/types/mod.rs` уже содержал ПОЛНЫЙ конвертер
`ResolvedType → TypeRef` (`resolved_to_typeref`, покрывает Scalar/Bool/Float/Str/Named-с-
generics/Array/Tuple/TypedPtr) — но он жил ВНУТРИ `impl<'a> TypeCheckCtx<'a>` (приватный,
недоступен из `codegen/emit_c.rs`) и НЕ разворачивал `Func` (сознательно, не нужно было
для его исходной задачи). Вынес его в `impl ResolvedType` (симметрично `from_type_ref`,
её же обратной функции) как `pub fn resolved_to_typeref` — старое приватное имя внутри
`TypeCheckCtx` теперь однострочный делегат (все ~12 старых call-sites `Self::
resolved_to_typeref(...)` не тронуты). `emit_c.rs`'s `resolved_type_to_typeref_named`
получил единственный новый catch-all: `_ => crate::types::ResolvedType::resolved_to_typeref(rt, span)`.

### Корень A′ (обнаружен И исправлен ПОПУТНО, тот же коммит) — generic-параметр протекает как «конкретный»

Как только А выше заставил Func-конверсию УСПЕШНО проходить чаще (return `bool`/`int`
больше не рушит всё) — открылся СОСЕДНИЙ, ранее замаскированный дефект: замыкание
`Option[T].filter(pred fn(T) -> bool)`, вызванное КАК `a.filter(...)` на конкретном
`Option[int]`, стало компилироваться в CC-FAIL (`Nova_T *` — bogus, никогда не
объявленный тип). Причина: `materialize_literal_coercion`'s `ClosureLight`-плечо
(types/mod.rs, «172.1.2 АТОМ 3a», ~L17227) гейтит регистрацию канала предикатом
`ConcreteNamedNoArgs`, который ЧИСТО СТРУКТУРНО пропускает ЛЮБОЙ голый `Named{args:[]}`
— включая протёкшее generic-scope имя `T`, не прошедшее `mark_type_params` (в отличие от
ДВУХ других call-sites `materialize_literal_coercion`, уже чинивших ЭТОТ ЖЕ класс —
`[196.5 closure-lowering fix]` и `[M-instance-method-closure-arg-generic-return]`, оба
через `typeref_mentions_any` против generic-scope callee). У ЭТОГО, самого раннего и
самого простого плеча, нет доступа к `gs`/callee — чинил ИНАЧЕ: потребовал, чтобы голый
`Named` был РЕАЛЬНО объявленным типом (`self.types.contains_key(name)`), не структурной
похожестью. `char` (единственный «примитив», выглядящий как `Named`) проверяется ПЕРВЫМ
через `TypeSet::Primitive`, гейт `self.types` его не касается.

Найдено ПОПУТНО (той же волной, не отдельным окном) — правило «нулевая толерантность»:
дефект вскрыт МОИМ ЖЕ фиксом, чинится ТОЙ ЖЕ волной, не помечается «TODO» без действия.

### НЕ исправлено — два ОТДЕЛЬНЫХ, более глубоких сбоя Vela-рантайма

`a_q3_println_debug_record` (RUN-FAIL) и `standalone/m2211_108_main_fiber_accept`
(RUN-FAIL/NEG-WRONG-STDOUT) **остаются красными и ПОСЛЕ фикса корня A/A′** — они НЕ
касаются closure/value-record byref материи вообще. Полный core-dump + gdb backtrace
(`ulimit -c unlimited`, прямой запуск бинаря без обёртки раннера):

* `a_q3` (тест «Plan 173 §5.2 watchdog: cleanup/body overrun», М:N-таймаут-сценарий) —
  `SIGSEGV` внутри `_nova_park_mark_slot` (`compiler-codegen/nova_rt/nova_sched.h:335`),
  вызванного из `nova_sched_park`/`nova_sched_park_until`/`_nova_sleep_via_driver`
  (`Duration.sleep()` внутри watchdog-тела).
* `m2211_108_main_fiber_accept` (bare-fiber TCP accept + `detach`-клиент) — `SIGSEGV`
  внутри `uv_run`/`uv.run_pending` (libuv), вызванного из `nova_supervised_drain_main_scope`
  → `nova_runtime_drain_orphans()`, запущенного как **atexit-обработчик** (`__run_exit_handlers`
  → `exit()`). Именно ЭТО совпадает с формулировкой задания «падение ПОСЛЕ прохождения
  проверок — в эпилоге, коде возврата или при выходе» — но для ДРУГОГО теста, не m2217.

Оба падения ловятся ГЛОБАЛЬНО-установленным `_arena_sigsegv_handler`
(`compiler-codegen/nova_rt/fiber_arena.c:207`) — но НЕ как «в диапазоне арены» (адрес
фолта не попадает ни в один fiber-stack, `in_our_range == false`), то есть это НЕ
guard-page/stack-overflow диагностика (обычный `raise(SIG_DFL)`-passthrough,
без диагностического `fprintf`) — обычный use-after-free/dangling-pointer где-то в
М:N-scheduler/orphan-drain путях, специфичный для Linux (WSL2 воспроизводит стабильно
100%, детерминированно, не флейк — трижды подряд).

Это материя `nova_rt/**` concurrency (spawn/detach/scope/driver) — по норме CLAUDE.md
относится к `docs/dev/mn-coding-conventions.md`/`docs/dev/debugging-races.md`, требует
отдельного race-investigation протокола (state-dump, не наскоком). **Вне масштаба
данного окна** (материя — «byref value-record closures», не М:N-scheduler). Рекомендация
владельцу: отдельный маркер/окно для (а) `nova_sched_park`/watchdog-overrun SIGSEGV,
(б) `nova_runtime_drain_orphans`-в-atexit SIGSEGV — возможно ОБЩИЙ корень (обе точки —
park/drain-cleanup путь), но нужен отдельный state-dump, не наспех.

`m2217_16_detach_ctx_capture_value_ptr_mismatch` — **не воспроизвёлся** ни разу (5/5
прогонов, ДО и ПОСЛЕ фикса, детерминированно PASS). Его фикстура тестирует УЖЕ РАНЕЕ
смёрженный фикс (`emit_detach`'s capture-populate loop ↔ `ref_params`, отдельная материя
от closure_channel_param_tys) — я НЕ утверждаю, что мой фикс его касается; честно:
не смог ни подтвердить, ни опровергнуть его роль в исходном `FAIL: 4` CI-прогоне.

## Шаг 3 — фикс (материя: checker-канал)

Файлы:
* `compiler-codegen/src/types/mod.rs` — `resolved_to_typeref` вынесен из
  `impl<'a> TypeCheckCtx<'a>` в `impl ResolvedType` (публичный, симметричен
  `from_type_ref`); старое имя внутри `TypeCheckCtx` — однострочный делегат.
  `materialize_literal_coercion`'s `ClosureLight`-плечо — `ConcreteNamedNoArgs`
  дополнен проверкой `self.types.contains_key(name)` (реальная декларация, не
  структурное совпадение).
* `compiler-codegen/src/codegen/emit_c.rs` — `resolved_type_to_typeref_named`'s
  catch-all: `_ => None` → `_ => crate::types::ResolvedType::resolved_to_typeref(rt, span)`.

Обоснование «checker-канал, не легаси-emit_c-рост»: единственная СТРУКТУРНАЯ правка
`emit_c.rs` — одна строка (замена `None` на делегат к чекер-функции); вся типовая
логика (что есть примитив/Named/Func) живёт в `types/mod.rs`, `emit_c.rs` только её
потребляет — ровно паттерн, требуемый памятью `feedback-compiler-fixes-checker-channel-196`.

## Приёмка

### 1. Матрица по оси платформы (правило 6)

| Фикстура | Linux (WSL2, до фикса) | Linux (после) | Windows (до фикса, main-бинарь) | Windows (после, свежая сборка) |
|---|---|---|---|---|
| `m2217_26_generic_static_method_value_arg_addr_mismatch` | RUN-FAIL (SIGSEGV, gdb-подтверждено) | **PASS** | PASS (уже был зелёным — ABI-везение) | **PASS** |
| `m2217_16_detach_ctx_capture_value_ptr_mismatch` | PASS (не смог воспроизвести падение, 5/5) | PASS | PASS | **PASS** |
| минимальное repro (`\|r\| use_req(r)`, НЕ-generic, `Req` 8 полей) | RUN-FAIL | **PASS** | PASS (тихо давал НЕВЕРНОЕ значение, не крашился — та же C-сигнатура, другой ABI) | **PASS** |
| `plan200_14_option_result_flat_map_filter.nv` (`Option.filter`) | PASS (по счастливой случайности — nova_int==T до фикса A) | **PASS** (уже принципиально, не совпадением) | PASS | **PASS** |
| `a_q3_println_debug_record` | RUN-FAIL | **RUN-FAIL (не исправлено, отдельный корень)** | не проверялось (не относится к byref-материи) | — |
| `standalone/m2211_108_main_fiber_accept` | RUN-FAIL (детерминированно 3/3) | **RUN-FAIL (не исправлено, отдельный корень)** | не проверялось | — |

`nova check std/src`: **PASS: 151 FAIL: 26 WARN: 61** на ОБЕИХ платформах (Windows и
Linux, до и после фикса) — канон 151/26/61 не растёт.

Полный мега-CU на Linux (`--jobs 4`, кэш `.nova-cache` очищен перед каждым сравнением):
**до фикса: PASS: 693 FAIL: 3** (`a_q3`, `m2217_26`, `m2211_108`) →
**после фикса: PASS: 694 FAIL: 2** (`a_q3`, `m2211_108` — оба вне масштаба, см. выше).

### 2. Проба «подсунь заведомо негодное»

`git diff` двух правленых файлов сохранён в патч, применён `git checkout --` (без
`stash`, по правилу), Linux пересобран (~105с), прогнаны 4 цели:

```
RUN-FAIL       repro_closure  #   FAIL: closure param byref repro — <garbage>
RUN-FAIL       m2217_26_generic_static_method_value_arg_addr_mismatch  # ... via_closure не допечатал PASS
PASS           repro_filter        (Option.filter — «повезло» через старый nova_int-дефолт)
PASS           m2217_16_detach_ctx_capture_value_ptr_mismatch  (не связан с фиксом, см. выше)
PASS: 2  FAIL: 2
```

**Вердикт: сломанный фикс → фикстуры красные, откат к патчу → фикстуры зелёные.** Проба
пройдена. Фикс восстановлен через `git apply` того же патча (без `stash`).

### 3. Регресс-фикстура (правило 1)

Новая СИНТАКСИЧЕСКАЯ форма не добавлена — фикс не меняет язык, чинит существующее
несоответствие caller/callee C-сигнатур для УЖЕ существующей формы (`ro f fn(T) -> R =
\|params\| {...}` с T/R примитивом ИЛИ generic-scope именем). Регресс-покрытие — уже
существующие в репозитории `m2217_26_generic_static_method_value_arg_addr_mismatch.nv`
(корень A) и `plan200_14_option_result_flat_map_filter.nv` (корень A′), оба теперь
зелёные ПРИНЦИПИАЛЬНО (не по случайному совпадению с `nova_int`-дефолтом, как раньше).
Новых временных репро-файлов в коммит не добавлял (были в scratch, удалены).

### 4. `nova check std/src`

См. таблицу выше — 151/26/61 на обеих платформах, без роста FAIL.

### 5. Рост `emit_c.rs`

`git diff --numstat compiler-codegen/src/codegen/emit_c.rs` → **+27 / −1** строка,
net **+26**. Из них ТОЛЬКО ОДНА структурная (замена `None` на вызов чекер-функции);
остальное — doc-комментарий, обосновывающий, почему правка именно здесь и почему она
безопасна (не тянет за собой новую типовую логику в легаси-слой).

## Диагноз для владельца — что дальше

1. Корень A/A′ — считаю ЗАКРЫТЫМ, матрица зелёная на обеих платформах, проба пройдена.
2. `a_q3`/`m2211_108` — ОТДЕЛЬНЫЙ, непроверенный до конца класс (Vela М:N scheduler/
   orphan-drain SIGSEGV, Linux-only, детерминированный, НЕ флейк) — нужен отдельный
   маркер/окно с полным state-dump протоколом (`docs/dev/debugging-races.md`), не эта
   волна. Оставляю gdb-backtraces обоих в этом файле как отправную точку.
3. `m2217_16` — не смог подтвердить его роль в исходном `FAIL: 4`; фикстура сама себя
   не воспроизводит как красную ни разу локально.
