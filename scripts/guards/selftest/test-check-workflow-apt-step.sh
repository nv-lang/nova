#!/usr/bin/env bash
# Самотест check-workflow-apt-step.sh (задача #37). Случаи, законное первым:
#   1. шаг через ci-apt-install.sh с timeout-minutes: 15 и apt в комментарии -> ok;
#   2. голый `sudo apt-get update && sudo apt-get install` (форма, зависавшая 2026-10-07) -> FAIL с адресом;
#   3. шаг через скрипт без timeout-minutes -> FAIL;
#   4. timeout-minutes: 30 -> FAIL;
#   5. пустой корень -> «судить нечего»;
#   6. пределом соседнего шага не прикрыться: timeout-minutes стоит у ДРУГОГО шага -> FAIL;
#   7. скрипт повтора: зависающий apt обрывается пределом и повторяется ровно 3 раза, затем rc=1;
#   8. скрипт повтора: первая попытка падает, вторая проходит -> rc=0.
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GUARD="$HERE/../check-workflow-apt-step.sh"
TOOL="$HERE/../../tools/ci-apt-install.sh"
T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
CASES=0
FAILS=0
ok()  { CASES=$((CASES+1)); echo "  ok: $1"; }
bad() { CASES=$((CASES+1)); FAILS=$((FAILS+1)); echo "  FAIL: $1"; }
mk() { rm -rf "$T/r"; mkdir -p "$T/r/.github/workflows"; }
# Голая форма СОБИРАЕТСЯ: файл лежит под scripts/, а страж читает workflow, но образцы не должны краснить поиск по дереву.
AG="apt-""get"
wf() { printf '%s\n' 'jobs:' '  a:' '    steps:' "$@" > "$T/r/.github/workflows/w.yml"; }

mk; wf '      - name: Install' "        # $AG install -y x -- quoted" '        run: bash scripts/tools/ci-apt-install.sh libgc-dev' '        timeout-minutes: 15'
out=$(bash "$GUARD" "$T/r" 2>&1); rc=$?
[ "$rc" -eq 0 ] && ok "1: законная форма -> ok" || bad "1: rc=$rc: $out"

mk; wf '      - name: Install' "        run: sudo $AG update && sudo $AG install -y libgc-dev"
out=$(bash "$GUARD" "$T/r" 2>&1); rc=$?
if [ "$rc" -ne 0 ] && printf '%s' "$out" | grep -q 'w.yml:5'; then ok "2: голый apt -> FAIL с адресом"; else bad "2: не пойман (rc=$rc): $out"; fi

mk; wf '      - name: Install' '        run: bash scripts/tools/ci-apt-install.sh libgc-dev'
out=$(bash "$GUARD" "$T/r" 2>&1); rc=$?
if [ "$rc" -ne 0 ] && printf '%s' "$out" | grep -q 'без timeout-minutes'; then ok "3: нет предела -> FAIL"; else bad "3: не пойман (rc=$rc): $out"; fi

mk; wf '      - name: Install' '        run: bash scripts/tools/ci-apt-install.sh libgc-dev' '        timeout-minutes: 30'
out=$(bash "$GUARD" "$T/r" 2>&1); rc=$?
if [ "$rc" -ne 0 ] && printf '%s' "$out" | grep -q '> 15'; then ok "4: предел 30 -> FAIL"; else bad "4: не пойман (rc=$rc): $out"; fi

rm -rf "$T/r"; mkdir -p "$T/r"
out=$(bash "$GUARD" "$T/r" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && printf '%s' "$out" | grep -q 'судить нечего'; then ok "5: пустой корень"; else bad "5: rc=$rc: $out"; fi

mk; wf '      - name: Install' '        run: bash scripts/tools/ci-apt-install.sh libgc-dev' '      - name: Other' '        run: echo hi' '        timeout-minutes: 10'
out=$(bash "$GUARD" "$T/r" 2>&1); rc=$?
if [ "$rc" -ne 0 ]; then ok "6: предел чужого шага не засчитан"; else bad "6: прикрылись чужим пределом: $out"; fi

# 7: apt, который молчит дольше предела
cat > "$T/hang.sh" <<'EOF'
#!/bin/sh
echo x >> "$CI_APT_COUNT"
sleep 30
EOF
: > "$T/count"
out=$(CI_APT_SUDO="" CI_APT_GET="$T/hang.sh" CI_APT_DPKG=true CI_APT_ATTEMPT_TIMEOUT=1 CI_APT_PAUSE=0 CI_APT_COUNT="$T/count" bash "$TOOL" pkg 2>&1); rc=$?
n=$(grep -c . "$T/count")
# update стоит первым в && : зависший update считается попыткой -> по одному запуску на попытку
if [ "$rc" -eq 1 ] && [ "$n" -eq 3 ]; then ok "7: зависание -> 3 попытки, затем rc=1"; else bad "7: rc=$rc запусков=$n: $out"; fi

# 8: первая попытка падает, вторая проходит
cat > "$T/flaky.sh" <<'EOF'
#!/bin/sh
case " $* " in *" update "*) echo u >> "$CI_APT_COUNT"; [ "$(grep -c . "$CI_APT_COUNT")" -ge 2 ] || exit 100;; esac
exit 0
EOF
: > "$T/count"
out=$(CI_APT_SUDO="" CI_APT_GET="$T/flaky.sh" CI_APT_DPKG=true CI_APT_ATTEMPT_TIMEOUT=5 CI_APT_PAUSE=0 CI_APT_COUNT="$T/count" bash "$TOOL" pkg 2>&1); rc=$?
if [ "$rc" -eq 0 ] && printf '%s' "$out" | grep -q 'попытка 2'; then ok "8: сбой, затем успех на 2-й попытке"; else bad "8: rc=$rc: $out"; fi

# 9: настоящий sudo сбрасывает окружение (env_reset); пакеты обязаны дойти до `apt-get install` аргументами.
# Фейковый sudo чистит окружение, как настоящий; фейковый apt-get пишет свои аргументы в файл с вшитым путём.
cat > "$T/sudo.sh" <<'EOF'
#!/bin/sh
exec env -i PATH="$PATH" "$@"
EOF
cat > "$T/rec.sh" <<EOF
#!/bin/sh
echo "\$*" >> "$T/argv"
exit 0
EOF
chmod +x "$T/sudo.sh" "$T/rec.sh"
: > "$T/argv"
out=$(CI_APT_SUDO="$T/sudo.sh" CI_APT_GET="$T/rec.sh" CI_APT_DPKG=true CI_APT_ATTEMPT_TIMEOUT=5 CI_APT_PAUSE=0 bash "$TOOL" libgc-dev clang 2>&1); rc=$?
if [ "$rc" -eq 0 ] && grep -q 'install -y libgc-dev clang$' "$T/argv" && grep -q ' update$' "$T/argv"; then ok "9: sudo сбросил окружение, пакеты дошли до apt-get install"
else bad "9: rc=$rc argv=[$(cat "$T/argv")] $out"; fi

if [ "$FAILS" -gt 0 ]; then
    echo "test-check-workflow-apt-step: FAIL ($FAILS из $CASES)"
    exit 1
fi
echo "test-check-workflow-apt-step ok: $CASES случаев"
exit 0
