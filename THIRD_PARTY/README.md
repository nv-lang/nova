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

## Ported Algorithms (std)

### 5. Rust `core::num::dec2flt` (rust-LICENSE)
- **Component**: correctly-rounded decimal→binary float conversion behind
  `str @to_f64()` — `std/src/runtime/float_parse/convert.nv` and its peers
  `pow5_table.nv`, `digit_buf.nv` (the files land with the phases of plan 283;
  `rust-LICENSE` lists the ones present)
- **License**: MIT OR Apache-2.0 upstream; Nova takes it under **MIT** (one file,
  one license — the same shape as the Go entry)
- **Source**: https://github.com/rust-lang/rust, `library/core/src/num/imp/dec2flt/`,
  commit `rust-lang/rust@48a229ceaefd4985c50990b14116b6d856af0985`
- **Copyright**: The Rust Project Contributors

A port, not a vendored copy: the algorithm (Clinger fast path, Eisel-Lemire over
128-bit powers of five, big-decimal slow path), its constants and the comments that
justify each rounding decision are preserved; types and primitives are Nova's. Rust's
`dec2flt` is itself a port of Daniel Lemire's fast_float.

### 6. Ryu (ryu-LICENSE, ryu-LICENSE-Boost)
- **Component**: shortest round-trip binary→decimal float printing behind
  `f64_fmt`/`f32_fmt` with `FloatKind.Shortest` —
  `std/src/runtime/fmt_buf/shortest.nv` (the file exists from plan 285 Ф.0 with this
  provenance recorded; the ported code lands with Ф.1 for `f64` and Ф.2 for `f32`)
- **License**: Apache-2.0 OR BSL-1.0 upstream (the donor's own choice, stated in each
  of its source headers); Nova takes it under **Apache-2.0**, the branch that composes
  with Nova's own `MIT OR Apache-2.0`
- **Source**: https://github.com/ulfjack/ryu, `ryu/d2s.c` with `ryu/d2s_full_table.h`
  and `ryu/d2s_intrinsics.h` (`f32`: `ryu/f2s.c`), commit
  `ulfjack/ryu@4c0618b0e44f7ef027ebae05d2cc7812048f7c8f` (2026-02-09)
- **Copyright**: 2018 Ulf Adams
- **Paper**: Ulf Adams, "Ryū: fast float-to-string conversion", PLDI 2018

A port, not a vendored copy — the donor's sources are NOT in this tree; they were read
through the GitHub API at the commit above on 2026-09-09. Both licence texts are
vendored anyway, because a port carries the notice even when the copy does not travel.

**Why this donor and not the one already used for the parse side.** Ryu produces the
shortest round-trip digits in a single pass. Rust's printing side, which would have
been the cheaper trail (same donor for both directions), is a two-path algorithm whose
fast path falls back to exact bignum arithmetic — its own module documentation in
`library/core/src/num/imp/flt2dec/mod.rs` says "They are total for all finite `f32`
and `f64` inputs (Grisu internally falls back to Dragon if necessary)". Plan 285's
acceptance for the producer forbids a check loop, so the choice follows the donor's
structure, not a preference.

## Summary Table

| Component | License | Location | Type |
|-----------|---------|----------|------|
| Go Runtime | BSD-3-Clause | compiler-codegen/nova_rt/runq.h | Vendored |
| minicoro | Unlicense/MIT | compiler-codegen/nova_rt/minicoro.h | Vendored |
| LuaCoco | MIT | compiler-codegen/nova_rt/minicoro.h (component) | Vendored |
| bdwgc (Boehm GC) | MIT-style | compiler-codegen/nova_rt/gc | Vendored (submodule) |
| libatomic_ops | MIT | compiler-codegen/nova_rt/libatomic_ops | Vendored (submodule) |
| libuv | MIT | compiler-codegen/nova_rt/libuv | Vendored (submodule) |
| Rust `core::num::dec2flt` | MIT (of MIT OR Apache-2.0) | std/src/runtime/float_parse/convert.nv (+ peers) | Ported |
| Ryu | Apache-2.0 (of Apache-2.0 OR BSL-1.0) | std/src/runtime/fmt_buf/shortest.nv | Ported |

## License Files

- `go-LICENSE` — Full BSD-3-Clause license text for Go Runtime
- `minicoro-LICENSE` — Full license texts for minicoro and LuaCoco components
- `rust-LICENSE` — Full MIT license text for the Rust `dec2flt` port, with the list of ported files
- `ryu-LICENSE` — Full Apache-2.0 license text for the Ryu port, downloaded verbatim from
  the donor at commit `4c0618b` (the branch Nova takes)
- `ryu-LICENSE-Boost` — Full BSL-1.0 text, the donor's alternative branch: kept because
  the donor grants the choice per file, and dropping the unused half would misstate the
  grant we actually received
