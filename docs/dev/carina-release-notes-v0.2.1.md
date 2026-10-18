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
`3729abcce42ec2d94efc898ec6eafe91ff6210d5`. The local evidence records Oracle
0.1.0 as stage A, emission of B1, native/check/emit smoke checks, a packaged
check, and a seed-pin verification. These local checks are not server CI and
do not establish a B/C fixed point. B/C equality is **not proven**; this release
makes no Oracle-free bootstrap claim.

The Windows build used GC headers from Oracle 0.1.0 and saved runtime libraries
from run2/source3729. Run1 bytes are unavailable, so no run1/run2 byte-equality
claim is made. Linux binaries and PDB files are not shipped. Linux validation
and any Linux release remain separate future work. Compiling the full examples
and package ecosystem remains a Carina 0.3 criterion and is not done here.

This release is an alpha candidate; release availability and compatibility are
not guaranteed. Use the included setup instructions and provide an external C
toolchain for native compilation.
