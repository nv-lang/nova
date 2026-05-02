# GC: размер кода и runtime overhead

Сравнение известных GC по двум метрикам:
1. **Размер исходного кода GC** (KLOC = тысячи строк) — насколько сложно реализовать.
2. **Дополнительный размер runtime** в запускаемом файле — какую цену
   платит готовый бинарь / пакет.

> ⚠️ **Часть цифр приблизительные.** GC активно эволюционируют, точные
> размеры меняются от релиза к релизу. Источники указаны в конце.

## Размер кода GC

| GC | LOC (приблизительно) | Язык реализации | Источник цифры |
|---|---|---|---|
| **ZGC** (OpenJDK) | ~25 000 (25 KLOC) | C++ | ACM TOPLAS paper «Deep Dive into ZGC» |
| **Shenandoah** (OpenJDK) | ~30 000 (по разным оценкам) | C++ | репозиторий OpenJDK, оценка |
| **G1GC** (OpenJDK) | ~50 000+ | C++ | косвенные оценки |
| **Go runtime GC** | ~10 000–15 000 | Go (mgc.go и около) | `src/runtime/mgc*.go` в Go repo |
| **.NET CoreCLR GC** | ~36 000–40 000 (`gc.cpp` один файл!) | C++ | HackerNews 2017, OpenSource issue |
| **OCaml multicore GC** | ~3 000–5 000 (compact) | C | `runtime/` в OCaml repo, оценка |
| **Erlang BEAM GC** | ~5 000–10 000 (per-process simple) | C | ERTS source, оценка |
| **Boehm-Demers-Weiser** (libgc) | ~15 000 | C | публичный репозиторий |

**Наблюдения:**

- **ZGC удивительно компактен** — 25 KLOC для sub-millisecond pause GC
  с поддержкой 16TB heap. Современные алгоритмы концентрируют
  сложность в нескольких ключевых файлах.
- **.NET `gc.cpp` — рекорд по монолитности** (~36k LOC в одном файле).
  Сообщество годами обсуждает рефакторинг.
- **OCaml/Erlang** — самые компактные (<10 KLOC), потому что:
  - OCaml использует generational + escape analysis, минимум магии.
  - Erlang делает per-process heap (не одна на VM), каждый GC простой.
- **Go GC скромен** (~10–15 KLOC), потому что:
  - Без compaction.
  - Без generations.
  - Tri-color mark-and-sweep — относительно простой алгоритм.

## Доп. размер в runtime / бинарнике

| Технология | Минимальный hello world | Минимальный runtime | Источник |
|---|---|---|---|
| **C** (gcc -O2 -static) | ~100–800 KB | ~0 (libc как dependency) | известно |
| **Rust** (release, no_std) | ~300 KB | без runtime | известно |
| **Rust** (release, std) | ~3 MB (статически слинкован) | ~3 MB | известно |
| **Go** (1.22+, default) | ~1.2 MB → 7.3 MB на разных Go-версиях | runtime включён | `Go 1.7 binary size` blog, `codestudy.net` |
| **Go** (1.22+, `-ldflags="-s -w"`) | ~7.3 MB | то же | оптимизация символов |
| **Go + UPX** | ~2.2 MB | компрессия | сжатие исполнимого |
| **TinyGo** (без полного runtime) | ~19 KB | минимальный | TinyGo project, ~98% reduction |
| **Java JRE** (полный) | требует ~200+ MB JRE | 200+ MB (полный JDK) | Oracle |
| **Java + jlink (custom JRE)** | требует ~24–32 MB JRE | 24–32 MB (java.base only) | Microsoft Learn jlink |
| **Java + jlink + strip** | требует ~10 MB JRE | ~10 MB | Jake Wharton blog |
| **GraalVM Native Image** | ~10–30 MB standalone | runtime в бинарнике | GraalVM docs |
| **.NET 8 Self-contained** | ~30–60 MB | runtime включён | Microsoft |
| **.NET 8 NativeAOT** | ~3–10 MB | минимальный native runtime | Microsoft |
| **OCaml** (native) | ~200–500 KB | minimal C runtime + GC | OCaml docs |
| **Erlang/BEAM** (escript) | ~30 MB+ runtime | ERTS обязателен | Erlang docs |
| **Haskell GHC** | ~1–10 MB | GHC RTS | GHC user guide |
| **Swift** (macOS, dynamic) | ~50 KB (linked to system) | ~0 (system framework) | Apple toolchain |
| **Swift** (Linux, static) | ~30 MB | runtime включён | Swift on Linux |

## Pause time (для сравнения с размером)

Размер GC слабо коррелирует с качеством pause. Для контекста:

| GC | Pause p99 |
|---|---|
| **Azul C4** (проприетарный) | <1ms на любом heap |
| **ZGC** | <1ms на heap до 16TB |
| **Shenandoah** | <10ms |
| **Go** | <1ms (но throughput hit ~25%) |
| **Erlang per-process** | <1ms (heap'ы маленькие) |
| **OCaml 5** | очень малая (concurrent generational) |
| **JVM G1** | ~50–200ms (throughput-focused) |
| **.NET Server GC** | ~ms (throughput-focused) |

## Что это значит для Nova

**Цели Nova по [decisions.md D6](../../decisions.md):**
- Pause: <1ms p99 (как Go/ZGC).
- Throughput: ~70–85% от C на типичной web-нагрузке.
- Realtime: zero allocation в `region { }` блоках.

**Реалистичный масштаб GC-имплементации:**

- **Минимальный концепт-доказательство** (mark-and-sweep, без
  compaction): **2–3 KLOC** на C/C++.
- **Production-ready concurrent GC** (как Go): **10–15 KLOC**.
- **Sub-millisecond pause GC** (ZGC-class): **25 KLOC**.
- **Add escape analysis в компилятор**: ещё ~5–10 KLOC в backend.
- **Add region-based allocator для `Realtime` блоков**: ~2 KLOC.

**Итого Nova GC + memory management:** ~15–25 KLOC C/C++/Rust в первой
production-версии. Это сопоставимо с Go и в 1.5–2× меньше чем .NET.

**Реалистичный размер бинарника Nova:**
- Hello world: **~2–5 MB** (как Go без оптимизаций, с GC + fiber runtime).
- С `-Os` + strip: **~1 MB**.
- Без runtime (нативный с region-only allocations, no GC): **~200–500 KB**.

Это ставит Nova в одну лигу с Go/Swift по размеру runtime, что для
системного языка с GC — норма.

## Источники

- [ACM TOPLAS: Deep Dive into ZGC](https://dl.acm.org/doi/full/10.1145/3538532) — 25 KLOC цифра.
- [Go GC source `mgc.go`](https://go.dev/src/runtime/mgc.go) — структура runtime.
- [The Smallest Go Binary (5KB)](https://totallygamerjet.hashnode.dev/the-smallest-go-binary-5kb) — экстрим минимизации.
- [How to Reduce Go Compiled File Size](https://www.codestudy.net/blog/how-to-reduce-go-compiled-file-size/) — 11MB→7.3MB.
- [Microsoft jlink Runtimes](https://learn.microsoft.com/en-us/java/openjdk/java-jlink-runtimes) — 24–32 MB minimum JRE.
- [Jake Wharton: jlink minimal JRE](https://jakewharton.com/using-jlink-to-cross-compile-minimal-jres/) — 10 MB JRE.
- [HackerNews: gc.cpp 37000 lines](https://news.ycombinator.com/item?id=13950136) — .NET CoreCLR.
- [TinyGo](https://tinygo.org/getting-started/overview/) — 19 KB embedded Go.
- [OpenJDK ZGC project](https://openjdk.org/projects/zgc/) — official.
- [Shenandoah OpenJDK Wiki](https://wiki.openjdk.org/display/shenandoah/Main).
- [Erlang Garbage Collection docs](https://www.erlang.org/doc/apps/erts/garbagecollection.html).
- [OCaml GC docs](https://ocaml.org/docs/garbage-collector).

> Цифры по `Erlang BEAM GC`, `OCaml runtime`, `Shenandoah` — не нашли
> точных данных в публичных источниках, оценки по структуре
> репозиториев и косвенным признакам. Для точности — `cloc` на исходниках.
