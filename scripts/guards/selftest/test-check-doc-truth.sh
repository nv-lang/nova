#!/usr/bin/env bash
# Селфтест scripts/guards/check-doc-truth.sh (реестр 221.1 №455).
# Покрывает ОБЕ оси: (1) имена EXPECT_*-маркеров, (2) исполнимость `nova ...`
# команд из code-fence в AGENTS.md/docs/dev/** через `<sub> --help`.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-doc-truth.sh"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

echo "== селфтест check-doc-truth =="

# 1. Реальная репа: долг не превышает baseline по обеим осям.
if bash "$G" "$ROOT" >/dev/null 2>&1; then
    ok "реальная репа: долг в пределах baseline (обе оси)"
else
    bad "реальная репа краснит (долг вырос сверх baseline?)"
fi

T=$(mktemp -d)
mkdir -p "$T/scripts/guards" "$T/docs/dev" "$T/docs/guide" "$T/nova-cli/target/release"
# BASELINE и BIN резолвятся через SCRIPT_DIR/ROOT самого стража — на фикстуре
# страж должен видеть baseline и бинарь ФИКСТУРЫ, не реальной репы, поэтому
# копируем стража и (один раз, ОСЬ 2 нужен реальный бинарь) собранный nova.
cp "$G" "$T/scripts/guards/"
TG="$T/scripts/guards/check-doc-truth.sh"

REAL_BIN=""
for cand in "$ROOT/nova-cli/target/release/nova.exe" "$ROOT/nova-cli/target/release/nova"; do
    [ -x "$cand" ] && REAL_BIN="$cand" && break
done
if [ -n "$REAL_BIN" ]; then
    cp "$REAL_BIN" "$T/nova-cli/target/release/"
else
    echo "  (нет собранного nova в $ROOT — ОСЬ 2 селфтеста пропущена, проверится только ОСЬ 1)" >&2
fi

# ---------- ОСЬ 1: имена маркеров ----------

printf 'unknown_markers=0\nunrunnable_commands=999\n' > "$T/scripts/guards/doc-truth.baseline"

cat > "$T/AGENTS.md" <<'EOF'
Marker reference: `EXPECT_STDOUT`, `EXPECT_COMPILE_ERROR`, `EXPECT_COMPILE_WARNING`.
EOF
if bash "$TG" "$T" >/dev/null 2>&1; then
    ok "ось 1: принимает только известные раннеру имена"
else
    bad "ось 1: ложно краснит на известных именах"
fi

cat > "$T/AGENTS.md" <<'EOF'
Marker reference: `EXPECT_STDOUT`, `EXPECT_LINT_WARNING`.
EOF
OUT=$(bash "$TG" "$T" 2>&1)
if [ $? -ne 0 ] && echo "$OUT" | grep -q "AGENTS.md"; then
    ok "ось 1: ловит неизвестное имя и называет файл"
else
    bad "ось 1: НЕ поймал EXPECT_LINT_WARNING или не назвал файл"
fi

rm -f "$T/AGENTS.md"
printf 'unknown_markers=1\n' > /dev/null  # (используем прежний baseline=0 ниже отдельно)
printf 'unrunnable_commands=999\n' > /dev/null

# храповик оси 1: долг в пределах ненулевого baseline — зелёный.
printf 'unknown_markers=1\nunrunnable_commands=999\n' > "$T/scripts/guards/doc-truth.baseline"
cat > "$T/docs/dev/x.md" <<'EOF'
`EXPECT_MADE_UP` is not real.
EOF
if bash "$TG" "$T" >/dev/null 2>&1; then
    ok "ось 1: храповик пропускает долг в пределах baseline"
else
    bad "ось 1: храповик ложно краснит на долге в пределах baseline"
fi
rm -f "$T/docs/dev/x.md"
printf 'unknown_markers=0\nunrunnable_commands=999\n' > "$T/scripts/guards/doc-truth.baseline"

# ---------- ОСЬ 2: исполнимость команд (нужен собранный nova) ----------

if [ -n "$REAL_BIN" ]; then
    # Ноль долга по обеим осям для 2a-2e — детекция и её результат (exit-код)
    # проверяются ВМЕСТЕ; ratchet-поведение отдельно проверено в 2f.
    printf 'unknown_markers=0\nunrunnable_commands=0\n' > "$T/scripts/guards/doc-truth.baseline"

    # 2a. Известная подкоманда + известный флаг + позиционный аргумент — зелёный.
    cat > "$T/AGENTS.md" <<'EOF'
```sh
nova check spec_tests
```
EOF
    if bash "$TG" "$T" >/dev/null 2>&1; then
        ok "ось 2: валидная команда (subcommand+flag+positional) проходит"
    else
        bad "ось 2: ложно краснит на валидной команде"
    fi

    # 2b. Неизвестная подкоманда — красный, сообщение называет её.
    cat > "$T/AGENTS.md" <<'EOF'
```sh
nova frobnicate-not-a-real-subcommand x
```
EOF
    OUT=$(bash "$TG" "$T" 2>&1)
    if [ $? -ne 0 ] && echo "$OUT" | grep -q "unknown-subcommand"; then
        ok "ось 2: ловит неизвестную подкоманду"
    else
        bad "ось 2: НЕ поймал неизвестную подкоманду"
    fi

    # 2c. Известная подкоманда, неизвестный флаг — красный.
    cat > "$T/AGENTS.md" <<'EOF'
```sh
nova check spec_tests --this-flag-does-not-exist
```
EOF
    OUT=$(bash "$TG" "$T" 2>&1)
    if [ $? -ne 0 ] && echo "$OUT" | grep -q -- "--this-flag-does-not-exist"; then
        ok "ось 2: ловит неизвестный флаг"
    else
        bad "ось 2: НЕ поймал неизвестный флаг"
    fi

    # 2d. `test` без позиционного пути (реальный регресс №455: Plan 172.6
    # сделал путь обязательным, AGENTS.md/test-conventions.md этого не знали).
    cat > "$T/AGENTS.md" <<'EOF'
```sh
nova-cli/target/release/nova test
```
EOF
    OUT=$(bash "$TG" "$T" 2>&1)
    if [ $? -ne 0 ] && echo "$OUT" | grep -q "missing-required-positional"; then
        ok "ось 2: ловит отсутствующий обязательный позиционный путь (№455 регресс)"
    else
        bad "ось 2: НЕ поймал missing-required-positional"
    fi

    # 2e. Плейсхолдер/shell-конструкция — пропускается (не красит), но считается.
    cat > "$T/AGENTS.md" <<'EOF'
```sh
nova check <file.nv>
nova check spec_tests | tee out.log
```
EOF
    printf 'unknown_markers=0\nunrunnable_commands=0\n' > "$T/scripts/guards/doc-truth.baseline"
    OUT=$(bash "$TG" "$T" 2>&1)
    if [ $? -eq 0 ] && echo "$OUT" | grep -q "пропущено.*=2"; then
        ok "ось 2: плейсхолдер/shell-конструкция пропущены, но посчитаны (=2)"
    else
        bad "ось 2: плейсхолдер/shell-конструкция не пропущены или не посчитаны"
    fi

    # 2f. Храповик оси 2: долг в пределах ненулевого baseline — зелёный;
    #     рост сверх baseline — красный.
    cat > "$T/AGENTS.md" <<'EOF'
```sh
nova frobnicate-not-a-real-subcommand x
```
EOF
    printf 'unknown_markers=0\nunrunnable_commands=1\n' > "$T/scripts/guards/doc-truth.baseline"
    if bash "$TG" "$T" >/dev/null 2>&1; then
        ok "ось 2: храповик пропускает долг в пределах baseline"
    else
        bad "ось 2: храповик ложно краснит на долге в пределах baseline"
    fi

    cat > "$T/AGENTS.md" <<'EOF'
```sh
nova frobnicate-not-a-real-subcommand x
nova another-fake-one y
```
EOF
    if bash "$TG" "$T" >/dev/null 2>&1; then
        bad "ось 2: НЕ поймал рост долга сверх baseline"
    else
        ok "ось 2: ловит рост долга сверх baseline"
    fi
fi

rm -rf "$T"

if [ "$FAILED" -eq 0 ]; then
    echo "селфтест check-doc-truth: все проверки ok"
    exit 0
fi
echo "селфтест check-doc-truth: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
