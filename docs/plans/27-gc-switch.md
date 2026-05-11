// SPDX-License-Identifier: MIT OR Apache-2.0
# План 27: GC switch — Boehm как default allocator

> **Статус:** план, не начат.
> **Создан:** 2026-05-11.
> **Приоритет:** ВЫСОКИЙ — текущий default `alloc.c` (plain malloc)
> делает Nova **непригодным** для long-running workloads.
> **Зависит от:** —
> **Открыт:** Plan 25 G3a.

---

## Зачем

Текущий runtime: `compiler-codegen/nova_rt/alloc.c` — Phase-0 implementation:

```c
void* nova_alloc(size_t size) {
    void* p = malloc(size);
    /* ...check OOM... */
    _alloc_count++;
    return p;
}

void nova_retain(void* ptr)  { (void)ptr; }  /* no-op */
void nova_release(void* ptr) { (void)ptr; }  /* no-op */
```

**Объекты создаются и никогда не освобождаются.** `nova_release` —
no-op. Codegen не вставляет cleanup calls.

**Следствие:** любой процесс, который аллоцирует >0 объектов в loop,
упадёт по OOM. CLI tools работают только потому что процесс короткий
(аллоцирует < доступной RAM).

Это **критический gap** — Nova сейчас не production-grade ни для какого
server-side workload'а, даже single-traffic-host.

## Что готово в репо

**Boehm GC backend:** `compiler-codegen/nova_rt/alloc_boehm.c`.

```c
void* nova_alloc(size_t size) {
    void* p = GC_malloc(size);
    /* ...check OOM... */
    memset(p, 0, size);
    return p;
}
```

Использует [bdwgc](https://github.com/ivmai/bdwgc) (Boehm-Demers-Weiser GC) —
**production-grade** mark-and-sweep, conservative tracing GC,
работает с любым C codegen без cooperation.

**vcpkg packages уже установлены** в репо:

- `compiler-codegen/vcpkg_installed/x64-windows-static/lib/gc.lib`
- `compiler-codegen/vcpkg_installed/x64-windows-static/include/gc.h`
- `compiler-codegen/vcpkg_installed/x64-windows-static/include/gc/`

Зависит ещё от `atomic_ops.lib` (тоже в vcpkg) для thread-safe operations.

## Чего НЕ хватает

Test-runner и codegen build chains hardcoded используют **только**
`alloc.c`. Нужно:

1. **Build flag `--gc=malloc|boehm`** в `nova-codegen test-build` и
   `test-all` (default определить после Шага 4).
2. **Conditional source pick** в `test_runner.rs:build_command`: вместо
   hardcoded `rt_alloc = opts.rt_dir.join("alloc.c")` — выбор по
   GC kind.
3. **Conditional link flags** для Boehm:
   - Windows: `gc.lib`, `atomic_ops.lib` + include path.
   - Linux/macOS: `-lgc` (apt install libgc-dev / brew install bdw-gc).
4. **Cross-platform vcpkg** — текущий x64-windows-static.
   Linux/macOS — system package managers либо vendored build.

## Фазы

### Ф.1 — Add `--gc` flag, malloc default

**Что:**
- `--gc=malloc|boehm` в `test-build` и `test-all` CLI args.
- Default: `malloc` (текущее поведение, no regression).
- `build_command` выбирает alloc source по flag.
- Boehm path добавляет gc.h include + gc.lib link для Windows.

**Файлы:**
- `compiler-codegen/src/main.rs` — добавить `--gc` arg в TestBuild/TestAll.
- `compiler-codegen/src/test_runner.rs`:
  - `BuildOpts` field `gc_kind: GcKind { Malloc, Boehm }`.
  - `build_command`: выбор `alloc.c` либо `alloc_boehm.c` по kind.
  - Conditional include path + link для Boehm.
- `run_tests.ps1`/`run_tests.sh`: pass-through `-Gc` parameter.

**Acceptance:**
- `nova-codegen test-all --gc malloc` works как сейчас (regression).
- `nova-codegen test-all --gc boehm` builds успешно на Windows MSVC/Clang.
- Smoke test: simple alloc-loop процесс не растёт unbounded под Boehm.

**Объём:** ~100 строк Rust + path config.

### Ф.2 — Verify Boehm GC actually collects

**Что:** test что Boehm реально освобождает unreachable objects.

**Tests:**

```nova
test "boehm gc collects unreachable objects" {
    // Force-create 100k records, drop reference, force gc.
    let mut last int = 0
    for i in 0..100_000 {
        let p = Point { x: i, y: i }  // unreachable после iter
        last = p.x
    }
    // Под malloc: live_count == ~100_000.
    // Под Boehm: live_count << 100_000 (collected).
    // Probe через extern nova_gc_live_count().
    let live = _nova_gc_live_count()
    assert(live < 50_000)  // generous bound
}
```

Через `external fn _nova_gc_live_count() -> int` для probing.

**Acceptance:**
- Bench under `--gc boehm` shows live_count << alloc_count после workload'а.
- Under `--gc malloc` тест fail'ит (как expected — leak forever).
- Long-running stress (10M alloc/drop iter) bounded в memory.

**Объём:** ~50 строк test + minor codegen extern probe.

### Ф.3 — Boehm GC pause benchmark

**Что:** measure pause times на realistic workloads.

**Bench:**
- 10k objects allocated → force `GC_gcollect()` → measure ms.
- 100k objects allocated → same.
- 1M objects allocated → same.
- 10M objects allocated → same (если RAM позволяет).

**Acceptance:**
- Bench numbers записаны в spec/overview.md (замена unverified «<1ms»).
- Pause distribution (p50/p99/p99.9) per heap size.

**Объём:** ~80 строк bench + spec update.

### Ф.4 — Switch default к Boehm

**Что:** после Ф.1-Ф.3 если Boehm fit (pauses acceptable, no
regressions) — сделать default GC.

**Trade-off:**
- **Боenhm pros:** correct cycle collection, no cooperation needed,
  production-tested. Default = production-ready behavior.
- **Boehm cons:** STW pauses (10-100ms на больших heap'ах), conservative
  tracing (может удержать память дольше, false roots).
- **Альтернатива** — RC через alloc_rc.c + codegen-rewrite. Это
  отдельный план (Plan 28 если потребуется).

**Acceptance:**
- `test-all` без `--gc` использует Boehm.
- 138/138 regression PASS.
- `nova_alloc` overhead не ухудшает sleep_bench / cancel_stress больше
  чем на 10%.

**Объём:** small (флаг default + docs).

### Ф.5 — Linux/macOS GC support

**Что:** Boehm GC на Linux/macOS.

**Linux:** `apt install libgc-dev` либо vendored build. `-lgc` link.
**macOS:** `brew install bdw-gc` либо vendored. `-lgc` link.

**Объём:** ~50 строк build invoker.

## Risks / Trade-offs

**R1. Boehm conservative tracing** — на 64-bit machine ложные roots
маловероятны, но возможны (integer выглядит как pointer). Эффект —
**memory не выпускается** в некоторых случаях. Practical impact:
generally небольшой, документировано в Boehm community decades.

**R2. STW pause latency.** На больших heap'ах (>1 GB) pauses могут быть
10-100ms. Для real-time это блокер. Для general backend — приемлемо.

**R3. vcpkg dependency на Windows.** vcpkg_installed уже vendored
в репо — это **plus**, не нужно external setup. На clean machine —
требуется restore (vcpkg install bdwgc). Documented in build chain.

**R4. Boehm has thread-stack scanning** — может тормозить fiber-heavy
workloads. Под M:N (Plan 23) потребует tuning.

## Acceptance overall

После Ф.1-Ф.4:

- `memory_growth_check` тест: 10M alloc/release iter, RSS bounded
  (не растёт linearly).
- `sleep_bench` 10k concurrent: no regression больше 10%.
- Spec/overview.md: «GC pause distribution» секция с measured
  numbers (вместо unverified `<1ms`).

После Ф.5: same на Linux.

## Что НЕ входит

- **RC codegen** (alloc_rc.c с retain/release inserting) — Plan 28
  если потребуется альтернатива Boehm.
- **Concurrent GC custom** — годами работы, после v1.0.
- **realtime nogc { }** arena allocator — Plan TBD, partial implementation.

## Связь

- [Plan 25 G3a/G3b](25-production-readiness-roadmap.md#g3) — этот план
  closes G3a, частично G3b.
- [D6 (spec/decisions/05-memory.md)](../../spec/decisions/05-memory.md#d6) —
  «managed by default» дизайн-decision, Plan 27 = первая реализация.
- [Plan 23 (M:N runtime)](23-mn-runtime-roadmap.md) — потребует GC
  cooperation cross-thread. Boehm thread-safe готов, RC потребует
  atomic refcount.

## История

- **2026-05-11** — создан после Plan 22 hardening retro + Plan 25 honest
  pass: обнаружено что default alloc = plain malloc без GC, любой
  long-running процесс leaks. vcpkg gc.lib уже vendored — switch к
  Boehm = ~1 день инженерной работы.
