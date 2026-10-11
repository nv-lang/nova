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
  compatibility or a published Docker image. The Windows archive is separately
  prepared from the saved, byte-identical stock B/C evidence, not from the
  Linux CI build.

## Prepared distribution (not current CI uploads)

The 0.2.0 assets are **Windows x86_64 only**:
`nova-carina-0.2.0-windows-x86_64.zip`,
`nova-carina-0.2.0-src.tar.gz`, `proof-and-accept-receipt.txt`, and
`SHA256SUMS`. The Windows ZIP contains the saved stock B `novac.exe` from
task #55, whose SHA256 is
`b3df460726e92eca612d7df3497c1a6184c07f415bf58f80001717cb2cf8536f`;
the saved stock C executable has the same hash. The release title is
`Nova Carina 0.2.0 (alpha)` and the release is marked pre-release. The tag is
`carina-v0.2.0`; alpha is not added to tag or filenames.

Current workflow inventory is narrower than this distribution: `novac` is
built on Linux in `.github/workflows/nova-gate.yml`, is not uploaded as a
releasable artifact, and current workflows do not build `novac` for Windows.
The current CI artifact uploads are benchmark-results JSON/Markdown and the
full test report; none is a Carina release binary. The Windows archive above
comes from the accepted Windows A → B → C evidence, not from a CI upload.
There is **no Linux Carina binary in 0.2.0**: Linux CI builds `novac`, but has
not proven a Linux B ≡ C double build. That belongs to a separate 0.2.1 task.

## Release order

Owner decision dated 2026-10-11: publish Oracle first, then Carina. Carina's
version line continues **0.2 → 0.3 → 1.0**; Oracle remains **0.1.x**. The
release procedure and final acceptance checklist remain in
[274.11 §7](../plans/274.11-carina-release.md#7-процедура-выпуска-бывш-a-v6-и-a-r2--переезжает-как-заготовка).
These notes remain a draft. Local archives and checksums have been prepared;
no release has been published or tag created by preparing these notes.
