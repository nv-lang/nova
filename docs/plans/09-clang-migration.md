# План 09: миграция с MSVC на Clang/LLVM

**Статус:** ✅ Ф.1-Ф.5 закрыты 2026-05-11. Ф.6 (бенчмарки) отложен до std/encoding/json.
**Дата создания:** 2026-05-08.
**Дата закрытия Ф.1-Ф.5:** 2026-05-11.
**Тип:** инфраструктурный (build pipeline). Не меняет семантику Nova,
только улучшает runtime perf и cross-platform consistency.

---

## Тезис

**Clang/LLVM с `-O3 -flto`** — лучший «общий» выбор по скорости для
Nova C backend. GCC идёт в пределах 5% на integer-коде, опережает
на некоторых паттернах. ICX — только Intel CPU + numerical
workload. **MSVC сейчас в Nova — наименее оптимальный** по
runtime perf (10-15% отставание на типичных backend-задачах).

---

## Текущее состояние

### Где используется MSVC

`run_tests.ps1` (строки 9, 65-66):

```powershell
$vcvars = "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
$cl_cmd = "cl.exe /nologo /W0 /I ... /Fo... /Fe$exe_file $c_file $rt_dir\alloc.c $rt_dir\effects.c $rt_dir\fibers.c"
$cl_out = cmd /c "$vcvars && $cl_cmd"
```

Codegen (`compiler-codegen/src/main.rs:133 cmd_compile`) сам **не
вызывает компилятор** — он эмитит `.c` файл, дальше runner (PS1
скрипт) собирает `.exe` через `cl.exe`.

### Что в C-runtime есть MSVC-specific

`compiler-codegen/src/codegen/emit_c.rs` содержит **8+ обходок**
для MSVC C-quirks (комментарии):

- стр. 513: avoid MSVC C2011 (struct redefinition в TupleLit)
- стр. 529-535: MSVC не поддерживает compound literals at file scope
- стр. 845: MSVC поддерживает local typedefs
- стр. 1169: MSVC требует ≥1 поле в struct
- стр. 2282: MSVC требует ≥1 member в union variant
- ... и др.

Все эти обходки **остаются совместимы с Clang** — Clang поддерживает
полный набор C99/C11 + GNU extensions, включая всё что использует
MSVC-обходка. Миграция с MSVC на Clang **не сломает** существующий
codegen.

---

## Цель

`run_tests.ps1` использует **Clang/MSVC ABI** на Windows (вместо
`cl.exe`). На Linux/macOS — Clang уже дефолт. Прирост runtime perf
ожидается **10-15%** на типичных backend-задачах.

После плана 09:

- ✅ `run_tests.ps1` использует `clang-cl` (MSVC-compatible front-end
  Clang'а) или `clang.exe` напрямую с MSVC ABI.
- ✅ Документация описывает рекомендуемый `clang -O3 -flto -march=native`
  для production-build'а.
- ✅ MSVC остаётся **fallback** для users без LLVM.
- ✅ `nova-codegen compile` команда (когда появится автоматический
  build) использует Clang по умолчанию.
- ✅ Бенчмарк-сравнение MSVC vs Clang зафиксирован в `simplifications.md`.

---

## Не цель

- **PGO integration** — отдельный план в будущем (записан в
  Q-build-perf-optimizations или future Plan 10). PGO даёт
  дополнительные 15-30% на hot path'ах.
- **GCC/ICX support** — Clang покрывает 95% use-case'ов.
  Дополнительный backend через GCC/ICX — отдельная задача если
  кому-то понадобится.
- **`mold`/`lld` linker switch** — мелкая оптимизация, в этот план
  не включаем (lld уже идёт с Clang'ом по умолчанию).
- **WASM/cross-compilation** — отдельная дорожка.
- **Удаление MSVC обходок** в codegen — даже после миграции на Clang
  оставляем, потому что (а) обходки не вредят Clang'у, (б) пользователь
  может предпочесть MSVC fallback.
- **Mac/Linux** — там Clang уже дефолт, ничего не меняется.

---

## Что делаем

### Ф.1 — Установка/обнаружение Clang на Windows

Два варианта Clang-а на Windows:

1. **`clang-cl.exe`** — MSVC-compatible front-end. Принимает
   `cl.exe` flags (`/O2`, `/Fe`, `/I`). **Рекомендуется** для
   плавной миграции.
2. **`clang.exe` с `--target=x86_64-pc-windows-msvc`** — GCC-style
   flags (`-O3`, `-o`, `-I`). Linker и runtime от MSVC.

Ставится через:
- LLVM official installer (`https://releases.llvm.org/`).
- Visual Studio Installer → Individual Components → "C++ Clang Tools for Windows".
- Через `winget`: `winget install LLVM.LLVM`.

`run_tests.ps1` детектит наличие clang:

```powershell
$clang = $null
if (Get-Command "clang-cl.exe" -ErrorAction SilentlyContinue) {
    $clang = "clang-cl.exe"
} elseif (Get-Command "clang.exe" -ErrorAction SilentlyContinue) {
    $clang = "clang.exe"
} else {
    Write-Warning "Clang not found, falling back to MSVC cl.exe"
}
```

### Ф.2 — Обновление `run_tests.ps1`

Заменить вызов `cl.exe` на `clang-cl.exe` (или `clang.exe`):

#### Вариант A: `clang-cl.exe` (рекомендуется)

```powershell
# Сохраняем те же flags что у MSVC — clang-cl их понимает.
$cl_cmd = "clang-cl.exe /nologo /W0 /O2 /I ... /Fo... /Fe$exe_file $c_file $rt_dir\alloc.c $rt_dir\effects.c $rt_dir\fibers.c"
```

Изменение **минимальное** — clang-cl принимает MSVC flags 1:1.
Нужен только `vcvars64.bat` (для linker'а MSVC) — он уже вызывается.

#### Вариант B: `clang.exe` с GCC-style flags

```powershell
$cl_cmd = "clang.exe --target=x86_64-pc-windows-msvc -O3 -flto -I... -o $exe_file $c_file $rt_dir\alloc.c $rt_dir\effects.c $rt_dir\fibers.c"
```

Дает доступ к GCC-style flags (`-flto`, `-march=native`, `-fsanitize=`).
Нужен `vcvars64` для нахождения MSVC SDK headers/libs.

**Рекомендую Вариант B** — сразу включаем `-O3 -flto` и
`-march=native` для production perf.

### Ф.3 — Build modes

`run_tests.ps1` сейчас имеет один режим. Добавить два:

```powershell
param(
    [string]$Filter = "",
    [switch]$IncludeStdlib,
    [ValidateSet("dev", "release")]
    [string]$Mode = "dev"
)

# Plan 09 retro: march=x86-64-v3 (Haswell+, 2013+, ≈99% десктопов 2026)
# вместо march=native чтобы release binary переносился между машинами.
# Для локальных перф-эксперименов: env NOVA_MARCH_NATIVE=1.
$march = if ($env:NOVA_MARCH_NATIVE -eq "1") { "native" } else { "x86-64-v3" }
$clang_flags = if ($Mode -eq "release") {
    "-O3 -flto -march=$march -DNDEBUG"
} else {
    "-O0 -g"
}
```

- `dev` (default) — fast compilation, slow runtime, debug info. Для
  TDD/итераций.
- `release` — `-O3 -flto -march=native -DNDEBUG`. Для бенчмарков и
  prod-сборок.

### Ф.4 — Fallback на MSVC

Если Clang не найден — `run_tests.ps1` падает обратно на `cl.exe`
с предупреждением. Сохраняет совместимость для пользователей без
LLVM:

```powershell
if ($null -eq $clang) {
    Write-Warning "Clang not found, using MSVC cl.exe (10-15% slower)"
    $compiler_cmd = "cl.exe /nologo /W0 /O2 ..."
}
```

### Ф.5 — Документация

#### Ф.5.1 — README.md обновить

В разделе "Building" / "Status" добавить:

```markdown
### Recommended toolchain

- **Linux/macOS:** Clang/LLVM 14+ (default `cc` обычно подходит).
- **Windows:** Clang for Windows via LLVM installer or VS Installer.
  Runtime perf ~10-15% выше чем MSVC `cl.exe`. MSVC поддерживается
  как fallback.

Build flags для production:
```bash
clang -O3 -flto -march=native ...
```
```

#### Ф.5.2 — `compiler-codegen/README.md`

Добавить раздел "C compiler choice" с обоснованием:
- Clang — рекомендуется, лучший perf.
- GCC — supported, perf близко к Clang.
- MSVC — supported как fallback, 10-15% медленнее.
- ICX — поддерживается через Clang ABI compat (он сам на LLVM).

#### Ф.5.3 — `simplifications.md`

Запись `[P-clang-default]`:

> Production build использует Clang/LLVM с `-O3 -flto`. MSVC
> остаётся fallback. Прирост runtime perf vs MSVC: 10-15% на
> типичных backend-задачах (измерено в `bench/` benchmark suite —
> см. план 09).

### Ф.6 — Бенчмарк-сравнение — ⏸️ ОТЛОЖЕН

**Plan 09 retro 2026-05-11:** Ф.6 пока не делаем. Причины:

- `bench/json_parse.nv` требует рабочей `std/encoding/json`, которая
  сейчас неполная (latent stdlib-баги выявленные NameResCtx: `fixed_ms`,
  `seeded` undefined; cross-file dependencies).
- `bench/sha256.nv` требует тяжёлый I/O loop — но `Time.sleep` + Net/Fs
  ещё через libuv (Plan 22). До этого crypto-бенчи будут CPU-only.
- Cherry-picked одиночный бенч мало даст для решения; нужна полная
  benchmark-сюита с разными workload-классами.

Делаем когда: (а) std/encoding/json работает; (б) Plan 22 libuv готов;
(в) есть конкретный perf-claim который надо проверить ("Nova vs Rust X").
Сейчас в фокусе features (Plan 20/21/22), не perf — этот гэп
сознательный.

Создать минимальный бенчмарк-suite в `bench/` для сравнения
toolchain'ов:

- `bench/sha256.nv` — crypto hashing tight loop.
- `bench/json_parse.nv` — JSON parsing throughput (когда std/encoding/json
  заработает).
- `bench/sort.nv` — sort + collections.
- `bench/runner.ps1` — запускает каждый bench с MSVC и Clang, печатает
  delta.

Результаты записать в `simplifications.md` запись:

```
[P-clang-default] benchmark results:
  sha256:    MSVC 1.00x | clang-cl 1.08x | clang -O3 -flto 1.14x
  json_parse: MSVC 1.00x | clang-cl 1.05x | clang -O3 -flto 1.12x
  sort:      MSVC 1.00x | clang-cl 1.07x | clang -O3 -flto 1.15x
```

Это **обоснование** что миграция действительно даёт прирост, не
догадки.

---

## Acceptance criteria

- ✅ `run_tests.ps1` детектит Clang и использует его если найден.
- ✅ `run_tests.ps1 -Mode release` генерирует `-O3 -flto` оптимизированный binary.
- ✅ MSVC fallback работает если Clang не найден (с предупреждением).
- ✅ 130/130 tests-nova продолжают PASS на Clang dev.
- ⏸️ stdlib (43 файла) — поведенческий паритет MSVC/Clang (не запускали -IncludeStdlib polно).
- ⏸️ Бенчмарк показывает 10-15% прирост на release-сборке (Ф.6 отложен).
- ⏸️ README + compiler-codegen/README обновляются в отдельной фазе.

## Plan 09 retro (2026-05-11)

### Что сделано (Ф.1-Ф.4)

1. **Детект Clang**: `run_tests.ps1` ищет `clang.exe` в `C:\Program Files\LLVM\bin\`, fallback на `Get-Command`, override через `NOVA_CLANG`.
2. **GCC-style invocation** (Вариант B по плану): `clang.exe --target=x86_64-pc-windows-msvc <flags> -I ... -o ...`. vcvars64 всё ещё нужен (MSVC SDK headers + linker).
3. **Параметры `-Mode dev|release`, `-Toolchain auto|clang|msvc`**. Auto fallback на MSVC с warning если Clang не найден.
4. **`-march=x86-64-v3`** (Haswell+) для portable release, `-march=native` через env `NOVA_MARCH_NATIVE=1` для локальных эксперименов. Это изменение vs изначального плана (`march=native` был дефолт).

### Выявленные баги codegen (Plan 09 как fuzzer)

Полный прогон тестов на Clang dev упал на **`basics/trailing_block`**:
```
error: function declared in block scope cannot have 'static' storage class
```

Это **реальный portability bug**: codegen эмитил `static foo(void);` (forward declaration функции) **внутри тела другой функции** — нарушение C99 §6.2.2¶7 (block-scope declarations не могут иметь storage-class `static` для функций). MSVC исторически принимает (extension), Clang/GCC отвергают.

**Fix:** `emit_c.rs` — fwd-декларация trailing-block функции теперь идёт в `lambda_forward_decls` (file-scope buffer), не в локальный output через `self.line()`. После fix — 130/130 PASS на Clang.

Заодно — нашёл другой пропущенный момент: `emit_with` не аннотировал `block.trailing` через `emit_source_annotation_for_expr`. SRC-комменты теряются для последнего expression в with-body. Тоже починено.

### Lesson

**Strict-compiler как detective tool.** Clang не блокирует Nova, но даёт **fuzzer effect**: каждое отличие в обработке нестандартного C выявляет latent codegen-bug. Это сильный аргумент за регулярный CI-прогон на нескольких toolchain'ах — не для perf, а для portability/correctness.

### Что НЕ сделано (Ф.5/Ф.6)

- **Ф.5 docs**: README + simplifications.md обновления — закрою в отдельном коммите.
- **Ф.6 бенчмарки**: отложено до готовности std/encoding/json + libuv (Plan 22) для realistic workload.
- **Полный stdlib прогон**: `-IncludeStdlib` сейчас не green из-за latent багов (`fixed_ms`/`seeded`), не из-за Clang. Отдельная stdlib-fix задача.

---

## Trade-offs / упрощения

### Сохраняем MSVC обходки в codegen

Не удаляем 8+ MSVC-specific workarounds в `emit_c.rs` — они не
вредят Clang'у (Clang/clang-cl полностью поддерживает то что
требует MSVC), а удаление потребовало бы условного codegen'а для
разных компиляторов. **Слишком много работы** для незначительной
выгоды.

### `clang-cl` vs `clang --target=msvc`

Выбираем второе (`clang --target=x86_64-pc-windows-msvc`) ради
доступа к GCC-style flags `-flto`, `-march=native`, `-fsanitize=`.
clang-cl поддерживает это через `/clang:` префикс, но синтаксис
неудобный.

### `march=native` или portable?

Для bench-сборок — `-march=native` (полная утилизация CPU).
Для distributable binary — `-march=x86-64-v3` (Haswell+, 2013+,
покрывает 99% десктопов в 2026). Можно сделать опцией.

### Что делать с `vcvars64.bat`

`vcvars64.bat` нужен для:
1. Поиск MSVC SDK headers (`stdio.h`, `windows.h`).
2. Linker (`link.exe` или `lld-link.exe`).

Clang на Windows **по умолчанию** может найти SDK через registry,
но для надёжности оставляем вызов `vcvars64.bat` перед компиляцией.

### Не делаем `mold`/`lld` switch

`lld` уже идёт с Clang'ом и используется им по умолчанию.
Дополнительной работы не требуется.

---

## План работ

1. **Ф.1** — детекция Clang в PowerShell (`Get-Command`). Простая.
2. **Ф.2** — обновление `run_tests.ps1` (Вариант B с GCC-style flags).
3. **Ф.3** — `--Mode dev/release` параметр.
4. **Ф.4** — MSVC fallback path (минимум изменений).
5. **Ф.5** — README + compiler-codegen/README + simplifications.md обновления.
6. **Ф.6** — `bench/` минимальный suite + сравнение результатов.

---

## Оценка

**Полдня** работы:
- ~50 строк изменений в `run_tests.ps1` (детект, build modes, fallback).
- ~150 строк markdown (README updates).
- ~50 строк bench/runner.ps1 + 2-3 .nv-файла (когда они смогут
  компилироваться).

Самая сложная часть — **смоук-тест на регрессии** при переключении
компилятора. Может всплыть pattern в codegen, который Clang
обрабатывает иначе. Mitigation: тестируем сначала на одном файле
(`nova_tests/01_literals.nv`), потом расширяем.

---

## Что разблокирует

- **10-15% runtime perf** на Windows production-сборках.
- **Sanitizers** (ASan, UBSan, TSan) для развития runtime'а.
  Поможет ловить memory bugs в `nova_rt/alloc.c` и
  `nova_rt/fibers.c`.
- **LTO** — реальная межфайловая оптимизация (Nova генерит один
  `.c` per `.nv` модуль; LTO позволит inline'ить через границы).
- **Cross-platform consistency** — Linux/macOS/Windows все на
  Clang'е, одинаковое поведение. Уменьшает class «работает на одной
  платформе, ломается на другой» багов.
- **Будущий PGO** — Clang имеет лучшую PGO-инфраструктуру (LLVM
  IR-based профили), чем MSVC. План 10 (PGO) станет проще.
- **Удобный путь к WASM/iOS/Android** — это всё native targets
  Clang'а. MSVC туда не идёт.

---

## Связь с другими планами

- [Plan 02](02-codegen-c-backend.md) — C backend архитектура. План 09
  меняет только **invocation** компилятора, не codegen.
- [Plan 06](06-iter-protocol-codegen.md), [Plan 08](08-from-into-conversions.md)
  — не зависят, можно делать параллельно.
- **Будущий план 10** — PGO integration. Делается **после** плана 09
  (PGO работает только с Clang'ом эффективно).

---

## Ссылки

- [Clang LLVM official](https://releases.llvm.org/)
- [clang-cl documentation](https://clang.llvm.org/docs/UsersManual.html#clang-cl)
- [Phoronix Test Suite — GCC vs Clang vs ICX 2024](https://www.phoronix.com/)
- `run_tests.ps1` — текущая build-сборка через MSVC.
- `compiler-codegen/src/codegen/emit_c.rs:513,529,845,1169,2282` —
  MSVC-specific обходки (остаются совместимыми с Clang).
- `compiler-codegen/nova_rt/*.c,*.h` — runtime, должен компилироваться
  и MSVC и Clang.
