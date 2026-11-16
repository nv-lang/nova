#!/usr/bin/env bash
# scripts/guards/check-binary-version.sh
# Печатаемая версия релиза совпадает с тегом (урок перевыпуска 0.1.3).
#
# ДОМ И ОСНОВАНИЕ: релиз oracle-v0.1.3 (2026-11-08) был перевыпущен в тот
# же день — оба .exe в официальном архиве печатали `nova 0.1.0`: Cargo-
# версия не бампалась НИ В ОДНОМ релизе (известно с 0.1.1, каждый раз
# откладывалось «иначе круг CI»; нашло внешнее окно при скачивании).
# План: docs/plans/295-oracle-release-v0-1.md.
# Страж закрывает класс дважды:
#   1. Cargo-версия (nova-cli) равна версии последнего тега oracle-v*;
#   2. собранный бинарь ПЕЧАТАЕТ её (`--version`), а не только декларирует.
#
# Запуск: scripts/guards/check-binary-version.sh <корень репо> [бинарь]
#   <корень>  — точка отсчёта (.git, Cargo.toml, теги);
#   [бинарь]  — необязательный путь к собранному nova; по умолчанию
#               nova-cli/target/release/nova (если собран); проверка (2)
#               молча пропускается, когда бинаря нет (сборка — не часть
#               каждого круга; страж не требует сборки, он её судит).
set -uo pipefail
ROOT="${1:-.}"
BIN="${2:-$ROOT/nova-cli/target/release/nova}"
cd "$ROOT" || { echo "check-binary-version: FAIL — нет корня $ROOT" >&2; exit 1; }

# ── (1) Cargo-версия == последнему тегу oracle-v* ────────────────────────
CARGO_V=$(sed -n 's/^version = "\(.*\)"/\1/p' nova-cli/Cargo.toml | head -1)
TAG=$(git tag -l 'oracle-v*' --sort=-v:refname | head -1)
if [ -z "$TAG" ]; then
    echo "check-binary-version ok: тегов oracle-v* ещё нет — судить нечего"
    exit 0
fi
TAG_V="${TAG#oracle-v}"
if [ "$CARGO_V" != "$TAG_V" ]; then
    echo "check-binary-version: FAIL — nova-cli/Cargo.toml version=$CARGO_V, а последний тег $TAG (= $TAG_V)." >&2
    echo "    Урок перевыпуска 0.1.3: бамп версии — тем же коммитом, что и тег," >&2
    echo "    иначе бинари релиза печатают прежнюю версию (класс «известно с 0.1.1»)." >&2
    exit 1
fi

# ── (2) бинарь печатает её ──────────────────────────────────────────────
if [ ! -x "$BIN" ]; then
    echo "check-binary-version ok (декларативно): Cargo=$CARGO_V == тегу $TAG; бинарь не собран — печать не судим"
    exit 0
fi
PRINTED_V=$("$BIN" --version 2>/dev/null | awk '{print $2}')
if [ "$PRINTED_V" != "$CARGO_V" ]; then
    echo "check-binary-version: FAIL — бинарь $BIN печатает '${PRINTED_V:-пусто}', а Cargo=$CARGO_V." >&2
    echo "    Пересобери: бинарь старее правки версии (stale-build класс)." >&2
    exit 1
fi
echo "check-binary-version ok: Cargo=$CARGO_V == тегу $TAG == печати бинаря"
