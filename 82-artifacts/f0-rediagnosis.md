# Plan 82 Ф.0 — Re-diagnosis (decision point)

> **Статус:** 🟡 В РАБОТЕ. Подтверждено: §1.2, §1.3, §1.6, модель §5.2,
> primitives памяти, toolchain-SEH. **Остаётся:** minicoro-интегрированные
> эксперименты — test (a) decision-point A/B, test (d) аномалия Попыток
> 3-4, test (e) SEH через границу fiber-стека.
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

## 5. Остаётся (minicoro-интегрированные эксперименты)

Probe-харнесс §2 самодостаточен (kernel32). Следующие эксперименты
требуют интеграции с `minicoro.h` (`MCO_USE_ASM`) и составляют
**decision-point** Ф.0 — их нельзя торопить, ошибка дизайна харнесса
даёт неверное архитектурное решение:

- **test (a) — DECISION POINT A vs B.** Корутина minicoro на
  `VirtualAlloc(MEM_RESERVE)` с движущейся `PAGE_GUARD`-вершиной; рост
  стека через guard → **растит ли ядро non-primary minicoro-стек**
  (после TIB-свопа minicoro). A = OS-native grow (VEH не нужен для
  happy-path); B = VEH lazy-commit fallback. — ⏳ НЕ СДЕЛАНО.
- **test (d) — аномалия Попыток 3-4** (§1.4): воспроизвести «`main_yield`
  TIMEOUT 60+ сек» либо доказать, что это был баг откаченного arena-кода.
  Если не воспроизвести и не объяснить — сигнал второго неизвестного
  бага, Ф.1 не начинать. — ⏳ НЕ СДЕЛАНО.
- **test (e) — SEH `__try/__except` через границу fiber-стека.** — ⏳ НЕ
  СДЕЛАНО.

## 6. Промежуточный go/no-go

**Go на продолжение Ф.0.** Все verifiable-посылки плана подтверждены без
сюрпризов: TIB решён (§1.1, плановая верификация), §1.3 реальный блокер
(Probe 1), §5.2-модель commit-безопасна (Probe 4 + VirtualQuery-clamp),
§1.6 API доступны (заголовки + линковка). Toolchain-SEH находка
**усиливает** план (подтверждает VEH-выбор §5.1).

Финальный go/no-go на **Ф.1** — после test (a)/(d)/(e): решение A/B и
объяснение аномалии Попыток 3-4 обязательны до старта Ф.1.
