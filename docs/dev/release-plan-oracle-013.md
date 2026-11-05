# Nova Oracle 0.1.3 (alpha)

**Nova Oracle 0.1.3** is the third quality patch of the Oracle line. One
additive standard-library API plus an honest set of exclusions: the two
deeper defects found by the live workaround sweep (the NovaValue_Output
C-emitter name collision, items 14, 21 and 22, and the lost signal in the
main-fiber root of item 19) are localized with saved reproduction evidence
but need design passes, so they are NOT included in this release (see
"Known limitations").

## Highlights

- **`io.read_line[R Read]`** — read one line over the `Read` protocol (up to
  and including the first `\n`, consumed but not included; `Ok("")` = EOF
  when nothing was read). Byte-wise 1-byte reads keep the documented
  fill-semantics `read_in` contract untouched, so line input works against
  the fill-only stdio primitive -- the workaround the product had to carry
  byte-wise (task 268). Invalid UTF-8 mid-line → `IoError{InvalidData}`,
  mirroring `read_to_string`. Mock-covered (scripted stdin, EOF,
  no-trailing-newline).
- (item 31; additive, no contract change -- the documented fill semantics of
  `read_in` is untouched.)

## Also in this tag (not by this wave)

- **Task 137 integration (19 commits, a separate window's work)**: gate
  findings of the 137 candidate closed, registry rows 1953-1955 assigned,
  `carina_text_backtick_tag` fixture. Listed here for the release chronology;
  the work and its evidence live in the task-137 records, not in this wave's
  files.

## Mark-sweep note

The compiler fixes in this release are what a batch of documented
workaround marks in a downstream product (built on this toolchain) was
waiting for: 72 `ORACLE-WORKAROUND(N)` marks there are now removed, and
the simplified code lands in that product repository through its own
acceptance process -- not in this compiler release. The marks referenced
spike items in that product'''s defect index; each fix here is pinned by a
conformance fixture, so the removals are verifiable against this release.

## Known limitations

- Everything from 0.1.1/0.1.2 applies (alpha, API not frozen,
  stop-the-world GC).
- Items 14/21/22 (the C-emitter `NovaValue_Output` collision: a user type
  named like a std/package type breaks the generated C) — root localized,
  minimal repro saved (`spikes/env/repro_output_c_collision/` in the
  product repo); needs a mangling design pass, not in 0.1.3.
- Item 19 root 3 (a rare lost wake of the main fiber under load): the exact
  window is documented at the G1 store in `nova_sched.h` and traced by
  `NOVA_DIAG_ITEM19_R3=1`; a minimal fix was implemented and rejected by an
  A/B under load (hang → segfaults). A scheduler design pass is filed.
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
