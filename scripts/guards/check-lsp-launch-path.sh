#!/usr/bin/env bash
# scripts/guards/check-lsp-launch-path.sh — путь сборки nova-lsp == путь запуска.
#
# ЗАЧЕМ (реестр 221.1 №531, план 262 Ф.А.1/А.2): редактор (VS Code/Neovim/
# Helix — `editors/vscode/client/extension.ts::findNovaLsp`,
# `editors/neovim/lspconfig.lua`) при отсутствии явного `nova.lsp.path`
# ищет бинарь по пути `<workspace_root>/target/{release,debug}/nova-lsp[.exe]`
# — то есть КОРЕНЬ РЕПЫ, не каталог крейта. `cargo build` из `nova-lsp/`
# (документированный способ сборки — CLAUDE.md: «LSP — из nova-lsp») по
# умолчанию кладёт результат в `nova-lsp/target/...` — ДРУГОЙ путь.
#
# Найдено 2026-08-09: запущенный редактором бинарь был на ШЕСТЬ ДНЕЙ старше
# компилятора. «Я пересобрал LSP» ничего не меняло и выглядело как фикс.
# Фикс носителя (руками пересобрать/скопировать) НЕ РЕШАЕТ — отстанет снова
# на следующий день. Настоящий фикс — `nova-lsp/.cargo/config.toml` с
# `target-dir = "../target"`, сводящий путь сборки к пути запуска АРХИТЕКТУРНО
# (одно и то же место, не два синхронизируемых вручную). Этот страж —
# проверка, что фикс НЕ ОТКАЧен и что не завалялся расходящийся артефакт.
#
# ЧТО ПРОВЕРЯЕТ (два направления, оба обязаны краснеть на реальную поломку):
#   (1) `cargo metadata` из nova-lsp/ обязан резолвить `target_directory` в
#       `<repo_root>/target` — если кто-то удалил/переопределил
#       nova-lsp/.cargo/config.toml, здесь красно.
#   (2) если ПОМИМО актуального пути (<repo_root>/target/{release,debug}/
#       nova-lsp.exe) на диске существует ещё и `nova-lsp/target/{...}/
#       nova-lsp.exe` (следы старой схемы сборки / ручного billed-собранного
#       бинаря) — их хеши обязаны совпадать, иначе на диске лежат ДВА разных
#       бинаря под одним именем и то, какой из них найдёт редактор, зависит
#       от того, что попадётся первым.
#
# Самопроверка обеих сторон (в самом файле, ниже, `--selftest`): истинная
# поломка красит гейт (сабботаж пути metadata + сабботаж хешей), заведомо
# годное дерево — зелёное.
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/guards/check-lsp-launch-path.sh [КОРЕНЬ]
#   bash scripts/guards/check-lsp-launch-path.sh --selftest   (проверка стража)

set -u
export LC_ALL=C

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    else
        # Windows git-bash без sha256sum (редкий случай) — certutil fallback.
        certutil -hashfile "$1" SHA256 2>/dev/null | sed -n '2p' | tr -d ' \r'
    fi
}

# ── основная проверка ───────────────────────────────────────────────────────
run_check() {
    local root="$1"
    local lsp_dir="$root/nova-lsp"
    local fail=0

    if [ ! -d "$lsp_dir" ]; then
        echo "check-lsp-launch-path: SKIP — $lsp_dir не найден" >&2
        return 0
    fi

    # (1) target-dir metadata resolves to <root>/target.
    if command -v cargo >/dev/null 2>&1; then
        local meta target_dir expected
        meta=$(cd "$lsp_dir" && cargo metadata --no-deps --format-version=1 2>/dev/null)
        if [ -n "$meta" ]; then
            target_dir=$(printf '%s' "$meta" | grep -o '"target_directory":"[^"]*"' | head -1 | sed 's/"target_directory":"//;s/"$//' | sed 's/\\\\/\//g')
            expected=$(cd "$root/target" 2>/dev/null && pwd -P || echo "$root/target")
            if [ -n "$target_dir" ]; then
                local target_dir_abs
                target_dir_abs=$(cd "$target_dir" 2>/dev/null && pwd -P || echo "$target_dir")
                if [ "$target_dir_abs" != "$expected" ]; then
                    echo "check-lsp-launch-path: НАРУШЕНИЕ — cargo target-dir у nova-lsp = '$target_dir_abs', ожидался '$expected'" >&2
                    echo "    (nova-lsp/.cargo/config.toml с target-dir=\"../target\" отсутствует или переопределён)" >&2
                    fail=1
                fi
            fi
        fi
    fi

    # (2) no diverging stale binary at the old (pre-fix) build path.
    for variant in release debug; do
        local stale="$lsp_dir/target/$variant/nova-lsp.exe"
        local launch="$root/target/$variant/nova-lsp.exe"
        # non-Windows binary name (no .exe) — check that variant too.
        local stale_nix="$lsp_dir/target/$variant/nova-lsp"
        local launch_nix="$root/target/$variant/nova-lsp"
        if [ -f "$stale" ] && [ -f "$launch" ]; then
            local h1 h2
            h1=$(sha256_of "$stale")
            h2=$(sha256_of "$launch")
            if [ "$h1" != "$h2" ]; then
                echo "check-lsp-launch-path: НАРУШЕНИЕ — $stale и $launch разошлись (хеш не совпадает)" >&2
                echo "    редактор запускает $launch — удали устаревший $stale, чтобы он не путал" >&2
                fail=1
            fi
        fi
        if [ -f "$stale_nix" ] && [ -f "$launch_nix" ]; then
            local h1n h2n
            h1n=$(sha256_of "$stale_nix")
            h2n=$(sha256_of "$launch_nix")
            if [ "$h1n" != "$h2n" ]; then
                echo "check-lsp-launch-path: НАРУШЕНИЕ — $stale_nix и $launch_nix разошлись (хеш не совпадает)" >&2
                fail=1
            fi
        fi
    done

    if [ "$fail" -eq 0 ]; then
        echo "check-lsp-launch-path ok: путь сборки nova-lsp == путь запуска редактора"
    fi
    return "$fail"
}

# ── самопроверка (--selftest): обе стороны — ловит поломку, не ложнит на годном ──
run_selftest() {
    local tmp
    tmp=$(mktemp -d) || { echo "selftest: mktemp failed" >&2; return 1; }
    trap 'rm -rf "$tmp"' RETURN

    # POS: nova-lsp/.cargo/config.toml present, target-dir resolves correctly,
    # no stale binary at all → must be GREEN.
    mkdir -p "$tmp/pos/nova-lsp/.cargo" "$tmp/pos/nova-lsp/src" "$tmp/pos/target"
    cat > "$tmp/pos/nova-lsp/.cargo/config.toml" <<'EOF'
[build]
target-dir = "../target"
EOF
    cat > "$tmp/pos/nova-lsp/Cargo.toml" <<'EOF'
[package]
name = "nova-lsp"
version = "0.1.0"
edition = "2021"
[[bin]]
name = "nova-lsp"
path = "src/main.rs"
EOF
    echo 'fn main() {}' > "$tmp/pos/nova-lsp/src/main.rs"
    if command -v cargo >/dev/null 2>&1; then
        if run_check "$tmp/pos" >/tmp/lsp_selftest_pos.log 2>&1; then
            echo "selftest POS: ok (проходит на годном дереве)"
        else
            echo "selftest POS: FAIL — годное дерево покраснело (ложняк):"; cat /tmp/lsp_selftest_pos.log
            return 1
        fi
    else
        echo "selftest POS: SKIP (нет cargo в PATH)"
    fi

    # NEG (1): no .cargo/config.toml at all → metadata target-dir stays
    # nova-lsp/target — must be RED.
    mkdir -p "$tmp/neg1/nova-lsp/src"
    cat > "$tmp/neg1/nova-lsp/Cargo.toml" <<'EOF'
[package]
name = "nova-lsp"
version = "0.1.0"
edition = "2021"
[[bin]]
name = "nova-lsp"
path = "src/main.rs"
EOF
    echo 'fn main() {}' > "$tmp/neg1/nova-lsp/src/main.rs"
    mkdir -p "$tmp/neg1/target"
    if command -v cargo >/dev/null 2>&1; then
        if run_check "$tmp/neg1" >/tmp/lsp_selftest_neg1.log 2>&1; then
            echo "selftest NEG1: FAIL — отсутствие .cargo/config.toml должно было покраснить, но прошло:"; cat /tmp/lsp_selftest_neg1.log
            return 1
        else
            echo "selftest NEG1: ok (ловит отсутствие .cargo/config.toml)"
        fi
    else
        echo "selftest NEG1: SKIP (нет cargo в PATH)"
    fi

    # NEG (2): diverging stale binary at both paths → must be RED regardless
    # of cargo availability (pure file-hash check).
    mkdir -p "$tmp/neg2/nova-lsp/target/release" "$tmp/neg2/target/release" "$tmp/neg2/nova-lsp/.cargo" "$tmp/neg2/nova-lsp/src"
    cat > "$tmp/neg2/nova-lsp/.cargo/config.toml" <<'EOF'
[build]
target-dir = "../target"
EOF
    cat > "$tmp/neg2/nova-lsp/Cargo.toml" <<'EOF'
[package]
name = "nova-lsp"
version = "0.1.0"
edition = "2021"
[[bin]]
name = "nova-lsp"
path = "src/main.rs"
EOF
    echo 'fn main() {}' > "$tmp/neg2/nova-lsp/src/main.rs"
    printf 'OLD-STALE-BINARY' > "$tmp/neg2/nova-lsp/target/release/nova-lsp.exe"
    printf 'NEW-FRESH-BINARY' > "$tmp/neg2/target/release/nova-lsp.exe"
    if run_check "$tmp/neg2" >/tmp/lsp_selftest_neg2.log 2>&1; then
        echo "selftest NEG2: FAIL — расходящиеся хеши должны были покраснить, но прошло:"; cat /tmp/lsp_selftest_neg2.log
        return 1
    else
        echo "selftest NEG2: ok (ловит расходящийся устаревший бинарь по хешу)"
    fi

    echo "check-lsp-launch-path selftest: ALL OK"
    return 0
}

ROOT_ARG="${1:-}"
if [ "$ROOT_ARG" = "--selftest" ]; then
    run_selftest
    exit $?
fi

ROOT="${ROOT_ARG:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
run_check "$ROOT"
exit $?
