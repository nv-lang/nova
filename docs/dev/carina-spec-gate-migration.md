<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Carina-first gate migration (274.16)

## Current dependency inventory

| Lane | Current behavior | Oracle dependency | Normative status |
|---|---|---|---|
| `scripts/gate-novac.sh` | Builds `novac/src/main.nv`, then runs convention/build/runtime guards | Requires a fresh `nova-cli` Oracle executable to compile novac; may borrow the main-tree GC | Required `novac-gate` CI job, not a spec judge |
| `scripts/guards/check-novac-differential.sh` | Checks accepted/rejected outcomes and smoke behavior for fixtures | Requires Oracle for check, compilation and runtime; treats `novac/divergences.allow` as permitted comparison differences; currently has an empty-selection failure | Required through `gate-novac.sh`; diagnostic comparison only after migration |
| `scripts/tools/novac-diff-corpus.sh` | Compares examples and self-build distance, maintains ratchets | Requires Oracle checks and Oracle behavior smoke; `divergences.allow` affects its counts | Called by full differential stage; not a spec judge |
| `.github/workflows/nova-gate.yml` / `novac-gate` | Linux builds Rust `nova`, runs conventions/differential, separately builds novac | Rust Oracle build and its cache are explicit job dependencies; Linux builds novac but has no Linux B≡C proof | Keep required until replacement lane is executable and exact-CI accepted |
| `scripts/guards/check-push-proven-by-ci.py` | Proves all required workflows green for candidate | Required workflow list includes `nova-gate` | Seven required workflows: `nova-gate`, `crate-tests`, `nova-lint`, `nova-test-regression`, `contracts-crosscheck`, `contracts-z3`, `nova-doc` |

## Lane map

| Before | Intended after proof/owner activation |
|---|---|
| Rust Oracle builds novac; Oracle differential is coupled to the required novac job | Verified pinned Carina B is the seed where a published platform asset exists; otherwise a prominently labelled temporary Oracle stage A is allowed only by explicit descriptor and reason |
| Oracle agreement and `divergences.allow` participate in the required verdict | Specification-linked expected stdout/stderr/exit and line-pinned negative fixtures are the normative Carina conformance verdict; Oracle comparison remains optional diagnostic, never an allowlist or empty PASS |
| Linux builds novac but has no B≡C evidence | Linux continues to label stage A temporary until separate 0.2.1 B proof/release; runs examples/packages under Carina B/C when executable, and never asserts Linux B≡C |
| Existing `nova-gate` / `novac-gate` required | Do not remove or bypass it until owner-named manifest/build-test driver exists, replacement coverage is executable, and exact candidate CI proves it |

`scripts/tools/carina-gate.py verify-seed` validates caller-provided published asset name/HTTPS URL/SHA256, platform, pinned seed source revision and compiler provenance against a local asset. Stage `oracle-a` requires explicit `--allow-oracle-stage-a` plus a reason and prints that it is temporary; it is not a default fallback. Validation is read-only, and no asset is downloaded or overwritten. `conformance` consumes a descriptor with a fixed `fixture_count`, per-fixture spec link, independent stdout/stderr/exit goldens, and optional line-pinned `nova:expect` negative marker; empty or removed selections fail closed.

The selftest creates all descriptors and test assets in a temporary directory and contains no live manifest or binary. `scripts/tools/carina-gate.py` is infrastructure only. The live CI switch remains blocked on the separately authorized manifest and a Carina build/test driver. Linux B/C and running examples/packages under Carina B/C are likewise not claimed by this infrastructure-only change. Release assets remain immutable; this change neither publishes nor replaces them.
