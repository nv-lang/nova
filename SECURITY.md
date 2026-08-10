<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Security policy

## Reporting a vulnerability

Do **not** open a public issue. Use GitHub's private reporting on the
[nv-lang/nova](https://github.com/nv-lang/nova/security/advisories/new) advisory
page, or write to `unitcraft@nv-lang.org`.

Tell us what you can reproduce, on which version, and what an attacker gets out
of it. A working proof of concept is welcome but not required — a precise
description of the flaw is enough to start.

## Scope, stated honestly

Nova is pre-1.0 and has not been audited. Two things are worth knowing before
you rely on it:

* **Memory safety is not absolute.** Nova compiles to C and offers `unsafe`,
  raw pointers and FFI. Code that uses them can corrupt memory exactly as C can.
* **Known gaps are recorded in the open.** `docs/plans/221.1-bug-sweep.md` lists
  every defect we know of, including security-relevant ones, with priorities. We
  would rather you find them there than discover them in production.

## What we consider a vulnerability

Anything that lets a program do what the language promises it cannot: escape a
declared effect set, violate a `consume` linearity guarantee in safe code, read
or write memory outside a safe abstraction, or make the compiler emit code that
contradicts the source without a diagnostic.

Bugs in `unsafe` blocks, in FFI, or in code that ignores a compiler diagnostic
are ordinary defects — file them as issues.

## Supported versions

Only the latest release and `main`. Nova is pre-1.0; there are no backports.
