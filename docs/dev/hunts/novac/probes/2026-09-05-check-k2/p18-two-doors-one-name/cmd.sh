#!/bin/sh
# run from the nova worktree root; both binaries are asked the same file
novac/target/novac.exe check docs/dev/hunts/novac/probes/2026-09-05-check-k2/p18-two-doors-one-name/probe.nv; echo "novac rc=$?"
nova-cli/target/release/nova.exe check docs/dev/hunts/novac/probes/2026-09-05-check-k2/p18-two-doors-one-name/probe.nv 2>&1 | grep -v vcpkg | head -6
