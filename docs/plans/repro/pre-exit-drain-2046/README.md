# Twins for #2046 — pre-exit orphan-drain hang (candidate 0.1.8 regression, Linux)

Carrier: wide `hub_test` run (canopus, selectel) passes all checks, then
hangs in `nova_supervised_drain_main_scope`; dump
`pre-exit-drain-10s-remote-4` (/tmp/canopus-329-wide-quiet-2workers.log on
selectel): orphan `cancel_req=1 pending_remote=4`, 4 detached fibers with
NESTED supervised scopes (their children live in worker scope slots,
parent_scope pointing into the session fibers' stacks), all `parked=0
hdl=(nil) stop_cb=(nil)`, two of them `mco_status=2` (MCO_RUNNING), both
workers otherwise idle — runnable/spinning fibers, NOT process waits.

Linux runtime delta oracle-v0.1.7..0.1.8 is EXACTLY ONE commit:
`af22bc6ee` (#2038, never signal a nonpositive pid/pgid). Each of its four
hunks was re-audited against the dump shape; none can produce
READY/RUNNING fibers with no pending handles (details in the #2046 report).
Leading hypothesis: the #2038 fix removed the `kill(0, SIGKILL)` group
suicide that on <= 0.1.7 terminated the process at/near pre-exit and MASKED
a pre-existing stuck-fiber shape (#791 / item-19-root-2 class). The 0.1.7
A/B of the wide run (pending on the canopus side) decides.

## Twins (all built with `nova build <t>.nv --toolchain clang`)

| twin | shape | Windows 0.1.8 | Windows 0.1.7 |
|---|---|---|---|
| t2_detach_sleep | 1 detached sleeping fiber | drains, exit 0 | — |
| t3_detach4_sleep | 4 detached sleeping fibers | drains, exit 0 | — |
| t4_nested_chan_waiter | detached fiber + nested supervised + channel waiter that never fires | drains, exit 0 | — |
| t5_nested_sleep_loop | detached fiber + nested long sleep-loop child | drains, exit 0 | — |
| t6_detached_treegroup_agents | 4 detached sessions, each waiting on a 20 s Tree.Group child (closest to the carrier: agents killed by the pre-exit cancel) | drains, exit 0 | — |

All GREEN on Windows 0.1.8 — the hang needs something the hub does beyond
these shapes (real spin loops without safepoints, or Linux-only paths).
Run the SAME twins on selectel against 0.1.7 and 0.1.8 toolchains:
t6 hanging on 0.1.8-Linux but not 0.1.7-Linux would re-point the root at
the process layer; t6 clean on both confirms the hub-specific shape and
the masking hypothesis.

## Incidental finding (separate defect, reported in the #2046 report)

`detach consume id = i { ... }` inside a `while` loop miscompiles into a
runaway: the loop runs ~156k iterations (ids climb far past the bound) and
either spins forever or dies with "fiber_arena exhausted". Repro:
`detach consume id = i { println("d ${id}") }` in `while i < 4`. The form
is the one E_CONCURRENT_MUT_CAPTURE itself suggests. Both 0.1.7 and 0.1.8
toolchains affected on Windows.
