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

- `binds.nv`: `8901f52ef98e61d5bd83e986edea162de0a778f4b149d92e587b789287b97388` == the branch's novac/src/check file (byte-identical modulo line endings; the committed blob is LF)
- `calls.nv`: `cd7ecbedd037f080453da62501c89296154c65b7bc84d5683d075a5458c3152e` -- base 3729abcc + task-130 hunks only (no task-92 host_os bridge)
- `const_types.nv`: `0e7b75ef23a69bf1e89dc3044b907fa021498b0d34d58e0dd192dc7eb4d957e2` == the branch's novac/src/check file (byte-identical modulo line endings; the committed blob is LF)
- `exprs.nv`: `010d50a147bc06ad076586ac0d111c931f0def0d48bb79fc6ca1b75142d9b023` == the branch's novac/src/check file (byte-identical modulo line endings; the committed blob is LF)
- `module_values.nv`: `18c74b770903c54abe58fddf43344719bcf67fba575d7874a3fe6821b4bcb5a7` == the branch's novac/src/check file (byte-identical modulo line endings; the committed blob is LF)
- `option_rules.nv`: `7ea9694aabdfc7039a44773804349f3fd039e62d85d81fd43056f0b908de6de6` == the branch's novac/src/check file (byte-identical modulo line endings; the committed blob is LF)
- `type_of.nv`: `0472a20f9ffb8d05b2433d8cd88ed4c2c9c3fbc79f00ccd34dd2c0c694612724` == the branch's novac/src/check file (byte-identical modulo line endings; the committed blob is LF)
