# Plan 82 Ф.0 — Re-diagnosis (decision point)

> **Статус:** ✅ **ЗАВЕРШЕНА (2026-05-22). GO на Ф.1.** Подтверждено:
> §1.2, §1.3, §1.6, модель §5.2, primitives памяти, toolchain-SEH.
> **DECISION POINT test (a) РЕШЁН: ПУТЬ A + обязательный патч
> `ctx.stack_limit`** (см. §5). **test (d)** — аномалия Попыток 3-4 НЕ
> воспроизведена на standalone-реплике дизайна 44.3; объяснена как баг
> интеграционного arena-кода, не фундаментальный блокер (см. §6).
> **test (e)** — SEH на fiber-стеках корректен и детерминирован на обеих
> toolchain (см. §7). Финальный go/no-go — §8.
> **Создан:** 2026-05-22. **Завершён:** 2026-05-22.
> Артефакты: `f0_probe.c`, `f0_gc_link.c`, `f0_test_a.c`, `f0_test_d.c`,
> `f0_test_e.c`, `build.ps1`.

Изолированный standalone C-харнесс (вне рантайма Nova) — в отличие от
4 интеграционных провалов Plan 44.3. Артефакты: `f0_probe.c`,
`f0_gc_link.c`.

---

## 1. git-археология (§1.2) — ✅ подтверждено

`compiler-codegen/nova_rt/minicoro.h` — один коммит, файл не менялся с
2026-05-05; попытки 44.3 (2026-05-13/14) шли на 8 дней позже. Гипотеза
44.3 «старый minicoro без Windows-asm» ложна → диагноз 44.3 «MCO_USE_ASM
не обновляет TIB» — definitive misdiagnosis. (Зафиксировано в плане §1.2;
здесь — повторное подтверждение.)

## 2. Standalone probe-харнесс `f0_probe.c` — ✅ 4/4 PASS на обеих toolchain

Самодостаточный (только kernel32). Probe'ы Windows-поведения памяти:

| Probe | Что | Результат |
|---|---|---|
| 1 (§1.3) | чтение `MEM_RESERVE`-но-не-`COMMIT` страницы | → `0xC0000005` ACCESS_VIOLATION |
| 2 | чтение committed `PAGE_NOACCESS` hard-guard | → `0xC0000005` ACCESS_VIOLATION |
| 3 | первое касание `PAGE_GUARD` страницы | → `0x80000001` GUARD_PAGE_VIOLATION, one-shot (guard снят, 2-е касание проходит) |
| 4 (§5.2) | by-byte скан **закоммиченного** окна reserved-региона | → fault-free |

**Следствия:**
- **§1.3 подтверждён.** Плоский `GC_add_roots` поверх lazy-commit-арены
  непереносим на Windows: byte-wise conservative scan Boehm'а упал бы AV
  на первой незакоммиченной странице (Probe 1).
- **Модель §5.2 подтверждена.** Precise push **только закоммиченного**
  диапазона `[committed_low, top]` — fault-free (Probe 4). Это обходит
  §1.3-AV by construction.
- **PAGE_GUARD-primitive исправен** (Probe 3) — механизм, на котором
  стоит lazy-commit путь A (§5.1), работает: фолт one-shot, ядро снимает
  guard.
- **`PAGE_NOACCESS` hard-guard** даёт детерминированный AV (Probe 2).

## 3. Toolchain-SEH — ⚠️ важная находка

`f0_probe.c` использует `__try/__except` для наблюдения фолтов. Сборка:

| Toolchain | hardware-SEH в `__try/__except` | Итог |
|---|---|---|
| **MSVC `cl.exe`** | ловит по умолчанию (C-режим) | ✅ 4/4 PASS |
| **clang-cl + `/EHa`** | ловит с `/EHa` | ✅ 4/4 PASS |
| clang-cl без `/EHa` | **НЕ ловит** → AV пролетает, краш | ❌ |
| `clang.exe` (GNU-драйвер) | **НЕ ловит** hardware-AV в `__try` | ❌ |

**Следствие для реализации (Ф.1):** overflow-handler production-кода
**обязан** использовать **VEH** (`AddVectoredExceptionHandler`) —
compiler-независимый и ловит hardware-исключения вне зависимости от
драйвера/флагов. Это совпадает с выбором плана §5.1 (путь B = VEH;
путь A — VEH только диагностический). `__try/__except` для guard/AV в
рантайме — ненадёжно на clang-cl без `/EHa`. Если же `__try` всё же
используется — clang-cl-сборка Nova обязана нести `/EHa`.

## 4. §1.6 — Boehm GC API — ✅ полностью подтверждено

**(i) Заголовки** (`vcpkg_installed/x64-windows-static/include/gc/`):
- `GC_call_with_alloc_lock` — `gc.h:1472` ✅
- `GC_get_stack_base` — `gc.h:1663` ✅
- `GC_set_stackbottom` — `gc.h:1686` ✅
- `GC_push_all` — `gc_mark.h:300` ✅
- `GC_set_push_other_roots` — `gc_mark.h:311` ✅

**(ii) Линковка** — `f0_gc_link.c` собран clang-cl'ом и слинкован против
статической `gc.lib` (`x64-windows-static`): все 5 символов резолвятся,
exe запускается. §5.2-дизайн (registry + precise push) стоит на реально
доступных API.

**(iii) VirtualQuery-clamp** (П5) — перечитан vendored
`vcpkg/blds/bdwgc/.../v8.2.8/win32_threads.c`:
- `GC_get_stack_min` (`:1552`) — `VirtualQuery` вниз от known-mapped
  адреса, цикл `while (Protect & PAGE_READWRITE && !(Protect &
  PAGE_GUARD))` → останавливается на первой не-RW / guard странице.
- `GC_push_stack_for` (`:1656`) использует `GC_get_stack_min` для
  вычисления `stack_min`.
- **Подтверждено:** per-thread скан worker'а на Windows **клампится** к
  закоммиченному RW-региону, где лежит `CONTEXT.Rsp` → fault-free даже
  если Rsp оказался на fiber-стеке, даже mid-switch. Mid-switch GC-race
  (П5) на Windows закрыт по построению — ровно как утверждает §1.6.

## 5. test (a) — DECISION POINT A vs B — ✅ РЕШЁН (2026-05-22)

minicoro-интегрированный харнесс `f0_test_a.c`: корутина minicoro
(`MCO_USE_ASM`) на `VirtualAlloc(MEM_RESERVE)`-стеке с lazy-commit-
раскладкой `[reserved][committed MARGIN 64K][PAGE_GUARD][committed
WINDOW 64K]`; рекурсия растит стек вниз через guard. VEH-счётчик:
guard-fault, обработанный ядром IN-KERNEL, в user-mode не доставляется
(путь A → VEH молчит); иначе VEH ловит (путь B).

**Матрица результатов (MSVC `cl.exe` + clang-cl, оба):**

| Сценарий | StackLimit | результат |
|---|---|---|
| a0 baseline (полностью committed) | — | OK |
| sub-page кадр (2K, без `__chkstk`) | minicoro default | **ПУТЬ A** |
| sub-page кадр (2K) | patched | **ПУТЬ A** |
| `__chkstk`-кадр (8K, >1 страницы) | minicoro default | **CRASH** на MSVC; путь A на clang-cl |
| `__chkstk`-кадр (8K) | patched | **ПУТЬ A** на обеих |

**Вердикт: ПУТЬ A** — ядро Windows растит non-primary minicoro-стек
штатно (OS-native grow; VEH для happy-path не нужен). Подтверждено:
рекурсия растит committed-регион на +120–128K, VEH ни разу не сработал.

**КЛЮЧЕВАЯ НАХОДКА — патч `ctx.stack_limit` ОБЯЗАТЕЛЕН.** minicoro
`_mco_makectx` (minicoro.h:765) ставит `ctx->stack_limit = stack_base`
(низ региона — claim'ит весь стек закоммиченным). У настоящего
`CreateFiber`-стека `StackLimit` = фактический committed-low. С
minicoro-дефолтом функции с кадром >1 страницы (компилятор эмитит
`__chkstk`-пробинг) **крашат процесс на MSVC** (`__chkstk` пробингует
стек far-below-RSP, ядро не распознаёт рост). Патч одной строкой —
`((_mco_context*)co->context)->ctx.stack_limit = committed_low` после
`mco_create` — делает minicoro-стек неотличимым от `CreateFiber`-стека;
после патча `__chkstk`-код растёт штатно на **обеих** toolchain.

clang-cl `__chkstk` без патча выживает — но полагаться нельзя
(toolchain-зависимо); патч обязателен для обеих.

→ Это ровно плановый §5.1 «путь A применим с патчем ctx.stack_limit
(одна строка в `nova_fiber_alloc`)» — подтверждено экспериментом, и
**уточнено**: патч не «опционально», а **обязателен** (план говорил
«если Ф.0 подтверждает A — custom VEH для happy-path не нужен»; VEH
действительно не нужен — но патч StackLimit нужен).

Артефакт: `f0_test_a.c` — самооркеструемый (default-прогон спавнит
`__chkstk`-под-прогоны отдельными процессами, child-crash не убивает
родителя). Стабильно: MSVC 8+ прогонов, clang-cl 3+.

## 6. test (d) — аномалия Попыток 3-4 — ✅ НЕ воспроизведена, объяснена

44.3 Попытки 3-4 (2026-05-13) сообщали «даже простой `main_yield`
TIMEOUT 60+ сек». Документированный root cause Попытки 4 («minicoro
`MCO_USE_ASM` не обновляет TIB») **опровергнут** (§1.1-1.2). Реальная
причина TIMEOUT оставалась неизвестной — §1.4 / §9-риск-2 плана требуют:
Ф.0 обязана воспроизвести аномалию ЛИБО доказать, что это был баг
откаченного arena-кода.

Харнесс `f0_test_d.c` — изолированная **standalone-реплика дизайна
Попытки 4** (вне рантайма Nova): arena 256 KB слотов на
`VirtualAlloc(MEM_RESERVE)`, guard 16 KB у низа слота, commit usable-
региона при первом alloc слота (`committed_bits`), bitmap free-list +
reuse, **без decommit вообще**. minicoro `MCO_USE_ASM` на этих слотах.
Watchdog-поток (20 сек) детектит hang; счётчик раундов ловит
soft-livelock планировщика.

| Под-тест | Что | Результат (MSVC + clang-cl) |
|---|---|---|
| T1 main_yield-аналог | round-robin над 1 и 3 корутинами (каждая yield'ит) — ядро `main_yield.nv` | ✅ завершается за 2-3 раунда, без hang |
| T2 slot reuse | 20000 циклов `create→run→destroy` на одном слоте | ✅ нет утечки: `slots_active` возвращается к 0, `high_water` не растёт |
| T3 concurrency+churn | 10 пачек × 32 корутины × 3 yield round-robin | ✅ все пачки завершаются, нет утечки слотов |
| T4 §5.1 layout-defect | корутина рекурсит до упора — стек растёт вниз В minicoro-header | exit `0xC0000005` — **чистый AV, НЕ hang** |

**Вердикт test (d): аномалия Попыток 3-4 НЕ воспроизводится на
standalone-харнессе.** minicoro-корутины на arena 256 KB слотов с
**точным** дизайном Попытки 4 (commit-on-first / bitmap reuse /
no-decommit) round-robin'ятся без hang'а и без утечки слотов на обеих
toolchain. Soft-livelock планировщика не наблюдается.

**Объяснение TIMEOUT 44.3 (требование §1.4):** это был баг
**интеграционного arena-кода**, не фундаментальное свойство
{minicoro-asm + VirtualAlloc-арена + round-robin}. Доводы:

1. **Standalone-реплика дизайна работает.** Если бы TIMEOUT был присущ
   связке «minicoro на VirtualAlloc-арене 256 KB», T1/T3 повисли бы.
   Они не виснут.
2. **44.3 сам документировал конкретный баг.** Попытка 3 наблюдала
   `fiber_arena exhausted (4096 slots used)` и выдвинула гипотезу
   slot-leak (некорректный учёт `slots_active`). T2 показывает: при
   **корректной** bitmap-реализации утечки нет — значит slot-leak 44.3
   был дефектом их кода, чинимым, а не фундаментальным.
3. **Документированный root cause Попытки 4 ложен** (§1.1-1.2): TIB
   свопается всегда. Объяснение Попытки 4 неверно целиком.
4. **44.3 гонял полный рантайм Nova** (Boehm `GC_THREADS` + libuv
   event loop); standalone-харнесс их исключает по методологии плана
   (§6 Ф.0 — «изолированный standalone C-харнесс»). Hang мог идти от
   GC-интеграции — плоский `GC_add_roots` поверх reserved-арены (§1.3)
   роняет conservative-скан AV'ом; §5.2 этот подход **выбрасывает**
   полностью (registry + precise push). Точный баг в удалённом
   `fiber_arena_win.c` 44.3 не локализуем — файл откачен и удалён, — но
   и не нужен: Ф.1 пишет **новую** корректную реализацию, Ф.2 заменяет
   ущербную GC-стратегию.

`__chkstk`-краш (находка test a) — это CRASH, не TIMEOUT → отдельное
явление, к Попыткам 3-4 (которые коммитили usable-регион целиком, где
minicoro-дефолтный `stack_limit` уже корректен) **не относится**.
Подтверждено: T4 с полностью закоммиченным слотом не крашит на `__chkstk`.

**§5.1 layout-defect — подтверждён как реальный, чинится в Ф.1.** T4
показывает: при overflow стек растёт вниз сквозь minicoro-header
(`[mco_coro][_mco_context][storage]`) к guard'у. В T4 рекурсия
безусловная → проскакивает header насквозь в guard → чистый AV. Но
overflow, остановившийся ВНУТРИ header'а (затёр `_mco_context.back_ctx`
/ `co->state`), дал бы повреждённый switch и потенциально livelock
планировщика — это **кандидат-механизм** того класса, к которому мог
относиться TIMEOUT 44.3, и ровно та причина, по которой §5.1 требует
ставить движущуюся `PAGE_GUARD`-вершину **над** header-блоком. Ф.1
обязана реализовать §5.1-раскладку.

## 7. test (e) — SEH через границу fiber-стека — ✅ корректно, детерминированно

Харнесс `f0_test_e.c` (корутинный стек — полностью закоммиченный
`VirtualAlloc`; SEH-поведение от lazy-commit не зависит). Собран обеими
toolchain (clang-cl — с `/EHa`, §3).

| Под-тест | Что | Результат (MSVC + clang-cl) |
|---|---|---|
| E1 SEH within fiber | hardware-AV внутри корутины, `__try/__except` на коро-стеке | ✅ поймано, `code=0xC0000005` |
| E2 TIB-своп | fiber читает `gs:[0x08/0x10]` — свои `StackBase/StackLimit` | ✅ свопнуты, попадают в `[stack_base, stack_base+size)` корутины, отличны от main-thread TIB |
| E4 RtlVirtualUnwind-walk | ручной `RtlLookupFunctionEntry`+`RtlVirtualUnwind`-обход коро-стека изнутри fiber'а | ✅ 5 кадров, `Rsp` ни разу не вышел за `[StackLimit,StackBase)`, walk штатно дошёл до poison-RA `_mco_makectx`'а |
| E3 cross-boundary (child) | fiber делает `RaiseException`; `__try/__except` — на CALLER-стеке вокруг `mco_resume` | exit `0xE0001234` — **чистый краш кодом исключения; caller НЕ поймал; нет hang** |

**Вердикт test (e):**

- **SEH внутри fiber'а работает** (E1): handler на коро-стеке ловит
  hardware-фолт на коро-стеке. unwind не уходит за границы — TIB-своп
  даёт корректные `StackBase/StackLimit` (E2).
- **`RtlVirtualUnwind` корректно обходит коро-стек** (E4): walk остаётся
  внутри `[StackLimit, StackBase)` и терминируется по poison return-
  address'у, который `_mco_makectx` кладёт на вершину коро-стека
  (minicoro.h:757). Это ровно §7-пункт тест-матрицы «`RtlVirtualUnwind`
  останавливается по `StackBase/StackLimit`».
- **SEH не переходит границу fiber→caller** (E3): exception, не пойманное
  на коро-стеке, **не** перехватывается `__try` вокруг `mco_resume` —
  потому что frame-chain коро-стека не ведёт на native-стек, а поиск
  handler'а bounded по TIB (= коро-стек). Исход — **чистый
  детерминированный краш** кодом исключения, без hang'а и без AV в самом
  unwinder'е.

Это **паритет с native Windows fibers**: SEH — per-stack механизм, через
`SwitchToFiber`-границу тоже не проходит. Для Nova это не дефект:
ошибки Nova-уровня (`fail`/эффект-хендлеры) разворачиваются через
`setjmp/longjmp` **в пределах** коро-стека (`NovaFailFrame`-цепочка,
`effects.h`), а не через C++ SEH между стеками. Ключевое для Plan 82
требование выполнено: поведение **детерминированно** — нет hang'а, нет
фолта внутри unwinder'а, нет ухода walk'а в незамапленную память.

## 8. Финальный go/no-go — ✅ GO на Ф.1

**GO на Ф.1, путь A с обязательным патчем `ctx.stack_limit`.**

Все пять Ф.0-проверок закрыты:

- **(a)** decision-point решён экспериментально и однозначно: **ПУТЬ A**
  (OS-native grow), обязателен патч `ctx.stack_limit = committed_low`
  после `mco_create` (§5).
- **(b)** overflow в `PAGE_NOACCESS` hard-guard → детерминированный AV
  (`f0_probe.c` Probe 2; `f0_test_d.c` T4).
- **(c)** GC-конфликт: плоский `GC_add_roots` поверх reserved-арены
  падает AV'ом (Probe 1); precise push **только закоммиченного**
  `[committed_low, top]` его обходит by construction (Probe 4) — модель
  §5.2 commit-безопасна.
- **(d)** аномалия Попыток 3-4 НЕ воспроизведена; объяснена как баг
  интеграционного arena-кода 44.3, не фундаментальный блокер (§6).
- **(e)** SEH на fiber-стеках корректен и детерминирован на обеих
  toolchain (§7).

Все verifiable-посылки плана подтверждены: TIB решён (§1.1-1.2), §1.3 —
реальный блокер (Probe 1), §5.2-модель commit-безопасна (Probe 4 +
VirtualQuery-clamp §1.6), §1.6 Boehm-API доступны и линкуются (§4).
Toolchain-SEH находка (§3) усиливает VEH-выбор для overflow-handler'а.

**Остаточные риски (план §9) — все понижены либо сняты:**

- Риск №1 (путь lazy-commit) — **снят**: путь A подтверждён, VEH-lazy-
  commit happy-path (путь B) не нужен.
- Риск №2 (аномалия Попыток 3-4) — **снят**: §9 требовал «Ф.1 не
  начинать до объяснения»; объяснение дано (§6) — интеграционный баг,
  не фундаментальный.
- Риск №3 (VirtualQuery-clamp) — **подтверждён** на vendored source
  (§4-iii); E2/E4 дополнительно показывают, что TIB-своп даёт корректные
  bounds для SEH-машинерии.
- Риск №4 (новый фундаментальный блокер) — **не обнаружен**: ни test
  (a), ни (d), ни (e) не выявили неустранимого поведения Windows.

**Обязательные требования к Ф.1, вытекающие из Ф.0:**

1. Патч `ctx.stack_limit = committed_low` после `mco_create` —
   обязателен (test a), без него `__chkstk`-код крашит на MSVC.
2. §5.1-раскладка: движущаяся `PAGE_GUARD`-вершина **над** minicoro-
   header'ом — обязательна (test d / T4): overflow не должен корёжить
   `_mco_context`/`mco_coro`-header.
3. Overflow-handler — через **VEH** (`AddVectoredExceptionHandler`), не
   `__try/__except` (§3): compiler-независимая ловля hardware-SEH.
4. GC-интеграция (Ф.2) — registry + precise push (§5.2), **0**
   плоских `GC_add_roots` поверх арены (§1.3).
