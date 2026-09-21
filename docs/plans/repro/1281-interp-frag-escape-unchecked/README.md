# Unknown escape inside an interpolated string's literal piece was never checked

Found window Carina, 2026-09-21, while reading `[M-novac-lex-escape-byte-step]`
(`docs/plans/backlog-followups.md`) -- a different, older suspicion about the
lexer's `i += 2` escape step landing mid-character. That suspicion holds up on
re-reading (see "What the original marker asked, answered" below); this is a
separate, real defect found while tracing who actually reads an interpolated
string's literal bytes.

## The gap

`novac/src/check/strings.nv`'s `@report_if_bad_escape` is the ONLY place that
refuses an unknown escape (`\q`, `\x`, anything outside `\" \\ \n \t \r $`).
It used to be called from exactly one site: `check.nv`'s `Lit` arm, which only
ever sees a `NodeKind.Lit` node -- i.e. a string with NO live interpolation
(the lexer keeps an uninterpolated string as one `StrLit` leaf).

A string WITH a live `${...}` slot lexes to a different node,
`NodeKind.InterpStr`, whose children are `StrFrag` leaves (the literal text
between slots) interleaved with the slot expressions. `check.nv`'s `InterpStr`
arm never called `@report_if_bad_escape` on anything, and `exprs.nv`'s
`InterpStr` walk (the one that visits `k == NodeKind.InterpStr`) looped only
over `is_expr_kind` children -- which is true for the slot expressions and
false for every `StrFrag` leaf. So no code anywhere ever looked at a StrFrag
leaf's bytes for a bad escape.

Effect: `"a\qb"` (plain) was refused; `"a\qb${o}"` (same illegal escape, one
slot added) compiled clean.

## Minimal probe, both ways

```
module probe_interp_escape

fn main() {
    ro o = 5
    println("a\qb${o}")
}
```

Before the fix: `novac check` exits 0, no diagnostic at all.
After the fix: `E_NOVAC_SUBSET`, "outside the subset: this string escape is
not compiled yet (the subset knows only \" \\ \n \t \r)" -- the identical
message a plain `"a\qb"` already gave.

Control (plain string, same escape): `"a\qb"` alone was already refused
before this fix and stays refused after -- unchanged, both runs.

Control (legal escape, real slot): `"a\nb${o}"` stays clean (0 diagnostics)
both before and after -- the fix does not turn every interpolated string into
a refusal, only the ones carrying an escape the subset does not know.

Proven both ways by revert/rebuild/reapply/rebuild (`novac/target/novac.exe`,
`nova-cli/target/release/nova.exe build novac/src/main.nv -o
novac/target/novac.exe`): reverting `strings.nv` + `exprs.nv` alone (keeping
the new `check_test.nv` case) makes the new test RUN-FAIL red; reapplying
makes it green.

## Fix

Two files, both one small change:

- `novac/src/check/strings.nv`: `@report_if_bad_escape`'s leaf-kind filter now
  accepts `TokenKind.StrFrag` alongside `TokenKind.StrLit` -- the byte-scan
  logic itself needed no change, since it already walks `leaf_text(c).bytes()`
  generically.
- `novac/src/check/exprs.nv`: the `InterpStr` arm now calls
  `@report_if_bad_escape(ikids)` once, before the per-slot loop (same order as
  the `Lit` arm: escape check before the interpolation-shape refusals),
  returning early if it reported anything.

## No self-build regression

`NOVAC_SELF_PATH=novac/src timeout 60 novac/target/novac.exe check <all
novac/src/**/*.nv except *_test.nv>` after the fix: zero occurrences of "this
string escape is not compiled yet" -- novac's own source carries no
interpolated string with an illegal escape, so the stricter check does not
regress self-build.

## Carrier caveat

The fix closes the CLASS, not just the carrier: `@report_if_bad_escape`
already walked ALL kids of whatever list it is given, filtering by leaf kind,
so the same call now covers every `StrFrag` piece of an `InterpStr` node,
not just the one in the probe above (leading fragment, middle fragment after
a `}`, and trailing fragment before the closing quote are all children of the
same `ikids` list and get the identical scan).

## What the original marker asked, answered

`[M-novac-lex-escape-byte-step]` asked whether the lexer's `i += 2` step
(four sites in `lex.nv`, now at lines 616/744/761/813) can land mid-character
when a multi-byte UTF-8 sequence follows a backslash, and whether that has
observable harm. Re-read today, independently of the finding above: it
cannot, for a structural reason the original note did not have -- every
delimiter byte the lexer's string/char scan tests against (`"` = 0x22,
`\` = 0x5C, LF = 0x0A) is ASCII (< 0x80), and every UTF-8 continuation byte is
>= 0x80 by construction. Landing on a continuation byte after a
mid-character step can therefore never be mistaken for a real delimiter --
the scan just treats it as an ordinary byte and steps forward one at a time
until it resynchronizes on the next real character boundary. Separately, the
byte-level escape validator (`@report_if_bad_escape`, both before and after
this fix) re-scans the ORIGINAL leaf text itself rather than depending on any
lexer-computed slice boundary, so a corrupted intermediate offset from the
lexer's scan never reaches a diagnostic. The marker's own "no observable
harm" conclusion holds; this note adds the proof and can close the marker.
