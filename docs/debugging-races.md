# Debugging Races in Nova M:N Runtime — Playbook

> **Internal methodology document.** Consolidates 20 lessons learned across
> Plan 83.11 (Centralized I/O driver) Ф.3 STALE-slot race investigation,
> §11.6 GC structural aliasing fix, and §12.31 use-after-free fix —
> ~25 hours of race investigation distilled into a reusable playbook.
>
> Audience: anyone working on Nova runtime concurrency
> (`compiler-codegen/nova_rt/`), spawn/cancel/scope/driver code paths,
> or debugging stochastic SEGV/hang/deadlock on M:N runtime.
>
> Source material:
> - [`docs/plans/83.11-centralized-io-driver.md`](plans/83.11-centralized-io-driver.md) §§10.4, 12.16, 12.24, 12.27-29, 12.31
> - [`nova-private/docs/articles/mn-race-stale-slot.md`](../../../nova-private/docs/articles/mn-race-stale-slot.md) — full case study (both parts)
> - Memory `reference-mn-race-case-study.md` — distilled template

---

## TL;DR — quick reference card

```
                ┌─────────────────────────────────────────────────┐
                │  STOCHASTIC SEGV / HANG / DEADLOCK              │
                │  in M:N concurrency code                        │
                └───────────────────┬─────────────────────────────┘
                                    │
                                    ▼
         ╔═════════════════════════════════════════════╗
         ║  Step 0: Invest in DIAGNOSTIC FIRST          ║
         ║  • Cost: ~1 session (~2-4h)                  ║
         ║  • Payoff: bug locates in MINUTES, not days  ║
         ╚═══════════════════╤══════════════════════════╝
                             │
       ┌─────────────────────┼──────────────────────┐
       ▼                     ▼                      ▼
  Crash with SEGV       Hang / TIMEOUT       Test sometimes
  (Windows)             (race or             passes / sometimes
                         deadlock)            fails
       │                     │                      │
       ▼                     ▼                      ▼
  In-process VEH        Programmatic         Bisect over
  + dbghelp             state-dump           hypothesized region
  (segv_diag.c          (nova_runtime_       first; n ≥ ⌈7/p⌉
   pattern)              dump_state)         iterations per probe
       │                     │                      │
       └─────────────────────┴──────────────────────┘
                             │
                             ▼
         ╔══════════════════════════════════════════════╗
         ║  Step 1: ONE point-probe per hypothesis      ║
         ║  • Single fprintf in cold path entry         ║
         ║  • Show actual heap value, not control flow  ║
         ║  • NOT broad instrumentation (Heisen-test)   ║
         ╚═══════════════════╤══════════════════════════╝
                             ▼
                  Frame[1] or heap garbage → root cause
                             │
                             ▼
            ┌────────────────────────────────────┐
            │  Step 2-5: surgical fix + tests +   │
            │  spec amend (see §«Algorithm»)      │
            └────────────────────────────────────┘
```

**The single most important lesson:** *Invest ONE session in proper
diagnostic infrastructure. ROI is 5-10× — beats N hypothesis-driven
fix attempts every single time.*

---

## §1. The 5-step algorithm

This is the canonical path for any non-trivial race investigation.
Skip steps only if you have already-validated diagnostic output for
the same bug from a prior session.

### Step 1 — Capture deterministic repro

Required output: **stress-script that fails consistently** (at least
~7%, ideally >50%) with one-line command + bug fixture in
`nova_tests/<plan>/`.

```bash
# Canonical pattern (see scripts/stress_bisect.sh):
bash scripts/stress_bisect.sh nova_tests/<plan>/<repro>.nv 30
# Exit 0 = all 30 PASS; exit 1 = ≥1 FAIL
```

If you cannot produce a deterministic repro: stop. Production bugs
that fire <1% on dev machines need either:
- More aggressive load (more fibers, longer scope, more cancellations)
- Different OS (try WSL Linux — Boehm/GC bugs are often Windows-only)
- Build mode change (`--mode dev` vs `release` — optimization shadows races)

**Stress count heuristic:** at repro rate `p`, false-GOOD with `n` runs
is `(1-p)^n`. Use **`n ≥ ⌈-ln(0.01)/p⌉`**:
- `p = 0.40` → `n ≥ 12` (default 15 fine)
- `p = 0.20` → `n ≥ 23`
- `p = 0.07` → `n ≥ 66`
- `p = 0.01` → `n ≥ 460`

Default `n=15` is **TOO LOW** for stochastic races (>30% false-good at
p=7%). Bisect over rare races needs `n ≥ 66` minimum.

### Step 2 — Install diagnostic infrastructure (one-time, ~165-200 LOC)

Pick exactly ONE diagnostic tool based on symptom:

| Symptom | Tool | LOC | Where to put |
|---------|------|-----|--------------|
| **SEGV/AV on Windows** | VEH + dbghelp `SymFromAddr`/`StackWalk64` | ~165 | New file `compiler-codegen/nova_rt/segv_diag.c` — already exists |
| **SEGV on Linux** | gdb (Docker / WSL) or built-in signal handler | varies | `compiler-codegen/nova_rt/runtime.c` |
| **Hang / TIMEOUT** | Programmatic state-dump (watchdog-triggered) | ~80 | `nova_runtime_dump_state()` in `runtime.c` — already exists |
| **Stochastic intermittent FAIL** | Bisect via `scripts/stress_bisect.sh` + `git bisect run` | reused | `scripts/` |
| **«It worked yesterday, fails now»** | Same — bisect first, before hypothesis | reused | `scripts/` |

**Gating:** every diagnostic MUST be `getenv("NOVA_DIAG_*")` gated.
Default disabled. Zero overhead when unset. This is non-negotiable —
diagnostic that always-fires changes timing and can mask the bug it
was meant to find (Heisen-test, §10.4 Lesson #1).

### Step 3 — Run repro under diagnostic, identify frame[1] or heap garbage

For **SEGV**:
```bash
NOVA_DIAG_SEGV=1 ./test.exe 2>&1 | tail -40
```
Expected output: register file + 30-frame stack with
`module!symbol+offset (file:line)`. Frame[0] is typically `memcpy` /
`nova_aint_cas` / runtime helper. **Frame[1] is the KEYSTONE** — that's
the Nova-level caller passing the bad pointer.

For **hang**:
- Run with `NOVA_WATCHDOG_DUMP_SECS=5` env var
- After 5 seconds of waiting, runtime dumps full state:
  - All workers (deque, runnext, wake_pending)
  - All fibers (parked/pending_wake per slot)
  - All supervised scopes (count, pending_remote, armed_sleeps_head)
- **«Impossible state» combinations point at root cause**, e.g.:
  - `fibers[i] = NULL` AND `parked[i] = true` → STALE-slot (§10)
  - `count = 372` AND `slot_lock = -1.2B` AND `head = .rdata` → garbage scope = use-after-free (§12.31)
  - `pending_remote = N` AND all workers idle → lost-wake somewhere

### Step 4 — ONE point-probe per hypothesis (NOT broad instrumentation)

If frame[1] / state-dump doesn't immediately reveal cause: add ONE
single `fprintf` at function entry showing actual heap value:

```c
static void _nova_driver_handle_cancel_scope(NovaFiberQueue* scope) {
    if (!scope) return;
    if (getenv("NOVA_DIAG_CSD")) {
        fprintf(stderr, "[csd] scope=%p bound_token=%p head=%p count=%d slot_lock=%d\n",
                (void*)scope, scope->bound_token,
                (void*)scope->armed_sleeps_head, scope->count,
                (int)nova_aint_load(&scope->slot_lock));
        fflush(stderr);
    }
    // ... existing code unchanged ...
}
```

This is a **point-probe**, NOT a Heisen-test (see §«Anti-patterns»).
Key properties:
- Single fprintf in cold path (entry, not hot loop)
- Prints actual heap value at known offset
- Gated by env var, zero overhead default
- Does NOT modify control flow

Run 5 iterations of the failing test. Compare values:
- Values consistent across runs → bug is **deterministic at that point**
- Values vary → bug is **timing-dependent**, look upstream
- Values look like garbage (random ints, `.rdata` addresses, NULL where
  unexpected) → **use-after-free or aliasing**

### Step 5 — Surgical fix + spec + tests + closure

Once root cause is identified:

1. **Fix the bug** with the minimal change that addresses root cause.
   No drive-by refactoring (per CLAUDE.md «Don't add features beyond
   what the task requires»).

2. **Add at least ONE negative regression test** in `nova_tests/<plan>/`
   that fails before fix, passes after. Lives forever.

3. **Author spec D-block** if this introduces a new invariant. Example:
   - §12.31 → D228 §6: `pending_driver_jobs` lifetime counter
   - §11.6 → D228 §7: `ctx_pins[]` GC-root anchor pattern

4. **Update plan doc** with closure section (root cause analysis +
   fix design + lessons learned). New entries become future material
   for THIS playbook.

5. **Update logs** (3 files):
   - `docs/project-creation.txt` — formal closure entry
   - `docs/simplifications.md` — closure if relevant
   - `nova-private/discussion-log.md` — narrative entry

6. **Update memory** (`project-<plan>-status.md`) for next session.

7. **Commit + merge + push** in logical groups (fix / tests / spec / docs).

---

## §2. Decision tree — which diagnostic when?

Use this tree to pick the right tool. **Wrong tool = wasted session.**

### 2.1 Symptom recognition

```
Q1: Does the test process EXIT non-zero?
  ├─ YES → likely SEGV/AV/abort
  │   └─ Q2: Is it Windows?
  │       ├─ YES → §3.1 VEH + dbghelp (segv_diag.c)
  │       └─ NO  → gdb (WSL Linux is preferred over native Linux
  │                     because /proc/maps is reliable)
  │
  └─ NO → likely hang or wrong-result
      └─ Q3: Does it TIMEOUT (> 60s)?
          ├─ YES → §3.2 state-dump (NOVA_WATCHDOG_DUMP_SECS)
          └─ NO  → wrong-result; instrument differently
              └─ Q4: Is value computed correctly some runs, garbage others?
                  ├─ YES → §3.3 point-probe (§1 Step 4)
                  └─ NO  → assertion logic bug, not race; fix Nova-side
```

### 2.2 «It worked yesterday, fails today»

**ALWAYS bisect first.** Hypothesis-driven debugging on regressions is
~5× more expensive than bisect (proven empirically: §12.21 spent 3+
hours via WSL on wrong commit region; 20-min bisect identified the
real culprit).

```bash
# Set up bisect range
git bisect start
git bisect bad HEAD
git bisect good <known-good-sha>

# Use stress_bisect as bisect-run script
git bisect run scripts/stress_bisect.sh nova_tests/<plan>/<repro>.nv 30

# Convergence in log2(N) steps, e.g. 184 commits → ~8 iterations
```

**Pitfall:** with default `n=15` stress at low `p`, bisect false-good
rate is ~34% at `p=0.07`. **Bump to `n=66+`** for any race investigation
where `p < 0.20`.

**Verify the graph before bisecting** (§12.29 lesson 13): use
`git log --graph --oneline good..bad` to confirm branch topology.
§12.28 misread branch topology and wasted a full session bisecting
the wrong region.

### 2.3 «Bug is rare, only in production-scale tests»

Symptom: works at 99 fibers, fails at 990+. Or works at 100ms sleep,
fails at 10s sleep. Race conditions exposed only at scale.

**Tools:**
- **`stress_iso_large` pattern** (Plan 83.11 §11.6): scale up the
  failing dimension by 10× in a dedicated test file. Keep small repro
  for fast iteration.
- **State-dump at full scale** is best — watchdog fires at 5s, dumps
  state of all 990+ fibers. «Impossible state» combinations stand out.

### 2.4 «Bug only on Windows»

Plan 83.11 §12.21 BREAKTHROUGH lesson: **always test on WSL Linux
FIRST** when Windows tooling fails. Bug repro on Linux = different
mental model (signal-based stack walk gives clean ucontext); bug
absent on Linux = strong hint at Windows-specific cause (SuspendThread,
TIB, conservative GC).

```bash
# Quick WSL repro test
wsl -d Ubuntu
cd /root/nova   # pre-existing Linux mirror
nova test nova_tests/<plan>/<repro>.nv --stress 30
```

If WSL passes 30/30 while Windows fails 0/30: bug is **Windows-only**.
Most likely causes (in order):
1. Boehm GC SuspendThread+stack-walk vs minicoro fiber stack swap
2. TIB.StackBase/StackLimit visibility differences
3. MSVC ABI calling-convention edge cases
4. Windows libuv backend differences (wepoll vs epoll)

For Windows-only: use **VEH + dbghelp in-process** (§3.1). cdb is
optional — `segv_diag.c` provides equivalent capability via Windows-
native APIs.

---

## §3. Tooling reference

### 3.1 In-process VEH + dbghelp crash localizer

**File:** [`compiler-codegen/nova_rt/segv_diag.c`](../compiler-codegen/nova_rt/segv_diag.c) (~165 LOC)

**Replaces:** cdb / WinDbg / external symbolizer. Windows-only.

**Activation:**
```bash
NOVA_DIAG_SEGV=1 ./test.exe
```

**Output on SEGV:**
```
=== [SEGV-DIAG] EXCEPTION_ACCESS_VIOLATION ===
ExceptionAddress: 00007FF6...88F (RIP at fault)
AccessMode:       WRITE
FaultAddress:     0x00007FF6...547
RIP RAX RBX RCX RDX ... (full register file)

=== Stack trace ===
  #00 ... nova_aint_cas+0x2F (sync.h:169)
  #01 ... _nova_driver_handle_cancel_scope+0x61 (driver.c:312)   ← KEYSTONE
  #02 ... _nova_driver_process_job+0x5A
  ...
```

**Why this beats cdb (per §12.31):**
- cdb requires Windows SDK install (admin elevation via `winget`)
- llvm-symbolizer WITHOUT DIA cannot read MSVC PDB → returns `??:0:0`
- dbghelp.dll is in `C:\Windows\System32` on every Windows machine
- dbghelp.lib already linked in `test_runner.rs` (Plan 27+)
- One file + one hook = ~165 LOC, no install required

**Anti-pattern:** do not use llvm-symbolizer for MSVC PDB on Windows
build of LLVM that lacks DIA support (which is most of them, including
VS BuildTools 18). It falsely resolves all `lock cmpxchg` instructions
as `memcpy` via fallback misclassification (§12.31 misdirection root).

### 3.2 Programmatic state-dump with watchdog

**File:** `nova_runtime_dump_state()` in `compiler-codegen/nova_rt/runtime.c`

**Activation:**
```bash
NOVA_WATCHDOG_DUMP_SECS=5 ./test.exe
```

**Triggered:** automatically by `nova_supervised_run_impl` when scope
wait exceeds threshold. Single dump per scope (idempotent).

**Output:**
```
=== NOVA_RUNTIME_DUMP === reason=supervised-watchdog-5s-remote-3
[globals] n_workers=N driver_started=1 armed=1 materialized=1
[worker 0] runnext=... wake_pending=... preempt_flag=... stop=...
  [w.0.scope] count=N cancel_req=B pending_remote=N
  [w.0.parked  cap=64] 11010000...
  [w.0.pwake   cap=64] 00000000...
  [w.0.fiber.sI] co=... mco_status=N parked=1 pwake=0 ...
[supervised] scope=... count=N pending_remote=N cancel_req=B armed_sleeps_head=...
[supervised.summary] slots=N alive=N dead=N null=N
[supervised.armed.I] st=... scope=... slot=N
=== END DUMP ===
```

**Reading the dump:**
- `pending_remote=N > 0` but no live fibers → lost-wake somewhere
- `fibers[i] = NULL` AND `parked[i] = true` → STALE-slot race (§10)
- `cancel_req=true` AND `pending_remote > 0` after cancel → cancel
  didn't propagate to one worker
- `armed_sleeps_head ≠ NULL` after no sleep was issued → driver state
  corruption OR scope ptr is stale (§12.31)

### 3.3 Stress harness `scripts/stress_bisect.sh`

**Usage:**
```bash
scripts/stress_bisect.sh nova_tests/<plan>/<repro>.nv [stress_n=15]
```

**Exit codes** (git-bisect compatible):
- `0` = all `stress_n` runs PASS (GOOD)
- `1` = at least one run FAIL (BAD)
- `125` = build failure (SKIP)

**Built-in stats:** prints `[stress-bisect] PASS=X FAIL=Y / N`.

**⚠ ALWAYS use this for stress — NEVER loop `nova test` in a shell `for`.**
`nova test <file>` is the suite runner: each invocation recompiles the WHOLE
runtime C (~10 .c files, ~16 parallel clang) from scratch — there is no .o
cache across invocations. Looping it N times = N full runtime recompiles
(~5-10 s each → 100 runs ≈ 15-25 min, and it *looks* like a hang). By contrast
`stress_bisect.sh` runs `nova test --keep-artifacts` **ONCE** to build the test
`.exe`, then re-runs that `.exe` N times directly (each run = the test's own
~tens-of-ms runtime). 100 runs ≈ seconds. (Lesson from Plan 83-go-cmn Ф.1b
closure: a `nova test` stress loop was mistaken for a runtime hang — it was just
the recompile cost.)

**Runs the `.exe` ARMED.** Because it executes the raw `.exe` directly (not via
`nova test`), it does NOT honour a fixture's `// ENV NOVA_AUTOARM=0` directive —
the exe runs in armed M:N (default). This is exactly what you want for verifying
that an AUTOARM=0-gated fixture is safe to un-gate (run it armed N times; if
PASS=N, the gate can go). It also means a fixture with its OWN non-atomic shared
mutation (`x += 1` from parallel fibers) will FAIL here for a test-level data
race, NOT a runtime bug — distinguish the two before blaming the runtime.

**No per-run timeout.** The exe is run directly, so a genuine runtime HANG
(lost-wake) blocks the iteration forever. Run under an outer timeout (Bash tool
`timeout`, or `timeout 300 bash scripts/stress_bisect.sh ...`) so a hang is
caught instead of wedging the harness.

**Stress count selection:**
```
n = ceil(-ln(0.01) / p)
```
Where `p` is observed repro rate. Default 15 is fine at `p ≥ 0.30`;
bump up for rarer races. For a near-DETERMINISTIC pre-fix hang (repro ≈100%),
n=30-100 clean is strong closure evidence.

### 3.4 Bisect over commit range

**Setup:**
```bash
cd <worktree>
git bisect start
git bisect bad <bad-sha-or-HEAD>
git bisect good <good-sha>
```

**Verify graph FIRST** (§12.29 lesson #13):
```bash
git log --graph --oneline good..bad | head -30
```
If you see unexpected branches/merges, your assumption about topology
is wrong — fix the range before bisecting.

**Run:**
```bash
git bisect run scripts/stress_bisect.sh nova_tests/<plan>/<repro>.nv 66
```
(`66` instead of default `15` for any race with `p < 0.20`.)

**Convergence:** `log2(N)` iterations for `N` commits.

**Sub-bisect within identified commit:** if first-bad commit is large
(>100 lines), CANNOT reliably identify the offending hunk via revert-
sub-bisect — interaction between hunks may compensate (§12.27 lesson
#10). Need dynamic analysis (state-dump under sub-bisect SHA), not
just removal.

### 3.5 WSL Linux as comparison environment

Plan 83.11 §12.21 BREAKTHROUGH.

**Setup (one-time):**
```bash
wsl --install -d Ubuntu   # if not already installed
wsl -d Ubuntu
# Inside WSL:
sudo apt install gdb clang libgc-dev libuv1-dev
# Mirror nova worktree to /root/nova for Linux-native fs speed
# (NOT mount via /mnt/d — fast for cargo build, slow for tests)
```

**Quick comparison:**
```bash
wsl -d Ubuntu bash -c "cd /root/nova && nova test nova_tests/<repro>.nv"
```

**If Linux passes 30/30 + Windows fails 30/30 → Windows-only bug.**

---

## §4. Twenty lessons consolidated

Numbered 1-20 in chronological order across §10.4 + §12.16 + §12.31.
Grouped by theme for navigability.

### Group A: Heisenbug and instrumentation discipline

**1. Heisenbug = memory ordering.** If `fprintf` fixes a race, it's a
memory-fence artifact, never a real fix. `stdio` lock implies `mfence`
which masks weak-ordering bugs. **Rule:** never declare a race
«fixed» while diagnostic printf is in the critical path.

**2. Counter atomicity must be paired with the operation they count.**
Diagnostic counters incremented separately from the operation they
measure produce misleading data. Plan 83.11 session 5 spent ~2 hours
debugging «pending_remote leak» that didn't exist — diagnostic counters
were not atomically paired.

**20. Differentiate Heisen-tests vs point-probes.** Heisen-test = broad
instrumentation across hot path (anti-pattern). Point-probe = single
fprintf at function entry in cold path showing correctness of heap
value (valuable diagnostic). Distinguish carefully.

### Group B: Diagnostic primacy

**3. State-dump > step-debugger.** At M:N concurrency with 16 OS
threads × N fibers, single-step doesn't reproduce race (Heisenberg
uncertainty). Programmatic lock-free state-dump shows full picture
per dump. ~80 LOC infrastructure + `NOVA_WATCHDOG_DUMP_SECS=5` env var.

**16. VEH + native SymFromAddr beats absent cdb on Windows.**
`dbghelp.dll` is on every Windows install. `dbghelp.lib` already linked.
~165 LOC `segv_diag.c` + one hook = full symbolized stack on SEGV.
Add to default diagnostic kit. **llvm-symbolizer without DIA is useless
for PDB** (returns `??:0:0`).

**17. One diagnostic print > eight hypothesis attempts.** Plan 83.11
§12.31 prior 7 attempts iterated in wrong region (GC scanning, fiber
stack coverage, ABI) because no ground-truth from debugger. Frame[1]
on first VEH run took 1 minute and revealed answer that 10h of
hypothesis-driven attempts missed. **Invest one full session in proper
diagnostic tooling before iterating fixes.** ROI is insane.

### Group C: Identity through pointer, not slot

**4. `expected_co` as identity field.** `close_cb` must know WHICH
fiber it serves, not just WHERE (slot index). Capture `mco_coro*` at
arm time; compare at close time. Slot index is mutable (can be
reused); pointer is identity.

**18. Compound-literal symptoms can be misdirection.** §12.9
«memcpy to .rdata» symptom was misleading: x86 `LOCK CMPXCHG` on
read-only page faults regardless of compare result (page-writability
check happens on the LOCK cycle, not after compare). llvm-symbolizer
without DIA misclassified `nova_aint_cas` as `memcpy`. 7 sessions
followed the false framing. **Don't trust llvm-symbolizer for MSVC
PDB; use Windows-native dbghelp via SymFromAddr.**

### Group D: Slot and scope ownership

**5. Slot ownership = NULL + parked + pending_wake.** `fibers[i] = NULL`
does NOT mean «slot is free». A slot is free when:
```
fibers[i] == NULL  AND  parked[i] == false  AND  pending_wake[i] == 0
```
Three conditions, not one. STALE-slot race fixed by checking all three
in `nova_scope_alloc_slot` (Plan 83.11 §10 Fix A).

**19. Stack-allocated scope must outlive all async references.**
General invariant any time stack pointer crosses thread/queue boundary.
Plan 83.11 Ф.2 introduced driver thread + Ф.3 added CANCEL_SCOPE jobs
carrying scope ptr — lifetime invariant was **NOT audited** at design
time. Bug fired 4 days later (§12.31).

**Pattern for any future plan:** any time a job/queue/channel contains
a pointer to caller's local variable, caller MUST wait for the
asynchronous consumer to finish dereferencing before returning.
Counter-based wait (the `pending_driver_jobs` pattern from §12.31)
is the canonical implementation. See D228 §6.

### Group E: Bisect methodology

**6. ~10 hours on one race is normal for M:N without debugger on
Windows.** Tokio spent ~3 years on equivalent class of races
(2017→2020 1.0 release). Plan 83.11 closed it in ~2 weeks of intensive
work. Don't beat yourself up if a race takes a session or two — that's
the going rate.

**8. Bisect over hypothesized region FIRST.** Plan 83.11 §12.21 spent
3+ hours diagnosing via WSL on the wrong commit region (184 main
commits). 20-min bisect would have identified the real culprit
instantly. **Always start bisect at first opportunity when you have a
clear GOOD/BAD bracket.**

**9. «Verified clean» needs explicit scope.** Plan 83.11 Ф.3 memory
said «verified 30/30 stress PASS» — that was for `stress_iso_3e`
specifically (which Ф.3 fixed). Not all tests. `cancel_semantics_test`
was also passing in single nova test run, but bug was latent
(stochastic 7% PASS rate for `_min.nv`) — only stress reveals.
**Mark verification scope explicitly:** «verified X under stress Y;
NOT verified for Z, W».

**10. Sub-bisect surgical reverts can be misleading.** If a commit
contains multiple interacting fixes, reverting one may not eliminate
the bug because other fixes' effects compensate. Need either to revert
ALL or to study interaction via state-dump under the sub-bisect SHA.

**13. Verify the branch graph before bisecting.** §12.28 misread branch
topology (assumed plan-83.11 NOT merged to main when it WAS via
`cf00a37fb16`) and wasted a session. Always
`git log --graph --oneline good..bad` BEFORE interpreting bisect
results.

**14. Stress n=15 is insufficient at low p.** Initial Plan 83.11 §12.28
bisect false-positive because `P(false-good)` at `p=7%` with `n=15` ≈
34%. **Heuristic for bisect:** `n ≥ ⌈-ln(0.01)/p⌉`. At `p=0.20`:
`n=23`; at `p=0.07`: `n=66`. **Default n=15 too low for stochastic
races.**

**15. Overlapping confidence intervals = no signal.** Always check CI
overlap before claiming partial signal. `5/30 PASS vs 0/10 PASS` =
same distribution likely (CIs `[5%, 36%]` vs `[0%, 26%]` overlap).
Don't claim recovery without statistical separation.

### Group F: Anti-patterns proven across sessions

**7. «Strict subset of existing coverage» trap.** Before implementing
any fix that adds GC root coverage, audit existing coverage. Plan 82's
walker already had what §12.23 proposed. Implementation became 80 lines
of code for measurably partial result. **Always audit before
implementing.**

**11. Partial test result IS a result.** §12.28 Test 1 gave 17%
recovery — not a «fix» but valuable elimination evidence. Don't
dismiss partial results as failures; document the rate change
explicitly with statistical CI.

**12. Background rate matters.** Bug rate changes across the codebase
evolution (e.g. 40% pre-merge → 100% post-merge) tells you about
interaction between identified culprit and other code. Use rate
changes diagnostically — they reveal where regressions interact.

---

## §5. Anti-patterns (what NOT to do)

Each anti-pattern below has burned ≥1 session in Plan 83.11. Repeat at
your peril.

### 5.1 Hypothesis-driven fix iteration without debugger

Plan 83.11 §12.13-26 burned ~10 hours across 7 attempts that all got
REVERTED because no one had ground-truth from a debugger. Each attempt
gave «partial recovery» (4/30, 21/30, 25/30 PASS) but no real fix.

**Rule:** if your first 2 attempts don't work, STOP iterating. Invest
the next session installing proper diagnostic. ROI guaranteed.

### 5.2 Heisen-tests (broad instrumentation)

Adding `fprintf` to every step of the failing code path. Each `fprintf`
implies a stdio lock + mfence, which serializes memory operations and
masks weak-ordering bugs.

**Rule:** instrumentation = ONE fprintf in cold path at function entry,
showing actual heap value. NOT control-flow tracing.

### 5.3 Trusting llvm-symbolizer for MSVC PDB

Most LLVM builds (including VS BuildTools' bundled llvm-symbolizer)
are built **without DIA support**. They return `??:0:0` for any PDB
lookup. Even worse: their fallback misclassifies `lock cmpxchg` as
`memcpy`, wasting sessions on the wrong code path (§12.31 §12.9
misdirection).

**Rule:** for MSVC PDB on Windows, use Windows-native `dbghelp` via
`SymFromAddr`. dbghelp.dll is on every Windows; dbghelp.lib is already
linked in `test_runner.rs`.

### 5.4 Bumping back test loads to «make CI green» instead of fixing root cause

Plan 83.11 §13.3 reduced `fibers_10k_sleep_cancel` 10k → 2k fibers to
surface only the Plan 110.x cleanup-shield bug, not Plan 83.11 itself.
This is OK because:
- Reduction is **documented** in test header
- Reason is **orthogonal subsystem bug** with separate marker
- Bump-back path is **explicitly documented** for once the upstream
  bug is fixed

**Rule:** load reduction is acceptable ONLY when:
1. Test header documents the reduction + bump-back path
2. Reduction is to expose a different, orthogonal bug
3. New marker is filed for the original-scale bug

If you reduce loads to «make CI pass» without these three: you're
hiding the bug.

### 5.5 Background-rate sampling without statistical significance check

5/30 PASS vs 0/10 PASS looks like recovery, but CIs `[5-36%]` vs
`[0-26%]` overlap — it's noise. Always compute CI overlap before
claiming partial signal.

**Rule:** at minimum, both samples must have same `n`, and the new
rate must be ≥2σ above the baseline rate. Otherwise: noise.

### 5.6 Sub-bisecting a fix commit via surgical reverts

When the first-bad commit contains multiple interacting fixes (Fix
A+B+C pattern), reverting one may not eliminate the bug because the
others compensate. §12.27 sub-bisect dropped each of Fix A / Fix B /
Fix C and bug still fired in all three sub-runs.

**Rule:** sub-bisect identifies coarse commit; understanding the
specific cause requires dynamic analysis (state-dump under sub-bisect
SHA), not just removal.

### 5.7 Adding more GC coverage as a fix for «GC missing fiber stacks»

Plan 83.11 §12.22-23 attempts (per-fiber `GC_add_roots`, GC_push_other_roots
callback) made GC strictly more coverage, yet PASS rate plateaued at
~21/30. The bug wasn't fiber-stack coverage at all — it was scope
lifetime (§12.31). Even Attempt 6 (broadest possible coverage) gave
+726 TIMEOUT regression in full suite because `GC_add_roots` is O(N)
per call → O(N²) for N fibers in suite.

**Rule:** if «more coverage» doesn't reach 30/30 PASS, the bug is NOT
about coverage. Stop adding coverage; look elsewhere.

### 5.8 «I'll just amend my last commit» on a pushed branch

Per CLAUDE.md: never amend pushed commits. Always create a new commit.
Rebasing/amending pushed history corrupts everyone else's local
checkout.

---

## §6. Case studies (deep dives)

### 6.1 Plan 83.11 Ф.3 — STALE-slot race (10h, 6 sessions, 2026-05-27-28)

**Final fix:** Fix A (alloc_slot skips STALE) + Fix B (close_cb direct
dispatch via `expected_co`) + Fix C (DISPLACED sentinel).

**Diagnostic that broke the case:** programmatic state-dump (~80 LOC)
revealed `fibers[5] = NULL AND parked[5] = true` — impossible state
pointing at alloc_slot bug.

**Full article:** [`nova-private/docs/articles/mn-race-stale-slot.md`](../../../nova-private/docs/articles/mn-race-stale-slot.md)
Часть I (~666 lines).

**Plan doc:** [`docs/plans/83.11-centralized-io-driver.md`](plans/83.11-centralized-io-driver.md) §10 (post-mortem).

### 6.2 Plan 83.11 §12.31 — use-after-free stack scope (10h prior + 30min fix, 2026-06-01)

**Final fix:** `pending_driver_jobs` lifetime counter on
`NovaFiberQueue` — main waits for driver to drain in-flight jobs
before returning from `supervised_run_impl`.

**Diagnostic that broke the case:** in-process VEH + dbghelp (~165 LOC
`segv_diag.c`). Frame[1] localized on FIRST run.

**Full article:** [`nova-private/docs/articles/mn-race-stale-slot.md`](../../../nova-private/docs/articles/mn-race-stale-slot.md)
Часть II (~605 lines).

**Plan doc:** [`docs/plans/83.11-centralized-io-driver.md`](plans/83.11-centralized-io-driver.md) §12.31.

**Spec canonical pattern:** [`spec/decisions/06-concurrency.md`](../spec/decisions/06-concurrency.md) D228 §6.

### 6.3 Plan 83.11 §11.6 — GC structural aliasing (10min fix, was 4-day-old design, 2026-06-01)

**Final fix:** `nova_scope_pin_ctx(&scope, tok)` one-line emit in
`emit_supervised` codegen — pins cancel token in `ctx_pins[]` so it
remains GC-reachable through scope's stack frame.

**Diagnostic:** pre-designed in §11.4 Option A four days before
implementation. Simply applied.

**Spec canonical pattern:** [`spec/decisions/06-concurrency.md`](../spec/decisions/06-concurrency.md) D228 §7.

**Lesson reinforced:** when fix is documented in plan doc, **try it
first** before iterating new hypotheses.

---

## §7. Quick-reference command list

```bash
# === Setup ===
export NOVA_GC_LIB_DIR="D:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/lib"
export NOVA_GC_INCLUDE_DIR="D:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/include"

# === Repro via stress harness ===
bash scripts/stress_bisect.sh nova_tests/<plan>/<repro>.nv 30

# === Diagnostic: SEGV / AV on Windows ===
NOVA_DIAG_SEGV=1 ./nova-cli/target/release/nova.exe test nova_tests/<plan>/<repro>.nv --keep-artifacts
# Find the built exe and run direct for full stderr:
EXE=$(find /tmp/nova_tests -name "<test_basename>.exe" -mmin -2 | head -1)
NOVA_DIAG_SEGV=1 "$EXE" 2>&1 | tail -50

# === Diagnostic: hang / TIMEOUT ===
NOVA_WATCHDOG_DUMP_SECS=5 ./nova-cli/target/release/nova.exe test nova_tests/<plan>/<repro>.nv

# === Bisect over commit range ===
git bisect start
git bisect bad HEAD
git bisect good <known-good-sha>
git log --graph --oneline good..bad | head -30   # verify topology FIRST
git bisect run scripts/stress_bisect.sh nova_tests/<plan>/<repro>.nv 66

# === WSL Linux comparison ===
wsl -d Ubuntu bash -c "cd /root/nova && nova test nova_tests/<plan>/<repro>.nv"

# === Static symbolization (if you have a known RIP RVA) ===
DUMPBIN="/c/Program Files (x86)/Microsoft Visual Studio/18/BuildTools/VC/Tools/MSVC/14.50.35717/bin/Hostx64/x64/dumpbin.exe"
MSYS_NO_PATHCONV=1 "$DUMPBIN" -HEADERS /tmp/nova_tests/<dir>/<test>.exe | grep "image base\|.text\|.rdata"
# Compute RVA = RIP - image_base, then grep symbol map for nearby symbols
```

---

## §8. When to update this playbook

Update this document when:

1. **A new race investigation completes** with a novel diagnostic
   pattern. Add it as a new lesson, anti-pattern, or tool section.
2. **A new tool is added to the codebase** (e.g. new `*_diag.c` file).
   Add to §3 Tooling reference.
3. **A wrong-tool-choice burned a session** — add to §5 Anti-patterns
   with brief case-study link.

Updates follow the standard plan-closure pattern (see CLAUDE.md): plan
doc closure → playbook update → logs entries.

---

## §9. Related documents

- **Plan 83.11 plan-doc:** [docs/plans/83.11-centralized-io-driver.md](plans/83.11-centralized-io-driver.md)
- **Cancellation case-study article:** [nova-private/docs/articles/mn-race-stale-slot.md](../../../nova-private/docs/articles/mn-race-stale-slot.md)
- **Spec D228 (canonical patterns):** [spec/decisions/06-concurrency.md](../spec/decisions/06-concurrency.md) §D228
- **Tools:**
  - [scripts/stress_bisect.sh](../scripts/stress_bisect.sh) — stress + bisect harness
  - [scripts/cdb_session.sh](../scripts/cdb_session.sh) — optional cdb wrapper
  - [compiler-codegen/nova_rt/segv_diag.c](../compiler-codegen/nova_rt/segv_diag.c) — VEH crash localizer
  - `nova_runtime_dump_state` in [compiler-codegen/nova_rt/runtime.c](../compiler-codegen/nova_rt/runtime.c) — state-dump

### 6.4 Plan 139.2 — block-expr value-type mis-inference (deterministic SEGV, ~15 min, 2026-06-12)

**Symptom:** deterministic `EXCEPTION_ACCESS_VIOLATION` (READ @0x26) on any bare
block-expression-as-value (`ro v = { ro a=10; ro b=20; a+b }; assert(v == 30)`).
Surfaced in `basics/control_flow` only AFTER Plan 139.2's binary; PASS on the prior binary.

**Diagnostic that broke the case:** `NOVA_DIAG_SEGV=1` (§3.1). Frame[1] =
`Vec____nova_byte_method_equal` in the test body localized it in ~1 minute — the `int` `==`
was dispatching to `Vec[u8]@equal`, i.e. the block-expr value `v` was mis-typed as a Vec view.

**Root cause:** `var_types` (codegen local-type map) is NOT per-fn scoped. Plan 139.2 made str
methods Nova-body with `Vec[u8]` locals (`a`/`b`); those leaked into later functions. The
block-expr inferred its value-type from the trailing expr BEFORE emitting its own body, so an
identifier in the trailing resolved against the stale leaked entry, and `BinOp::Add` with a
C-pointer left operand was read as pointer arithmetic → the whole block typed as `Vec____nova_byte*`.

**Fix:** `emit_block_expr` + `infer_expr_c_type` Block arm pre-register the block's own let-binding
types before inferring the trailing type (shadowing the stale entry).

**Lessons reinforced:** (a) **Lesson #17** — one SEGV-diag run beat an hour of `.c`-diff tooling
that fought Windows artifact paths; invest in the diagnostic. (b) **NEW: per-plan regression
sweeps must include `basics`.** Plan 139.2's sweep omitted `basics` and shipped the SEGV; an
**independent broad verification before merge** caught it. Make `basics` + a wide spread mandatory
in any codegen-touching plan's regression gate, and re-verify independently before merging to main.
