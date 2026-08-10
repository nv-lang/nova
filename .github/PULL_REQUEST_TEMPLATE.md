<!-- SPDX-License-Identifier: CC-BY-4.0 -->
## What this changes

<!-- One paragraph: what breaks today, and what this makes possible. Not a list
     of files — a reader should understand the problem before the diff. -->

## Registry entry

<!-- Nova tracks defects in docs/plans/221.1-bug-sweep.md. If this fixes one,
     name it: "Closes #NNN" (the registry number, not a GitHub issue number).
     If it fixes something not yet recorded, say so — a maintainer will assign
     a number. Work with no entry is invisible to planning. -->

- Registry: #
- Plan (if any):

## Fixing the class, not the carrier

<!-- This is the question we will ask first, so answer it here.

     A patch that makes one failing test pass is not accepted on its own. If
     the same mistake can exist elsewhere, say where you looked and what you
     found. If you fixed one site of several, say which sites remain — an
     honest partial fix is welcome, a partial fix presented as complete is not.
-->

## How you proved it

<!-- "It compiles" and "it does not crash" are not proof. We ask for two things:

     1. A fixture that asserts observable behaviour — a value, a counter, an
        exit code — not the absence of a crash.
     2. The sabotage probe: break your own fix, watch the fixture go red,
        restore it, watch it pass. Paste both outcomes. A fixture that would
        be green without your change proves nothing.
-->

- [ ] Fixture added (path: )
- [ ] Sabotage probe shown, both directions
- [ ] `bash scripts/gate.sh` run locally — paste the verdict line

## Language and spec

- [ ] Commit messages are in English (subject and body)
- [ ] If this changes the language: a D-block in `spec/decisions/` and its
      overview page `spec/<topic>.md` are in **this** pull request. A
      language change without the spec is not merged, and the D-block number
      is assigned by a maintainer — do not pick one.

## Anything you could not finish

<!-- Say it plainly here rather than leaving it to be discovered. A named gap
     is a contribution; a hidden one costs someone a day. -->
