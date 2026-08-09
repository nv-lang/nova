#!/usr/bin/env bash
# Селфтест scripts/guards/check-lsp-launch-path.sh (план 262 Ф.А.2, реестр 221.1 №531).
#
# Страж без селфтеста не работает — правило владельца 2026-07-27, и
# check-guard-wiring его энфорсит. Проверяем ОБА направления: страж ловит
# расхождение путь-сборки/путь-запуска и НЕ ложнит на годном дереве.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-lsp-launch-path.sh"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

echo "== селфтест check-lsp-launch-path =="

# 1. На реальной репе — зелено (nova-lsp/.cargo/config.toml уже сводит путь
#    сборки к пути запуска — план 262 Ф.А.1).
if bash "$G" "$ROOT" >/dev/null 2>&1; then
    ok "реальная репа: путь сборки == путь запуска"
else
    bad "реальная репа краснит — nova-lsp/.cargo/config.toml пропал или сломан?"
fi

T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT

# 2. Ловит отсутствие nova-lsp/.cargo/config.toml (cargo metadata резолвит
#    target-dir в nova-lsp/target, а не в <root>/target).
mkdir -p "$T/neg1/nova-lsp/src" "$T/neg1/target"
cat > "$T/neg1/nova-lsp/Cargo.toml" <<'EOF'
[package]
name = "nova-lsp"
version = "0.1.0"
edition = "2021"
[[bin]]
name = "nova-lsp"
path = "src/main.rs"
EOF
echo 'fn main() {}' > "$T/neg1/nova-lsp/src/main.rs"
if command -v cargo >/dev/null 2>&1; then
    if bash "$G" "$T/neg1" >/dev/null 2>&1; then
        bad "НЕ поймал отсутствие nova-lsp/.cargo/config.toml"
    else
        ok "ловит отсутствие nova-lsp/.cargo/config.toml (target-dir не сведён)"
    fi
else
    ok "cargo недоступен — этот пункт пропущен, страж деградирует мягко (SKIP тем же путём)"
fi

# 3. Ловит расходящийся устаревший бинарь по хешу (чисто файловая проверка,
#    не зависит от наличия cargo).
mkdir -p "$T/neg2/nova-lsp/target/release" "$T/neg2/target/release" "$T/neg2/nova-lsp/.cargo" "$T/neg2/nova-lsp/src"
cat > "$T/neg2/nova-lsp/.cargo/config.toml" <<'EOF'
[build]
target-dir = "../target"
EOF
cat > "$T/neg2/nova-lsp/Cargo.toml" <<'EOF'
[package]
name = "nova-lsp"
version = "0.1.0"
edition = "2021"
[[bin]]
name = "nova-lsp"
path = "src/main.rs"
EOF
echo 'fn main() {}' > "$T/neg2/nova-lsp/src/main.rs"
printf 'OLD-STALE-BINARY-CONTENT' > "$T/neg2/nova-lsp/target/release/nova-lsp.exe"
printf 'NEW-FRESH-BINARY-CONTENT' > "$T/neg2/target/release/nova-lsp.exe"
if bash "$G" "$T/neg2" >/dev/null 2>&1; then
    bad "НЕ поймал расходящийся устаревший бинарь (разные хеши по двум путям)"
else
    ok "ловит расходящийся устаревший бинарь по хешу"
fi

# 4. НЕ ложнит: годное дерево (config.toml + БЕЗ устаревшего бинаря) — зелено.
mkdir -p "$T/pos/nova-lsp/.cargo" "$T/pos/nova-lsp/src" "$T/pos/target"
cat > "$T/pos/nova-lsp/.cargo/config.toml" <<'EOF'
[build]
target-dir = "../target"
EOF
cat > "$T/pos/nova-lsp/Cargo.toml" <<'EOF'
[package]
name = "nova-lsp"
version = "0.1.0"
edition = "2021"
[[bin]]
name = "nova-lsp"
path = "src/main.rs"
EOF
echo 'fn main() {}' > "$T/pos/nova-lsp/src/main.rs"
if command -v cargo >/dev/null 2>&1; then
    if bash "$G" "$T/pos" >/dev/null 2>&1; then
        ok "годное дерево (config.toml на месте) не ложнит"
    else
        bad "годное дерево покраснело — ложняк"
    fi
else
    ok "cargo недоступен — этот пункт пропущен"
fi

if [ "$FAILED" -eq 0 ]; then
    echo "check-lsp-launch-path selftest: OK"
    exit 0
else
    echo "check-lsp-launch-path selftest: ПРОВАЛ" >&2
    exit 1
fi
