# Seed-backport manifest (task 130)

Exact contents of the seven `novac/src/check/` files laid over the seed base
`3729abcce42ec2d94efc898ec6eafe91ff6210d5` to build the fixed seed binary.
Six of them are byte-identical to commit `efb2af8c4`; `calls.nv` is the base
version plus ONLY the task-130 hunks (no task-92 `is_host_os_bridge` bridge:
the base's sem/ does not declare it).

Reproduction (no worktree edits, no scratch driver):

    git archive <BASE> novac std | tar -x        # base tree
    cp seed-backport/check/*.nv tree/novac/src/check/
    NOVA_STD_PATH=tree/std/src NOVAC_SELF_PATH=tree/novac/src \
        <pinned B1>/novac.exe emit tree/novac/src/main.nv > fixed.c
    # C sha256: see repro.log; compile via nova-driver-support
    # native_build::compile_c_to_exe (the nova build door) -> novac.exe

## SHA-256

- `binds.nv`: `8ed1438eacbeef1f3da8eab2572b821561e5aaa2f3756ca7d9c8176a791924af` == commit efb2af8c4 (byte-identical)
- `calls.nv`: `cd7ecbedd037f080453da62501c89296154c65b7bc84d5683d075a5458c3152e` -- base 3729abcc + task-130 hunks only (no task-92 host_os bridge)
- `const_types.nv`: `3d1a4937c07db46b877658b52056d651ceca6e5d165bf4c1c550bfa2df3c9f9b` == commit efb2af8c4 (byte-identical)
- `exprs.nv`: `195547c2c947a8c6519e313254e0c9849a2e8c5d779b98f6d123f4175963462e` == commit efb2af8c4 (byte-identical)
- `module_values.nv`: `56b8de814fab96217d3194a67ce750e9a6635d9d4cc5d73cccc11714a8bf1a41` == commit efb2af8c4 (byte-identical)
- `option_rules.nv`: `64b818d03c1da8c7afbfd1e68432dd68826fa520b6c0051aced56360d7de0ece` == commit efb2af8c4 (byte-identical)
- `type_of.nv`: `0435ef7cb4aed3b32a91bfb0209f35d64b7c22015f1f56659d3c22284900f162` == commit efb2af8c4 (byte-identical)
