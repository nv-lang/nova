# The D180 consume obligation is skipped when the constructor is a static named anything but `new`

Run 2026-09-14 23:53-23:55, `nova.sh check`, one file per run.

| form | binding | verdict | correct? |
|---|---|---|---|
| `control_consume` | `consume t = Secret.new(...)`, then `t.into_secret()` | **PASS** | yes |
| `control_plain_of` | `ro p = Plain.of(7)` on a NON-consume type | **PASS** | yes |
| `via_new` | `ro t = Secret.new(...)` | **`E_CONSUME_KEYWORD_MISSING`** | yes |
| `via_free_fn` | `ro t = build_secret(...)` | **`E_CONSUME_KEYWORD_MISSING`** | yes |
| `via_of` | `ro t = Secret.of(...)` | **PASS** | **NO -- the check is skipped** |
| `via_make` | `ro t = Secret.make(...)` | **PASS** | **NO -- the check is skipped** |

All four constructors have byte-identical bodies (`=> { secret }`) and the same return
type. The only difference between a form that is checked and a form that is not is the
NAME of the function.

## How it was found, which matters more than the probe

Not by looking. Task T2.28 renamed 223 non-variadic `X.of(` to `X.new(` across `src/`
because the linter's `W_NONVARIADIC_OF` said to. One line then stopped compiling:

```
src/accounts/login_test.nv:88:5: error: [E_CONSUME_KEYWORD_MISSING]
    binding `t` holds a consume-required instance of type `Secret` -- D180
    ro t = Secret.new("sk-ant-fixture-SECRET")
```

The line was not edited. `ro t = Secret.of(...)` had compiled for weeks. **A rename that
was supposed to change nothing changed whether a correctness check ran at all**, and
the type it was skipped on is the one that holds an API token.

## The axis, and the control that makes the alternative impossible

The claim is "the check keys on the constructor's name", not "the check is weak around
statics" and not "`.of` confuses the checker". Three controls separate those:

* `via_free_fn` errors, so it is not "only methods are checked".
* `via_new` errors, so the checker CAN see the obligation through a static.
* `control_plain_of` passes and its value is usable (`p.n + 1`), so `.of` does not
  return something untyped -- what it loses is specifically the ownership obligation.

`via_make` matters as much as `via_of`: with only `of` measured, the reading would have
been "the name `of` is special" (plausible -- it IS special to the linter). A third,
neutral name failing the same way says the rule is "anything but `new`".

## Two refusals to judge, both from the control

1. The first `Secret` had no consume-method, and `D133-empty-consume` reddened **all
   five** forms. A probe read at that moment says "nothing passes" and means nothing.
2. The fixed control still failed -- `D133-not-consumed` -- because it bound the value
   with `consume` but never consumed it. Only after `t.into_secret()` did the control
   pass and the other verdicts become readable.

Both were caught by running the control first and requiring it to pass.

## What it costs today

Every `consume` type built through a static not named `new` loses the guarantee it was
declared for. In this tree that was `Secret` -- the bearer token -- from 2026-09-08
until the rename. No test would have found it: the tests were green, and green is what
the missing check produces.
