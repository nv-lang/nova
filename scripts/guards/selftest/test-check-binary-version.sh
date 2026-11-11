#!/usr/bin/env bash
# Self-test for check-binary-version.sh.
set -euo pipefail

guard="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/check-binary-version.sh"
fixture="$(mktemp -d)"
trap 'rm -rf "$fixture"' EXIT

git -C "$fixture" init -q
GIT_AUTHOR_NAME=Selftest GIT_AUTHOR_EMAIL=selftest@example.invalid \
GIT_COMMITTER_NAME=Selftest GIT_COMMITTER_EMAIL=selftest@example.invalid \
    git -C "$fixture" -c commit.gpgSign=false commit --allow-empty -qm fixture
git -C "$fixture" tag --no-sign oracle-v0.1.4

mkdir "$fixture/nova-cli"
printf 'version = "0.1.4"\n' > "$fixture/nova-cli/Cargo.toml"
cat > "$fixture/nova" <<'EOF'
#!/bin/sh
test "$1" = --version || exit 2
printf 'nova %s\n' "$TEST_NOVA_VERSION"
EOF
chmod +x "$fixture/nova"

TEST_NOVA_VERSION=0.1.4 bash "$guard" "$fixture" "$fixture/nova"
if output=$(TEST_NOVA_VERSION=0.1.3 bash "$guard" "$fixture" "$fixture/nova" 2>&1); then
    echo 'Expected a stale binary to fail' >&2
    exit 1
fi
[[ "$output" == *'0.1.3'* && "$output" == *'Cargo=0.1.4'* ]]

printf 'version = "0.1.3"\n' > "$fixture/nova-cli/Cargo.toml"
if output=$(bash "$guard" "$fixture" 2>&1); then
    echo 'Expected a manifest/tag mismatch to fail' >&2
    exit 1
fi
[[ "$output" == *'version=0.1.3'* && "$output" == *'oracle-v0.1.4'* ]]
