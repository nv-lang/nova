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

- `binds.nv`: `5ac4dc742887b96a87a68d3867211e6b768b81e1222e0dbe6148fb5afd8508ac` == the branch's novac/src/check file (byte-identical modulo line endings; the committed blob is LF)
- `calls.nv`: `5d5afe71241429758aec270b1f168cbc505bc66087f61cc41173eed9cfd10094` -- base 3729abcc + task-130 hunks only (no task-92 host_os bridge)
- `const_types.nv`: `52d07e097865109d313897a9dc9281fbc3b7c529b0c585022f1300fbe7136ea1` == the branch's novac/src/check file (byte-identical modulo line endings; the committed blob is LF)
- `ctor_calls.nv`: `03e256e34be0db60626cc5d6ac23e237d208ec3dff212f611af21fef30c77da2` == the branch's novac/src/check file (byte-identical modulo line endings; the committed blob is LF)
- `exprs.nv`: `cbf977c78807af7bc60dedc9501d71a0ba2e6486793875b8af8ef89f6ca6d628` == the branch's novac/src/check file (byte-identical modulo line endings; the committed blob is LF)
- `module_values.nv`: `a84170b5e839c978d6c1c3beed48f79a8f64523a96d9cb1df00228b0d2043ae6` == the branch's novac/src/check file (byte-identical modulo line endings; the committed blob is LF)
- `option_rules.nv`: `259c8d271e0c42e322474702c0a4e1562118064a90812857c22b5482a002e2fc` == the branch's novac/src/check file (byte-identical modulo line endings; the committed blob is LF)
- `type_of.nv`: `0472a20f9ffb8d05b2433d8cd88ed4c2c9c3fbc79f00ccd34dd2c0c694612724` == the branch's novac/src/check file (byte-identical modulo line endings; the committed blob is LF)
