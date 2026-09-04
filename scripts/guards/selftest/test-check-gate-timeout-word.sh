#!/usr/bin/env bash
# scripts/guards/selftest/test-check-gate-timeout-word.sh — самотест стража
# «„убит пределом“ и „красный“ — РАЗНЫЕ СЛОВА» (правило Г16 конвенций гейта,
# docs/dev/gate-guard-conventions.md; аудит — план 274 §10.3а).
#
# ПОЧЕМУ САМОТЕСТ ИМЕННО ТАКОЙ. Страж заведён по классу, где ОТСУТСТВИЕ вывода
# надело одежду вывода: шаг `crate-tests` был снят своим пределом на 601-й
# секунде и напечатал «тесты Rust красные» при 1891 зелёном тесте. Страж,
# проверенный только с одной стороны, повторил бы ровно эту ошибку этажом выше:
# зелёный ноль на уехавшем образце неотличим от зелёного ноля на чистом дереве.
#
# СЕМЬ СЛУЧАЕВ, каждый отвечает на свой вопрос:
#   1. вызов с разбором кода 124 — зелёный;
#   2. вызов без единого признака различения — КРАСНЫЙ, и место названо файл:строка;
#   3. НИ ОДНОГО вызова timeout — КРАСНЫЙ как потеря мишени, а не зелёный ноль;
#   4. признак ДАЛЬШЕ окна — КРАСНЫЙ (иначе «где-то в файле» засчитывалось бы);
#   5. вызов в комментарии — НЕ считается (проза, цитирующая форму, законна);
#   6. `with-deadline.sh <предел>` — тоже вызов с пределом, и он судится;
#   7. под судом ноль файлов — КРАСНЫЙ, тоже потеря мишени.
set -u
export LC_ALL=C

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-gate-timeout-word.py"
T="${TMPDIR:-/tmp}/gate-timeout-word-selftest.$$"
FAILED=0

ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1"; FAILED=$((FAILED+1)); }

mkdir -p "$T"
trap 'rm -rf "$T"' EXIT

printf 'undistinguished=0\n' > "$T/base0"
run() { GATE_TIMEOUT_WORD_BASELINE="$T/base0" python "$G" "$ROOT" "$1" >"$T/out" 2>"$T/err"; }

filler() { for _ in $(seq 1 14); do echo '    x=0'; done; }

# --- 1. предел разобран: 124 назван ------------------------------------------
mkdir -p "$T/good/guards"
{ echo '#!/bin/sh'
  echo 'run_step() {'
  echo '    timeout 600 cargo test --release'
  echo '    rc=$?'
  echo '    if [ "$rc" -eq 124 ]; then'
  echo '        echo "step killed by its own limit: no verdict" >&2'
  echo '        return 1'
  echo '    fi'
  echo '    return "$rc"'
  echo '}'; } > "$T/good/guards/a.sh"
if run "$T/good"; then ok "вызов с разбором 124 — зелёный"; else bad "разбор есть, а страж красный: $(cat "$T/err")"; fi

# --- 2. ГЛАВНЫЙ случай: снятие неотличимо от отказа ---------------------------
mkdir -p "$T/naked/guards"
{ echo '#!/bin/sh'
  echo 'run_step() {'
  echo '    timeout 600 cargo test --release'
  echo '    rc=$?'
  echo '    if [ "$rc" -ne 0 ]; then'
  echo '        echo "Rust tests are red" >&2'
  echo '        return 1'
  echo '    fi'
  echo '    return 0'
  echo '}'; } > "$T/naked/guards/a.sh"
if run "$T/naked"; then
    bad "вызов без различения снятия прошёл зелёным"
else
    grep -q "a.sh:3" "$T/err" && ok "неразличающий вызов пойман и назван строкой" \
        || bad "красный, но без адреса: $(cat "$T/err")"
fi

# --- 3. мишень уехала: ни одного timeout --------------------------------------
mkdir -p "$T/notarget/guards"
{ echo '#!/bin/sh'
  echo 'run_step() {'
  echo '    cargo test --release'
  echo '}'; } > "$T/notarget/guards/a.sh"
if run "$T/notarget"; then
    bad "ни одного вызова timeout — а страж сказал зелёный"
else
    grep -q "мишень" "$T/err" && ok "ноль вызовов timeout — красный, назван потерей мишени" \
        || bad "красный, но не про мишень: $(cat "$T/err")"
fi

# --- 4. признак дальше окна ----------------------------------------------------
mkdir -p "$T/far/guards"
{ echo '#!/bin/sh'
  echo '    timeout 600 cargo test --release'
  filler
  echo '    [ "$rc" -eq 124 ] && echo "killed by limit"'; } > "$T/far/guards/a.sh"
if run "$T/far"; then
    bad "признак за пределами окна засчитался как различение"
else
    grep -q "a.sh:2" "$T/err" && ok "признак дальше окна не считается" \
        || bad "красный, но без адреса: $(cat "$T/err")"
fi

# --- 5. проза не считается ------------------------------------------------------
mkdir -p "$T/prose/guards"
{ echo '#!/bin/sh'
  echo '    timeout 600 cargo test --release'
  echo '    rc=$?'
  echo '    [ "$rc" -eq 124 ] && echo "killed by its own limit"'
  filler
  echo '# timeout 30 curl status.json -- so it used to be written, and it is gone'
  echo '    echo done'; } > "$T/prose/guards/a.sh"
if run "$T/prose"; then ok "вызов в комментарии не считается"; else bad "комментарий посчитан вызовом: $(cat "$T/err")"; fi

# --- 6. вторая форма предела: with-deadline.sh ---------------------------------
mkdir -p "$T/wd/tools"
{ echo '#!/bin/sh'
  echo '    timeout 5 true'
  echo '    rc=$?'
  echo '    [ "$rc" -eq 124 ] && echo "killed by its own limit"'
  filler
  echo '    bash "$ROOT/scripts/tools/with-deadline.sh" 300 bash "$STEP"'
  echo '    echo done'; } > "$T/wd/tools/x.sh"
if run "$T/wd"; then
    bad "with-deadline.sh без различения снятия прошёл зелёным"
else
    grep -q "x.sh:19" "$T/err" && ok "with-deadline.sh судится наравне с timeout, место названо" \
        || bad "красный, но без адреса: $(cat "$T/err")"
fi

# --- 7. под судом ноль файлов ---------------------------------------------------
mkdir -p "$T/empty/guards"
if run "$T/empty"; then
    bad "ноль подсудных файлов — а страж зелёный"
else
    grep -q "мишень" "$T/err" && ok "ноль файлов — красный, назван потерей мишени" \
        || bad "красный, но не про мишень: $(cat "$T/err")"
fi

echo "итог: FAIL $FAILED"
if [ "$FAILED" -eq 0 ]; then
    echo "test-check-gate-timeout-word ok: неразличённое снятие, признак вне окна и обе потери мишени краснеют с адресом; разбор 124 и проза законны"
    exit 0
fi
exit 1
