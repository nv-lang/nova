#!/bin/sh
# p8: novac_oracle_key - what changes the oracle's fixture binary but not the key.
# The oracle (cwd = root, smoke line 52) takes std from: env NOVA_STD_PATH,
# else root nova.toml key `std = "..."` (compiler-codegen/src/manifest.rs:1623,
# resolve_std_root_no_env), else root/std; runtime from env NOVA_RT_DIR /
# NOVA_CG_INCLUDE (nova-cli/src/main.rs RepoPaths). The key hashes only
# HEAD:compiler-codegen, HEAD:std, their dirty diff/untracked files, oracle, clang.
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="${NOVA_REPO_ROOT:-$(git -C "$HERE" rev-parse --show-toplevel)}"
W="$HERE/work"; rm -rf "$W"; mkdir -p "$W"
. "$ROOT/scripts/guards/lib/novac.sh" || exit 2
R="$W/repo"; mkdir -p "$R/compiler-codegen/nova_rt" "$R/std/src" "$R/std2/src"
echo 'int rt;' > "$R/compiler-codegen/nova_rt/rt.c"
echo 'fn a() -> Int => 1' > "$R/std/src/a.nv"
echo 'fn a() -> Int => 2' > "$R/std2/src/a.nv"
printf '[workspace]\nmembers = ["std"]\n' > "$R/nova.toml"
printf '*.a\n' > "$R/.gitignore"
echo oracle-bytes > "$W/oracle"; printf '#!/bin/sh\necho "clang version 99.0.0"\n' > "$W/clang"; chmod +x "$W/clang"
git -C "$R" init -q && git -C "$R" add -A && git -C "$R" -c user.email=p@p -c user.name=p commit -qm init || exit 2
k() { novac_oracle_key "$R" "$W/oracle" "$W/clang"; echo " rc=$?"; }
echo "K0 clean:                                   $(k)"
printf '[workspace]\nmembers = ["std"]\nstd = "std2"\n' > "$R/nova.toml"
echo "K1 root nova.toml now says std = std2:      $(k)"
git -C "$R" checkout -q -- nova.toml
echo "K2 NOVA_STD_PATH=std2 in env:               $(NOVA_STD_PATH=std2 k)"
echo "K3 NOVA_RT_DIR=/elsewhere in env:           $(NOVA_RT_DIR=/elsewhere k)"
echo 'archive' > "$R/compiler-codegen/nova_rt/libstale.a"
echo "K4 gitignored compiler-codegen/nova_rt/libstale.a appears: $(k)"
rm -f "$R/compiler-codegen/nova_rt/libstale.a"
echo 'fn a() -> Int => 3' > "$R/std/src/a.nv"
echo "K5 control: tracked std/src/a.nv edited:    $(k)"
git -C "$R" checkout -q -- std/src/a.nv
echo "K6 clang path that does not exist:          $(novac_oracle_key "$R" "$W/oracle" "$W/no-such-clang"; echo " rc=$?")"
mkdir -p "$W/nogit/compiler-codegen" "$W/nogit/std"
echo "K7 root outside git:                        [$(GIT_CEILING_DIRECTORIES="$W" novac_oracle_key "$W/nogit" "$W/oracle" "$W/clang"; echo " rc=$?")]"
