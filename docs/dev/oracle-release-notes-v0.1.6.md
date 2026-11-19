# Nova Oracle 0.1.6 (alpha)

Oracle 0.1.6 is based on Oracle 0.1.5 and retains its Windows x64
packaging and static Boehm GC configuration.

- Effect handlers and other fiber-owned TLS slots now follow a fiber across
  worker migration. Typed `Fail[E]` handlers participate in snapshots and GC
  rooting in the same registration order on every worker.
- Nested resumes preserve the caller's handler frame and finalizers.
  Cross-effect interrupt routing checks the owner frame's stack.
- Windows GC reads fiber arena configuration before installing its
  stop-the-world root callback. The callback avoids CRT locks.
- Split C builds in `nova test` use the same name for generated includes and
  the written common header.

See registry entries 1986–1990 and [the TLS audit](tls-slot-audit.md).
