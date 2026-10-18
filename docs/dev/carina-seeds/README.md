<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Pinned Carina bootstrap assets

These platform-specific JSON descriptors are metadata only and match the
caller-supplied schema for `scripts/tools/carina-gate.py verify-seed`. The
Windows descriptor pins the Carina 0.2.1 alpha candidate archive and source
revision `3729abcce42ec2d94efc898ec6eafe91ff6210d5`. Its ZIP SHA-256 is
`7e8264fd7b72d4005ca9bde585a6a732d3e7d6c248b65fd5787cd4a766e69cdb`; the
archive contains `novac.exe` (3,663,360 bytes, SHA-256
`824bf597f8f073c68961df8b824ef5d4e72c4b463c47b727a4e121716f6996f4`). This
B1 smoke artifact does not prove B/C fixed-point equality. Stage A was built by
published Oracle 0.1.0. The release URL is reserved for the immutable asset to
be published after the owner tags the accepted commit. The executable and
private/local receipts are deliberately not stored here.

The Linux descriptor pins only the published Oracle 0.1.0 Linux archive as a
temporary stage A. GitHub's release API reports the pinned SHA-256. The archive
was not smoke-tested here, and this pin is not evidence of Linux Carina B or
B≡C. Verification requires `--allow-oracle-stage-a` and a nonempty reason;
downstream tested binaries must be Carina B/C.

The Windows descriptor does not activate the 0.3 gate or supply a Carina
build/test driver. Linux remains a separate future proof/release track. No
release asset is changed by these pins.
