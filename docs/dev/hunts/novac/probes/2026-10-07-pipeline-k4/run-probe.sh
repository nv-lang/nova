# Hunt 2026-10-07 pipeline-k4 -- body shared by every cmd.sh.
#
# A probe is ONE multi-module PROGRAM: <probe>/main.nv.txt (the entry, with
# `// NOVAC_PROGRAM`) and its modules in subdirectories as <dir>/<mod>.nv.txt.
# Evidence is not a fixture: the suffix keeps it out of every corpus. This body
# copies the tree to a temporary package (a nova.toml beside it, every .nv.txt
# renamed .nv, the directory named as the probe's module path wants), then runs
# scripts/tools/novac-e1-smoke.sh on the entry from the repository root: the
# oracle builds the program, Carina emits ONE C file for all its modules
# (`NOVAC_SELF_PATH=<root> novac emit`), the C is compiled with the oracle's
# flags, and both stdouts are compared. Then Carina's C is grepped for the C
# spellings of the probe's sums (both layouts), so a run shows which one each
# unit printed.
#
# ROOT is found by walking up to AGENTS.md (or pass ROOT=<repo>). NOVAC
# overrides Carina's binary (default: the repository's, through the door of
# scripts/guards/lib/novac.sh). Every artefact goes to TMPDIR.
[ -n "$PROBE" ] || { echo "run-probe.sh: PROBE (the probe dir) is not set"; exit 2; }
[ -n "$NAME" ] || { echo "run-probe.sh: NAME (the probe's module root) is not set"; exit 2; }
[ -f "$PROBE/main.nv.txt" ] || { echo "MISSING $PROBE/main.nv.txt"; exit 2; }
[ -n "$TMPDIR" ] || TMPDIR=/tmp
export TMPDIR
if [ -z "$ROOT" ]; then
    d="$PROBE"
    while [ -n "$d" ] && [ "$d" != "/" ] && [ ! -f "$d/AGENTS.md" ]; do d=$(dirname "$d"); done
    if [ -f "$d/AGENTS.md" ]; then ROOT="$d"; else echo "run-probe.sh: no repository root above the probe -- set ROOT=<repo>"; exit 1; fi
fi
[ -n "$NOVAC" ] || { . "$ROOT/scripts/guards/lib/novac.sh"; NOVAC=$(novac_bin "$ROOT"); }
[ -x "$NOVAC" ] || { echo "run-probe.sh: no novac at $NOVAC (pass NOVAC=)"; exit 1; }
T=$(mktemp -d) || exit 1
printf '[package]\nname = "hunt31"\nversion = "0.0.0"\n' > "$T/nova.toml"
mkdir -p "$T/$NAME"
( cd "$PROBE" && find . -name '*.nv.txt' ) | while read -r f; do
    mkdir -p "$T/$NAME/$(dirname "$f")"
    cp "$PROBE/$f" "$T/$NAME/${f%.txt}"
done
[ -n "$(cd "$T/$NAME" && find . -name '*.nv')" ] || { echo "MISSING: no module copied"; rm -rf "$T"; exit 2; }
echo "=== PROGRAM"
( cd "$T/$NAME" && find . -name '*.nv' | sort | while read -r f; do echo "--- $f"; cat -n "$f"; done )
cd "$ROOT" || exit 1
# A path to the temporary directory is cut down to `<tmp>` in BOTH spellings: the
# shell's (/tmp/...) and the native one a diagnostic prints (a drive letter first),
# so no run.out carries the machine's own path (registry 698).
hide_tmp() { sed -E "s|$T|<tmp>|g; s|[A-Za-z]:/[^\"]*/($NAME/)|<tmp>/\\1|g; s|[A-Za-z]:/[^ ]*/novac-smoke\.[0-9]*/|<smoke>/|g"; }
echo "=== SMOKE: oracle build and run, Carina's one C file, compiled and run"
NOVAC="$NOVAC" NOVAC_BIN="$NOVAC" sh scripts/tools/novac-e1-smoke.sh "$T/$NAME/main.nv" 2>&1 | hide_tmp
echo "--- Carina's emission alone: its exit code, diagnostics, layouts of the probe's sums"
NOVAC_SELF_PATH="$T/$NAME" "$NOVAC" emit "$T/$NAME/main.nv" > "$T/e.c" 2> "$T/e.err"; echo "novac emit rc=$?"
grep '"code"' "$T/e.c" | hide_tmp | head -5
[ -s "$T/e.c" ] || echo "MISSING: Carina printed nothing"
for s in ${SUMS:-}; do
    echo "$s: NovaValue_$s $(grep -o "NovaValue_$s\b" "$T/e.c" | wc -l), Nova_$s* $(grep -o "Nova_$s\*" "$T/e.c" | wc -l)"
done
rm -rf "$T"
