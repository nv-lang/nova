#!/usr/bin/env bash
# Самотест стража check-commit-language: делегирует его собственному
# режиму --selftest, чтобы проверки жили рядом с проверяемым и не
# расходились с ним при правках.
set -u
export LC_ALL=C
DIR="$(cd "$(dirname "$0")/.." && pwd)"
exec bash "$DIR/check-commit-language.sh" --selftest
