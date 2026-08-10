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

### 3. Boehm-Demers-Weiser GC (`bdwgc`) — VENDORED AS A SUBMODULE
- **Component**: garbage collector; consumed by `compiler-codegen/nova_rt/alloc_boehm.c`
- **Vendored at**: `compiler-codegen/nova_rt/gc` (git submodule, see `.gitmodules`)
- **License**: MIT-style (the bdwgc licence; the project is NOT LGPL — see its own
  `LICENSE` file in the submodule)
- **Source**: https://github.com/bdwgc/bdwgc
- **Note (2026-08-10)**: this entry previously said “external via vcpkg, LGPL-2.0+”.
  Both parts were stale: the collector is now vendored as a submodule, so we
  redistribute its sources, and the licence statement was simply wrong.

### 3a. libatomic_ops — VENDORED AS A SUBMODULE
- **Component**: atomic primitives required by bdwgc
- **Vendored at**: `compiler-codegen/nova_rt/libatomic_ops` (git submodule)
- **License**: MIT (see the submodule's own `LICENSE`)
- **Source**: https://github.com/bdwgc/libatomic_ops

### 4. libuv — VENDORED AS A SUBMODULE
- **Component**: event loop, networking and threading — the substrate of
  `nova_rt/net.c`, `nova_rt/eventloop.c` and the channel/timer machinery
- **Vendored at**: `compiler-codegen/nova_rt/libuv` (git submodule, pinned to v1.52.1)
- **License**: MIT (see the submodule's own `LICENSE`)
- **Source**: https://github.com/libuv/libuv
- **Note (2026-08-10)**: previously described as “external via vcpkg”. It is
  vendored, and vendoring means we redistribute the sources — a different set of
  obligations than linking against something the user installed.

## Summary Table

| Component | License | Location | Type |
|-----------|---------|----------|------|
| Go Runtime | BSD-3-Clause | compiler-codegen/nova_rt/runq.h | Vendored |
| minicoro | Unlicense/MIT | compiler-codegen/nova_rt/minicoro.h | Vendored |
| LuaCoco | MIT | compiler-codegen/nova_rt/minicoro.h (component) | Vendored |
| bdwgc (Boehm GC) | MIT-style | compiler-codegen/nova_rt/gc | Vendored (submodule) |
| libatomic_ops | MIT | compiler-codegen/nova_rt/libatomic_ops | Vendored (submodule) |
| libuv | MIT | compiler-codegen/nova_rt/libuv | Vendored (submodule) |

## License Files

- `go-LICENSE` — Full BSD-3-Clause license text for Go Runtime
- `minicoro-LICENSE` — Full license texts for minicoro and LuaCoco components
