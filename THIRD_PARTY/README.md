# Third-Party Licenses

This directory documents third-party software and components integrated into Nova.

## Vendored Sources

### 1. Go Runtime (go-LICENSE)
- **Component**: Run queue implementation (`compiler-codegen/nova_rt/runq.h`)
- **License**: BSD-3-Clause
- **Source**: Go 1.4 runtime (https://github.com/golang/go)
- **Copyright**: 2009 The Go Authors

The Nova run queue is adapted from Go's scheduler, specifically the runqput/runqget/runqgrab/runqputslow algorithms.

### 2. minicoro + LuaCoco (minicoro-LICENSE)
- **Component**: Asymmetric stackful coroutine library (`compiler-codegen/nova_rt/minicoro.h`)
- **License**: Unlicense (Public Domain) OR MIT No Attribution (your choice)
- **Source**: https://github.com/edubart/minicoro
- **Copyright**: 2021-2023 Eduardo Bart
- **Subcomponent**: Assembly code from LuaCoco by Mike Pall
  - **License**: MIT
  - **Copyright**: 2004-2016 Mike Pall
  - **Source**: https://coco.luajit.org/

Minicoro is used for fiber/coroutine implementation in Nova's M:N runtime.

## External Dependencies (vcpkg)

These dependencies are not vendored but installed via vcpkg at build time:

### 3. Boehm GC
- **Component**: Garbage collector (`compiler-codegen/nova_rt/alloc_boehm.c`)
- **License**: LGPL-2.0+
- **Source**: https://github.com/ivmai/bdwgc
- **Usage**: Provides full tracing garbage collection for Nova managed heap

### 4. libuv
- **Component**: Event loop and threading (`compiler-codegen/nova_rt/bench.h`, channels)
- **License**: MIT
- **Source**: https://github.com/libuv/libuv
- **Usage**: Cross-platform I/O and threading primitives (timer, thread creation)

## Summary Table

| Component | License | Location | Type |
|-----------|---------|----------|------|
| Go Runtime | BSD-3-Clause | compiler-codegen/nova_rt/runq.h | Vendored |
| minicoro | Unlicense/MIT | compiler-codegen/nova_rt/minicoro.h | Vendored |
| LuaCoco | MIT | compiler-codegen/nova_rt/minicoro.h (component) | Vendored |
| Boehm GC | LGPL-2.0+ | vcpkg_installed/ | External |
| libuv | MIT | vcpkg_installed/ | External |

## License Files

- `go-LICENSE` — Full BSD-3-Clause license text for Go Runtime
- `minicoro-LICENSE` — Full license texts for minicoro and LuaCoco components
