#!/bin/sh
# usage: sh probe.sh <path-to-nova.exe> [project-dir]
# The commands must run with cwd INSIDE a Nova project (default: cwd), so that the
# nova.toml lookup succeeds and the only failing condition left is "file does not exist".
# Writes nothing anywhere.
# docs/guide/nova-cli.md:161 -- exit 2 = "Usage error (bad flag, file not found,
# wrong extension, missing nova.toml)".
# Exit 0 = every command exits 2. Exit 1 = at least one disagrees.
NOVA="$1"
[ -x "$NOVA" ] || { echo "usage: sh probe.sh <path-to-nova.exe> [project-dir]"; exit 3; }
[ -n "$2" ] && cd "$2"
[ -f nova.toml ] || { echo "note: no nova.toml in $(pwd) -- cd into a Nova project or pass one"; exit 3; }
fail=0
probe() {
  out=$(NO_COLOR=1 "$NOVA" "$@" 2>&1)
  r=$?
  printf '%-32s rc=%s  | %s\n' "nova $*" "$r" "$(printf '%s' "$out" | head -1)"
  [ "$r" = "2" ] || fail=1
}
probe check nosuch.nv
probe build nosuch.nv
probe test nosuch.nv
probe consume-analyze nosuch.nv
probe gc-layout-analyze nosuch.nv
probe doc nosuch.nv
probe test-build nosuch.nv
probe contracts list nosuch.nv
probe contracts verify nosuch.nv
probe bench run nosuch.nv
probe doc-query nosuch.json
echo "(the doc requires rc=2 on every line above)"
exit $fail
