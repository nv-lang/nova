# Nova Oracle 0.1.3 (alpha)

**Nova Oracle 0.1.3** is the third quality patch of the Oracle line.
Composition is decided by the owner and contains NO exclusions: every
item of the workaround sweep is part of this release. Items whose
minimal fix was rejected by an A/B (hang → segfaults) or that need a
design pass stay IN the composition and are delivered by that design
pass -- not silently dropped (owner's directive, 06.11.2026).

## Composition

| # | Item | State |
|---|------|-------|
| 1 | `io.read_line[R Read]` (item 31) | DONE — release/0.1.3 (eb1a656aa), merged to main (61d7cd2b1) |
| 2 | Bare-None if-branch unification, checker join + emitter reconciliation (item 126) | DONE — release/0.1.3, merged to main (69c5aae31) |
| 3 | Task 137 integration (19 commits, a separate window's work) | DONE — in the tag; evidence in the task-137 records |
| 4 | C-emitter `NovaValue_Output` name collision (items 14/21/22) | IN COMPOSITION — root localized, repro saved (`spikes/env/repro_output_c_collision/`); the mangling design pass is part of this release, work in progress |
| 5 | Item 19 root 3 (rare lost wake of the main fiber under load) | IN COMPOSITION — the G1 scheduler design pass is part of this release, work in progress; minimal fix was A/B-rejected (hang → segfaults) |

## Item details

- **`io.read_line[R Read]`** — read one line over the `Read` protocol (up to
  and including the first `\n`, consumed but not included; `Ok("")` = EOF
  when nothing was read). Byte-wise 1-byte reads keep the documented
  fill-semantics `read_in` contract untouched, so line input works against
  the fill-only stdio primitive -- the workaround the product had to carry
  byte-wise (task 268). Invalid UTF-8 mid-line → `IoError{InvalidData}`,
  mirroring `read_to_string`. Mock-covered (scripted stdin, EOF,
  no-trailing-newline).
- **Item 126** — a bare-None if-branch (`if cond {} else { Some(x) }` shape)
  now unifies with the other branch's `Option[X]` at the checker join, and
  the emitter prefers the non-bare branch's materialization. Pinned by a
  conformance fixture; verified on the product carrier.
- **Items 14/21/22** — the C emitter stamped `Named{"Message"}`-style names
  verbatim, so a user type named like a std/package type breaks the
  generated C (`NovaValue_Output` collision). Root localized
  (`emit_c.rs` record_def / `type_def_modules`); the fix is a mangling
  design pass for user-vs-std type names in the generated C.
- **Item 19 root 3** — a rare lost wake of the main fiber under load: the
  exact window is documented at the G1 store in `nova_sched.h` and traced
  by `NOVA_DIAG_ITEM19_R3=1`; a minimal fix was implemented and rejected
  by an A/B under load (hang → segfaults). The G1 scheduler design pass
  (latch/wait ownership) delivers the fix in this release.

## Mark-sweep note

The compiler fixes in this release are what a batch of documented
workaround marks in a downstream product (built on this toolchain) was
waiting for: 72 `ORACLE-WORKAROUND(N)` marks there are now removed, and
the simplified code lands in that product repository through its own
acceptance process -- not in this compiler release. The marks referenced
spike items in that product's defect index; each fix here is pinned by a
conformance fixture, so the removals are verifiable against this release.

## Known limitations

- Everything from 0.1.1/0.1.2 applies (alpha, API not frozen,
  stop-the-world GC).
- Item 17 (an Atomic* handle copied through a value record) — already
  closed earlier by D447 `#no_copy`; verified live (fiber/main share the
  heap cell correctly on 0.1.2).
- Unit-variant constructors of generic sums without arguments still have no
  mono route (filed during item 8).
- `nova build` of programs ICEs in `sem/channel.nv:812` in some shapes
  (pre-existing; filed).

## Links

- [Quickstart](https://github.com/nv-lang/nova/blob/main/docs/guide/quickstart.md)
- Previous: [Oracle 0.1.2](https://github.com/nv-lang/nova/releases/tag/oracle-v0.1.2)
