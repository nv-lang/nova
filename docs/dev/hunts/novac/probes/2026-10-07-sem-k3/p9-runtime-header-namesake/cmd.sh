#!/bin/sh
# run from anywhere inside the repository; novac/target/novac.exe must be built
cd "$(git rev-parse --show-toplevel)" || exit 2
novac/target/novac.exe emit docs/dev/hunts/novac/probes/2026-10-07-sem-k3/p9-runtime-header-namesake/probe.nv | grep -n 'struct Nova_ChannelState'; grep -n 'struct Nova_ChannelState {' compiler-codegen/nova_rt/channels.h
