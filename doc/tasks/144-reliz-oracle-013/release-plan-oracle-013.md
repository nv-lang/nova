# Nova Oracle 0.1.3 (alpha) — итоговый план

**Nova Oracle 0.1.3** is the third quality patch of the Oracle line.
Composition is decided by the owner and contains NO exclusions and NO
deferrals: every item of the approved fixes wave (21/14/22, 16, 17, 18,
49, 126, 25, 19-к3, 20) plus item 31 (read_line) plus the
print/println → Io core is part of THIS release. Owner's directives,
06.11.2026: no unilateral scope decisions; no work started without the
owner's approval; no tag without the owner's command.

## Composition (12 items — full wave, nothing excluded)

| # | Item | State |
|---|------|-------|
| — | **Состав утверждает владелец, исключений и переносов НЕТ. Все пункты утверждённой волны фиксов (21/14/22, 16, 17, 18, 49, 126, 25, 19-к3, 20) + item 31 (read_line) + ядро print/println → Io — в этом релизе.** | утверждение владельца, 06.11.2026 |
| 1 | `io.read_line[R Read]` (item 31) | DONE — release/0.1.3 (eb1a656aa), merged to main (61d7cd2b1) |
| 2 | Bare-None if-branch unification, both doors (item 126, doors) | DONE — release/0.1.3, merged to main (69c5aae31) |
| 3 | Item 126 remainder (`126-остаток`) | IN COMPOSITION — work pending, owner-approved scope |
| 4 | print/println → Io (core) | IN COMPOSITION — work pending; owner restored it to the composition, twice lost from the plan, now recorded |
| 5 | Item 16 | IN COMPOSITION — work pending |
| 6 | Item 17 (Atomic* handle copied through a value record) | IN COMPOSITION — closed earlier by D447 `#no_copy`, verified live on 0.1.2 (fiber/main share the heap cell); closure evidence is part of this release's records |
| 7 | Item 18 | IN COMPOSITION — work pending |
| 8 | Item 49 | IN COMPOSITION — work pending; the natural repro did NOT reproduce on 0.1.1, so the real carrier shape from the product is required (fact reported to the owner; exclusion NOT implied) |
| 9 | Item 25 | IN COMPOSITION — work pending |
| 10 | Item 20 | IN COMPOSITION — work pending |
| 11 | C-emitter `NovaValue_Output` name collision (items 14/21/22) | IN COMPOSITION — root localized, repro saved (`spikes/env/repro_output_c_collision/` in the product repo); the mangling design pass is part of this release |
| 12 | Item 19 root 3 (rare lost wake of the main fiber under load) | IN COMPOSITION — the G1 scheduler design pass (latch/wait ownership) is part of this release; minimal fix was A/B-rejected (hang → segfaults), that rejection does NOT exclude the item |

Plus, for release chronology (not wave scope): **Task 137 integration**
(19 commits, a separate window's work) — gate findings of the 137
candidate closed, registry rows 1953-1955, `carina_text_backtick_tag`
fixture; evidence in the task-137 records.

## Item details

- **`io.read_line[R Read]`** (item 31) — read one line over the `Read`
  protocol (up to and including the first `\n`, consumed but not included;
  `Ok("")` = EOF when nothing was read). Byte-wise 1-byte reads keep the
  documented fill-semantics `read_in` contract untouched, so line input
  works against the fill-only stdio primitive -- the workaround the
  product had to carry byte-wise (task 268). Invalid UTF-8 mid-line →
  `IoError{InvalidData}`, mirroring `read_to_string`. Mock-covered
  (scripted stdin, EOF, no-trailing-newline).
- **Item 126 (doors)** — a bare-None if-branch (`if cond {} else { Some(x) }`
  shape) now unifies with the other branch's `Option[X]` at the checker
  join, and the emitter prefers the non-bare branch's materialization.
  Pinned by a conformance fixture; verified on the product carrier.
- **print/println → Io (core)** — the console text sink routes through the
  `Io` effect like the rest of the byte I/O, instead of a side channel;
  the wave core the product's stdin/stdout workarounds wait on. Detail
  level per the wave journals.
- **Items 16 / 18 / 25 / 20 / 49 / 126-остаток** — the remaining wave items
  per the defect index of the product sweep (72 marks removed, remainder
  map 23 marks / 19 items is the mandatory input); scope and evidence per
  the task journals in both repositories.
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
- Unit-variant constructors of generic sums without arguments still have no
  mono route (filed during item 8; pre-existing, not a wave item).
- `nova build` of programs ICEs in `sem/channel.nv:812` in some shapes
  (pre-existing; filed; not a wave item).

## Process rules fixed by the owner (06.11.2026)

1. No unilateral scope decisions: items are excluded from a composition
   only by the owner.
2. No work starts without the owner's approval.
3. The tag `oracle-v0.1.3` is set only by the owner's command.

## Links

- [Quickstart](https://github.com/nv-lang/nova/blob/main/docs/guide/quickstart.md)
- Previous: [Oracle 0.1.2](https://github.com/nv-lang/nova/releases/tag/oracle-v0.1.2)
