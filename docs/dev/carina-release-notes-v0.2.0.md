<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Nova Carina 0.2.0 — release notes (alpha draft)

**Nova Carina 0.2.0 (alpha)** is the accepted self-hosting milestone for
Carina, the Nova compiler written in Nova. This is an early alpha: the
language surface and APIs may change, and compatibility is not guaranteed.
This note describes the accepted compiler milestone; it does not claim that
the wider package ecosystem is ready.

## What this release represents

Carina compiles itself at the accepted 0.2 revision. The acceptance receipt
records the double-build sequence A → B → C: B and C executables are
byte-identical, B.c and C.c are byte-identical, and CI is green on the same
final source SHA. The accepted SHA is
`99756a8867f913bd6156ed10accf8a20edea00d1`.

The saved receipt is `proof-and-accept-receipt.txt:1–15` and the build
evidence is `bootstrap-stock.log:16–19` in the acceptance-evidence worktree
identified by [274.11](../plans/274.11-carina-release.md) §0. The receipt
establishes acceptance of that source revision; it is not evidence that a
release asset has been built or published. Before an actual release, verify
the same SHA and receipt again.

## Scope and known limitations

- The accepted milestone is the 0.2 self-compilation step. Carina compiling
  the wider package ecosystem and examples is a **0.3 criterion**, not part
  of the 0.2 acceptance.
- The bootstrap's stage A must come from the Oracle release; do not silently
  substitute a locally built compiler. See [Oracle plan 295](../plans/295-oracle-release-v0-1.md)
  and [the bootstrap description](novac-bootstrap.md).
- Passing the exact-SHA bootstrap and CI receipt does not certify package
  compatibility, release archives, a Windows Carina binary, or a published
  Docker image.

## Planned distribution (not current CI uploads)

The planned assets are `nova-carina-0.2.0-linux-x86_64.tar.gz`,
`nova-carina-0.2.0-windows-x86_64.zip`,
`nova-carina-0.2.0-src.tar.gz`, and `SHA256SUMS`. The release title is
`Nova Carina 0.2.0 (alpha)` and the release is marked pre-release. The tag is
`carina-v0.2.0`; alpha is not added to tag or filenames.

Current workflow inventory is narrower than the planned assets: `novac` is
built on Linux in `.github/workflows/nova-gate.yml`, is not uploaded as a
releasable artifact, and current workflows do not build `novac` for Windows.
The current CI artifact uploads are benchmark-results JSON/Markdown and the
full test report; none is a Carina release binary. The Windows archive above
is therefore a planned release deliverable, not a CI-proven binary.

## Release order

Owner decision dated 2026-10-11: publish Oracle first, then Carina. Carina's
version line continues **0.2 → 0.3 → 1.0**; Oracle remains **0.1.x**. The
release procedure and final acceptance checklist remain in
[274.11 §7](../plans/274.11-carina-release.md#7-процедура-выпуска-бывш-a-v6-и-a-r2--переезжает-как-заготовка).
This is a draft only. No tag, archive, checksum file, release, or publication
has been created by preparing these notes.
