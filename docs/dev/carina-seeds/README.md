<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Pinned Carina bootstrap assets

These platform-specific JSON descriptors are metadata only and match the
caller-supplied schema for `scripts/tools/carina-gate.py verify-seed`. The
Windows descriptor pins the published Carina 0.2.0 B archive and its source
revision. Its stock B `novac.exe` SHA-256 is
`b3df460726e92eca612d7df3497c1a6184c07f415bf58f80001717cb2cf8536f`, verified
against the archive fetched from the published release; the executable and
receipt are deliberately not stored here.

The Linux descriptor pins only the published Oracle 0.1.0 Linux archive as a
temporary stage A. GitHub's release API reports the pinned SHA-256. The archive
was not smoke-tested here, and this pin is not evidence of Linux Carina B or
B≡C. Verification requires `--allow-oracle-stage-a` and a nonempty reason;
downstream tested binaries must be Carina B/C.

Neither descriptor activates the 0.3 gate or supplies a Linux B seed or a
Carina build/test driver. No release asset is changed by these pins.
