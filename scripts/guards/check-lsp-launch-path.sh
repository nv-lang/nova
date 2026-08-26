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

# norm_path <path> -- one spelling for one place, WITHOUT requiring existence.
#
# Prev form was `cd "$p" && pwd -P || echo "$p"`, and that normalises only an
# EXISTING directory. On a tree never built -- EVERY CI checkout and every
# fresh worktree -- `<root>/target` is absent, so the two sides stay in
# DIFFERENT spellings of one path: `D:/.../nova-lsp/../target` against
# `/d/.../target`. The guard judged spelling rather than place, and was green
# only on an already-built disk (found 2026-08-23, same class as the CRLF
# baselines).
#
# `cygpath -u` settles the drive form where one exists (absent and unneeded on
# Linux); `realpath -m` collapses `..` on paths that are not there.
norm_path() {
    local p="$1"
    if command -v cygpath >/dev/null 2>&1; then
        p=$(cygpath -u "$p" 2>/dev/null || printf '%s' "$1")
    fi
    # (a) collapse `..` -- works on paths that are not there.
    if command -v realpath >/dev/null 2>&1; then
        p=$(realpath -m "$p" 2>/dev/null || printf '%s' "$p")
    elif command -v python >/dev/null 2>&1; then
        p=$(python -c "import os,sys;print(os.path.normpath(sys.argv[1]).replace(os.sep,'/'))" "$p")
    fi
    # (b) resolve the deepest EXISTING ancestor physically and re-attach the
    #     missing tail. `realpath -m` alone leaves a git-bash mount alias
    #     standing -- `/tmp/x` and `/c/Users/<name>/AppData/Local/Temp/x` are
    #     one place under two names, and only `pwd -P` settles that. `cd`
    #     needs the directory to exist, which is precisely what (a) is for.
    local tail="" base="$p"
    while [ -n "$base" ] && [ "$base" != "/" ] && [ "$base" != "." ] && [ ! -d "$base" ]; do
        tail="/$(basename "$base")$tail"
        base=$(dirname "$base")
    done
    if [ -d "$base" ]; then
        base=$(cd "$base" && pwd -P)
    fi
    printf '%s%s' "$base" "$tail"
}

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
            expected=$(norm_path "$root/target")
            if [ -n "$target_dir" ]; then
                local target_dir_abs same
                target_dir_abs=$(norm_path "$target_dir")
                # СРАВНИВАЕМ КАТАЛОГ, А НЕ БАЙТЫ ЕГО ИМЕНИ (№766, 2026-08-26).
                # `cargo metadata` отдаёт путь в UTF-8, а `$root` приходит в кодировке
                # оболочки: у разработчика с не-ASCII именем пользователя это ОДИН
                # и тот же каталог в двух написаниях, и строковое сравнение врёт.
                # На CI было зелёно только потому, что там `/home/runner`.
                # `-ef` сравнивает устройство+inode, то есть сами каталоги; строки
                # остаются запасным путём для ещё НЕ СОЗДАННОГО каталога.
                # СРАВНИВАЕМ СЫРЫЕ ПУТИ: именно `norm_path` (cygpath/realpath) и
                # портит кодировку — после неё каталога с таким именем не существует
                # вовсе, и `-ef` нечего сравнивать.
                same=0
                if [ -d "$target_dir" ] && [ -d "$root/target" ]; then
                    [ "$target_dir" -ef "$root/target" ] && same=1
                elif [ "$target_dir_abs" = "$expected" ]; then
                    same=1
                fi
                if [ "$same" -ne 1 ]; then
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
    # A sandbox path that is not ASCII breaks the comparison being tested: this
    # machine's Windows user name is Cyrillic, `mktemp -d` lands under it, and
    # cargo's metadata JSON and `pwd -P` then return the same directory in two
    # encodings -- the POS case would red on the encoding, not on the path.
    # Fall back to a sandbox beside the guard, ASCII by construction. On Linux
    # (CI) `mktemp -d` is already ASCII and this branch never runs.
    # NB: the ASCII test has to run on the RESOLVED path -- `mktemp -d` returns
    # `/tmp/tmp.XXXX`, which is ASCII, and the Cyrillic only appears once the
    # mount alias is resolved. Testing the alias would never fire.
    _tmp_real=$(cd "$tmp" && pwd -P)
    case "$_tmp_real" in
        *[!\ -~]*)
            rm -rf "$tmp"
            tmp="$(cd "$SCRIPT_DIR/../.." && pwd)/.guard-selftest-$$"
            rm -rf "$tmp"
            mkdir -p "$tmp" || { echo "selftest: cannot create $tmp" >&2; return 1; }
            ;;
    esac
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

    # POS2: the tree was NEVER built -- `<root>/target` absent, config in place.
    # This is exactly the shape the guard false-redded on before 2026-08-23, and
    # exactly the shape CI has EVERY time: a clean checkout, nothing built yet.
    mkdir -p "$tmp/pos2/nova-lsp/.cargo" "$tmp/pos2/nova-lsp/src"
    cat > "$tmp/pos2/nova-lsp/.cargo/config.toml" <<'EOF'
[build]
target-dir = "../target"
EOF
    cat > "$tmp/pos2/nova-lsp/Cargo.toml" <<'EOF'
[package]
name = "nova-lsp"
version = "0.1.0"
edition = "2021"
[[bin]]
name = "nova-lsp"
path = "src/main.rs"
EOF
    echo 'fn main() {}' > "$tmp/pos2/nova-lsp/src/main.rs"
    if command -v cargo >/dev/null 2>&1; then
        if run_check "$tmp/pos2" >/tmp/lsp_selftest_pos2.log 2>&1; then
            echo "selftest POS2: ok (an unbuilt tree does not false-red)"
        else
            echo "selftest POS2: FAIL -- a tree without target/ went red (that very false red):"; cat /tmp/lsp_selftest_pos2.log
            return 1
        fi
    else
        echo "selftest POS2: SKIP (no cargo in PATH)"
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
