# How to run

**First, restore the suffixes.** Every source here is stored as `*.nv.txt` and the
manifest as `nova.toml.txt` -- rule #695 (`docs/plans/repro/README.md`): evidence must
REPRODUCE a defect and is therefore often red, so it is kept out of the reach of the
runner, the lint and every guard that walks `**/*.nv`. Nothing below runs until the
names are restored into a scratch copy:

    cp -r <this dir> /tmp/probe1106 && cd /tmp/probe1106
    mv nova.toml.txt nova.toml
    for f in src/*.nv.txt; do mv "$f" "${f%.txt}"; done

Copy rather than rename in place: renaming here puts red evidence back under the
guards, which is exactly what the suffix prevents.


    cd probes/consume-check-keys-on-name
    ../../nova.sh check src/control_consume.nv   # must PASS, or nothing else counts
    ../../nova.sh check src/via_new.nv
    ../../nova.sh check src/via_of.nv
    ../../nova.sh check src/via_make.nv
    ../../nova.sh check src/via_free_fn.nv

`check` is the verdict: `E_CONSUME_KEYWORD_MISSING` is a checker diagnostic.

## Reading the outcomes

An ERROR is the language working. A PASS on any of `via_*` is the defect, because all
four build the same `consume`-required value and bind it with plain `ro`.

| pattern | reading |
|---|---|
| all four error | no defect; something else explains the claude-limits case, and I must find it before filing anything |
| only `of` passes | the check keys on the name `of` specifically |
| `of` and `make` pass, `new` errors | the check recognises only `new` as a constructor -- the widest and worst case |
| the free function also passes | it is not about statics at all |
