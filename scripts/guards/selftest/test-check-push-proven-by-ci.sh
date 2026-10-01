#!/usr/bin/env bash
# Самотест scripts/guards/check-push-proven-by-ci.py — обе стороны, офлайн.
#
# Страж решает, может ли main переехать на коммит (решение владельца 2026-09-30:
# main принимает только коммит, доказанный CI). Ошибка в разрешающую сторону —
# main на недоказанном коммите; в запрещающую — стоящая выкладка. Поэтому здесь
# клетки на ОБА исхода каждого условия, а ответы `gh` подложены файлами
# (--runs-json / --jobs-json), реестр и список принятого красного — тоже
# (--registry / --accepted): вердикт не зависит ни от сети, ни от того, что
# сегодня лежит на GitHub (Г15).
#
# Последние клетки — не про ядро, а про ПОДКЛЮЧЕНИЕ: pre-push исполняется во
# временной репе с заглушкой стража и проверяется, что (1) он зовёт стража с
# ЛОКАЛЬНЫМ sha отправляемого main, (2) NOVA_SKIP_CI_CHECK=1 его НЕ выключает,
# (3) пуш не в main стража не зовёт.

set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
GUARD="$ROOT/scripts/guards/check-push-proven-by-ci.py"
HOOK="$ROOT/scripts/githooks/pre-push"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

PY=""
for _c in python3 python; do
    if command -v "$_c" >/dev/null 2>&1 && [ "$("$_c" -c 'print(42)' 2>/dev/null)" = "42" ]; then
        PY="$_c"; break
    fi
done
[ -n "$PY" ] || { echo "test-check-push-proven-by-ci: FAIL -- нет рабочего python/python3" >&2; exit 1; }

PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }

SHA=1111111111111111111111111111111111111111
WFS="nova-gate crate-tests nova-lint nova-test-regression contracts-crosscheck contracts-z3 nova-doc"

# runs <file> <wf:status:conclusion:id>... — прогоны на SHA; неназванные воркфлоу
# получают зелёный прогон, `-` в поле conclusion значит null, `none` в status —
# «прогона нет вовсе». Время создания растёт по порядку записи.
runs() {
    _f="$1"; shift
    "$PY" - "$_f" "$SHA" "$WFS" "$@" <<'PYEOF'
import json, sys
f, sha, wfs = sys.argv[1], sys.argv[2], sys.argv[3].split()
spec = [a.split(":") for a in sys.argv[4:]]
named = set(s[0] for s in spec)
out, t = [], 0
def add(wf, st, co, rid):
    global t
    t += 1
    out.append({"workflowName": wf, "status": st, "conclusion": None if co == "-" else co,
                "databaseId": int(rid), "event": "push", "headSha": sha,
                "createdAt": "2026-09-30T18:%02d:00Z" % t})
for i, wf in enumerate(wfs):
    if wf not in named:
        add(wf, "completed", "success", 100 + i)
for wf, st, co, rid in spec:
    if st != "none":
        add(wf, st, co, rid)
json.dump(out, open(f, "w"))
PYEOF
}

# Реестр-фикстура: открытая, закрытая, частично закрытая, без поля статуса.
REG="$TMP/registry.md"
cat > "$REG" <<'EOF'
| 9001 | 🟠 К2 | открытая строка. **Статус:** ОТКРЫТО |
| 9002 | 🟠 К2 | закрытая строка. **Статус:** ЗАКРЫТ 2026-09-30 (коммит abc). |
| 9003 | 🟠 К2 | частично. **Статус:** ЗАКРЫТ ЧАСТИЧНО — остаток класса жив. |
| 9004 | 🟠 К2 | строка без поля статуса. |
EOF

acc() { printf '%s\n' "# comment line" "$@" > "$TMP/acc.list"; }

JOBS_RED="$TMP/jobs.json"
cat > "$JOBS_RED" <<'EOF'
{"501": [{"name": "novac-gate (self-hosted compiler builds + module tests)", "conclusion": "failure"},
         {"name": "nova-gate (conformance + flagship examples)", "conclusion": "success"},
         {"name": "docs-guard (doc-conventions.md enforcement)", "conclusion": "skipped"}],
 "502": [{"name": "novac-gate lookalike in another workflow", "conclusion": "failure"}],
 "503": [],
 "508": [{"name": "conformance shard", "conclusion": "cancelled"}]}
EOF

# run_case <имя> <ждём rc> <ждём подстроку> <runs-файл> [env...]
run_case() {
    _name="$1"; _want="$2"; _needle="$3"; _runs="$4"; shift 4
    _out="$(env "$@" "$PY" "$GUARD" "$SHA" --runs-json "$_runs" --jobs-json "$JOBS_RED" \
            --registry "$REG" --accepted "$TMP/acc.list" 2>&1)"
    _rc=$?
    if [ "$_rc" != "$_want" ]; then
        bad "$_name (ждал rc=$_want, получил $_rc): $(printf '%s' "$_out" | tail -2)"
    elif ! printf '%s' "$_out" | grep -qF -- "$_needle"; then
        bad "$_name (нет '$_needle' в выводе): $(printf '%s' "$_out" | tail -2)"
    else
        ok "$_name"
    fi
}

echo "== ядро =="
acc
runs "$TMP/green.json"
run_case "все обязательные зелёные -> ok" 0 "ok: 7/7" "$TMP/green.json"

runs "$TMP/missing.json" "contracts-z3:none:-:0"
run_case "нет прогона одного воркфлоу -> FAIL" 1 "contracts-z3: no run" "$TMP/missing.json"

runs "$TMP/progress.json" "crate-tests:in_progress:-:7"
run_case "прогон ещё идёт -> FAIL (ждать)" 1 "still in_progress" "$TMP/progress.json"

runs "$TMP/red.json" "nova-gate:completed:failure:501"
run_case "красное задание не из списка -> FAIL" 1 "nova-gate / novac-gate" "$TMP/red.json"
run_case "NOVA_SKIP_CI_CHECK=1 стража не выключает" 1 "not proven green" "$TMP/red.json" NOVA_SKIP_CI_CHECK=1

acc "nova-gate / novac-gate #9001 open reason"
run_case "красное из списка, строка ОТКРЫТА -> ok" 0 "accepted by open row #9001" "$TMP/red.json"

acc "nova-gate / novac-gate #9003 partial"
run_case "строка ЗАКРЫТ ЧАСТИЧНО считается открытой -> ok" 0 "accepted by open row #9003" "$TMP/red.json"

acc "nova-gate / novac-gate #9002 closed reason"
run_case "из списка, строка ЗАКРЫТА -> FAIL (запись протухла)" 1 "#9002 is closed" "$TMP/red.json"
run_case "протухшая запись краснит и при всём зелёном" 1 "#9002 is closed" "$TMP/green.json"

acc "nova-gate / novac-gate #9999 no such row"
run_case "из списка, строки нет -> FAIL" 1 "#9999 does not exist" "$TMP/red.json"

acc "nova-gate / novac-gate #9004 no field"
run_case "из списка, у строки нет поля статуса -> FAIL" 1 "#9004 has no status field" "$TMP/red.json"

acc "nova-gate novac-gate without slash"
run_case "запись не по форме -> FAIL" 1 "unparsed entry" "$TMP/green.json"

acc "nova-gate / novac-gate #9001 open reason"
runs "$TMP/other.json" "nova-lint:completed:failure:502"
run_case "запись одного воркфлоу не покрывает задание другого" 1 "nova-lint / novac-gate lookalike" "$TMP/other.json"

runs "$TMP/nojobs.json" "nova-doc:completed:failure:503"
run_case "прогон красен, красных заданий не названо -> FAIL" 1 "cannot tell what failed" "$TMP/nojobs.json"

runs "$TMP/newer-green.json" "nova-lint:completed:failure:504" "nova-lint:completed:success:505"
run_case "судится НОВЕЙШИЙ прогон: красный, затем зелёный -> ok" 0 "ok: 7/7" "$TMP/newer-green.json"

runs "$TMP/newer-running.json" "nova-lint:completed:success:506" "nova-lint:queued:-:507"
run_case "судится новейший: зелёный, затем в очереди -> FAIL" 1 "still queued" "$TMP/newer-running.json"

runs "$TMP/cancelled.json" "nova-gate:completed:cancelled:508"
run_case "отменённое задание не доказательство" 1 "nova-gate / conformance shard: cancelled" "$TMP/cancelled.json"

echo "== ключ NOVA_PUSH_UNPROVEN =="
acc
run_case "ключ без номера строки -> FAIL" 1 "must name a registry row" "$TMP/red.json" NOVA_PUSH_UNPROVEN=1
run_case "ключ с номером -> ok, причина напечатана" 0 "SKIPPED by NOVA_PUSH_UNPROVEN: github down #9001" "$TMP/red.json" "NOVA_PUSH_UNPROVEN=github down #9001"

echo "== живой список против живого реестра =="
# Настоящий ci-accepted-red.list обязан разбираться, и каждая его запись —
# держаться открытой строкой настоящего реестра. Закрыли строку, не убрав
# запись, — краснеет здесь, в гейте, а не на пуше интегратора.
_out="$("$PY" "$GUARD" "$SHA" --runs-json "$TMP/green.json" --jobs-json "$JOBS_RED" 2>&1)"
if [ $? -eq 0 ]; then ok "живые записи списка держатся открытыми строками"; else
    bad "живой список: $(printf '%s' "$_out" | tail -2)"; fi

echo "== подключение: pre-push =="
REPO="$TMP/repo"
mkdir -p "$REPO/scripts/guards" "$REPO/scripts/githooks"
git -C "$REPO" init -q 2>/dev/null
cp "$HOOK" "$REPO/scripts/githooks/pre-push"
# Заглушка стража: записывает, с чем её позвали, и отказывает.
cat > "$REPO/scripts/guards/check-push-proven-by-ci.py" <<'EOF'
import sys
open(sys.argv[0] + ".called", "w").write(" ".join(sys.argv[1:]))
sys.exit(1)
EOF
printf '#!/usr/bin/env bash\nexit 0\n' > "$REPO/scripts/guards/check-ci-status.sh"
STUB_MARK="$REPO/scripts/guards/check-push-proven-by-ci.py.called"
LSHA=2222222222222222222222222222222222222222
RSHA=3333333333333333333333333333333333333333

( cd "$REPO" && printf 'refs/heads/main %s refs/heads/main %s\n' "$LSHA" "$RSHA" \
    | NOVA_SKIP_CI_CHECK=1 bash scripts/githooks/pre-push origin url >/dev/null 2>&1 )
_rc=$?
if [ "$_rc" = 1 ] && [ -f "$STUB_MARK" ] && [ "$(cat "$STUB_MARK")" = "$LSHA" ]; then
    ok "pre-push main: страж зван с ЛОКАЛЬНЫМ sha и при NOVA_SKIP_CI_CHECK=1, отказ остановил пуш"
else
    bad "pre-push main: rc=$_rc, звали с '$(cat "$STUB_MARK" 2>/dev/null)' (ждал rc=1 и $LSHA)"
fi
rm -f "$STUB_MARK"

( cd "$REPO" && printf 'refs/heads/integrate %s refs/heads/integrate %s\n' "$LSHA" "$RSHA" \
    | bash scripts/githooks/pre-push origin url >/dev/null 2>&1 )
_rc=$?
if [ "$_rc" = 0 ] && [ ! -f "$STUB_MARK" ]; then ok "pre-push не в main: страж не зван"; else
    bad "pre-push integrate: rc=$_rc, страж зван=$([ -f "$STUB_MARK" ] && echo да || echo нет)"; fi

( cd "$REPO" && printf '(delete) %s refs/heads/main %s\n' 0000000000000000000000000000000000000000 "$RSHA" \
    | NOVA_SKIP_CI_CHECK=1 bash scripts/githooks/pre-push origin url >/dev/null 2>&1 )
if [ ! -f "$STUB_MARK" ]; then ok "удаление main: доказывать нечего, страж не зван"; else
    bad "удаление main: страж зван с нулевым sha"; fi

TOTAL=$((PASS+FAIL))
if [ "$FAIL" -ne 0 ]; then
    echo "test-check-push-proven-by-ci: FAIL -- $FAIL/$TOTAL" >&2
    exit 1
fi
echo "test-check-push-proven-by-ci ok: $PASS/$TOTAL"
exit 0
