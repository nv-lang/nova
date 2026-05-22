# Plan 82 Ф.0 — Re-diagnosis (decision point)

> **Статус:** 🟡 В РАБОТЕ. Подтверждено: §1.2, §1.3, §1.6, модель §5.2,
> primitives памяти, toolchain-SEH. **DECISION POINT test (a) РЕШЁН
> (2026-05-22): ПУТЬ A + обязательный патч `ctx.stack_limit`** (см. §5).
> **Остаётся:** test (d) аномалия Попыток 3-4, test (e) SEH через
> границу fiber-стека.
> **Создан:** 2026-05-22.

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

## 6. Остаётся в Ф.0

- **test (d) — аномалия Попыток 3-4** (§1.4): «`main_yield` TIMEOUT».
  Найденный `__chkstk`-краш — это CRASH, не TIMEOUT → вероятно отдельная
  аномалия. Воспроизвести/объяснить. — ⏳ НЕ СДЕЛАНО.
- **test (e) — SEH через границу fiber-стека.** Частичные данные уже
  есть: `__try/__except` ВНУТРИ корутины (в пределах её стека) работает
  (харнесс `f0_test_a.c` на нём построен); `__try` вокруг `mco_resume`
  (через границу switch) фолт на коро-стеке НЕ ловит. Полная проверка
  (SEH-unwind через границу, `RtlVirtualUnwind` по `StackBase/Limit`) —
  ⏳ НЕ СДЕЛАНА.

## 7. Промежуточный go/no-go

**GO на Ф.1, путь A с обязательным патчем `ctx.stack_limit`.**
Decision-point (test a) решён экспериментально и однозначно. Все
verifiable-посылки плана подтверждены: TIB решён (§1.1), §1.3 реальный
блокер (Probe 1), §5.2-модель commit-безопасна (Probe 4 +
VirtualQuery-clamp), §1.6 API доступны. Toolchain-SEH находка усиливает
VEH-выбор для overflow-handler'а.

Остаточный риск понижен: путь A подтверждён → нет нужды в VEH-lazy-
commit happy-path (путь B). test (d)/(e) — верификации меньшего масштаба;
рекомендуется закрыть до/в начале Ф.1.
