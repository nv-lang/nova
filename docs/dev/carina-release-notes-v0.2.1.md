<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Nova Carina 0.2.1 — Windows x86_64 release notes (alpha)

Nova Carina 0.2.1 is an experimental Windows x86_64 alpha pre-release of
Carina, Nova's self-hosted compiler. Compiler behavior and interfaces may
change; compatibility is not guaranteed.

## What this release contains

The Windows ZIP contains a 3,663,360-byte `novac.exe` (SHA-256
`824bf597f8f073c68961df8b824ef5d4e72c4b463c47b727a4e121716f6996f4`). The ZIP
SHA-256 is
`7e8264fd7b72d4005ca9bde585a6a732d3e7d6c248b65fd5787cd4a766e69cdb`; the
source archive `nova-carina-0.2.1-src.tar.gz` SHA-256 is
`716b58090af840326a2cdf3440590d7295015e5e6e9c07f75d7776bec7e0db67`.
`SHA256SUMS` is LF-terminated (463 bytes; SHA-256
`66ed06aba924491bfe8b292d392d9aebf27b0971a8f2a9a7ebf884b7ffa555d0`).

The candidate is built from source revision
`3729abcce42ec2d94efc898ec6eafe91ff6210d5`. **Owner decision 2026-10-19:**
«Карина с0.2.1 собирается без запуска Oracle». This claim is about the
reproducible Windows C-level bootstrap, not a native B2 build: published B1
emits `B2raw.c`, bytewise equal to the stage-A emitted `B1.c` for the pinned
source. The independent post-release raw-output run observed both C files at
13,250,302 bytes, SHA-256
`25c5404f1e8e175652f19c9ca6cb03d29de82cee70a7cd9f4258c6724115c8a0`, with
empty stderr. The managed expected-C baseline is
[`scripts/guards/carina-bootstrap-c.baseline`](../../scripts/guards/carina-bootstrap-c.baseline);
repeat it with the published ZIP or shipped executable and an explicit pinned
source tree using
`python scripts/tools/verify-carina-bootstrap-c.py <pinned-source-tree> --asset <published-zip-or-executable> --work-dir <scratchpad>`.

**Historical provenance (not hidden):** published Oracle 0.1.0 built the
initial Carina A; A emitted `B1.c`; Clang built the published B1 executable
from that C. Oracle 0.1.0 GC headers and saved run2 libraries/source3729 were
used. Oracle was a one-time historical stage A and is not executed by the
repeatable B1 emit verifier. The original post-release measurement used cwd
`<source-tree>`, `NOVA_STD_PATH=<source-tree>/std/src`, relative
`NOVAC_SELF_PATH=novac/src`, and `cmd` raw stdout redirection (not PowerShell
text redirection). An absolute `NOVAC_SELF_PATH` was not tested.

Native B2 executable/PDB/LIB byte equality is **not proven and is not required**
for this C-level criterion; this is not a claim that a native B2 smoke/build
was run. The Windows artifact is alpha. Linux binaries are not shipped and
Linux bootstrap validation remains separate. Full examples/packages and
Carina `build`/`test` remain 0.3 criteria and are not done here.

This release is an alpha candidate; release availability and compatibility are
not guaranteed. Use the included setup instructions and provide an external C
toolchain for native compilation.
