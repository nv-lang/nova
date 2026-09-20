#!/bin/sh
# Probe 2 -- what unit_of actually builds. Same verbatim copy of
# novac/src/pipeline/pipeline.nv lines 110-439; prints the unit text and table.
# from novac/src/pipeline/pipeline.nv (lines 110-439).
#
# NOVA = full path to the oracle binary, e.g.
#   /d/Sources/nv-lang/nova/nova-cli/target/release/nova.exe
# and the three repo-relative roots the oracle needs when built outside a repo:
#   NOVA_STD_PATH=<repo>\std\src
#   NOVA_CG_INCLUDE=<repo>\compiler-codegen
#   NOVA_RT_DIR=<repo>\compiler-codegen\nova_rt
# Run from the directory that holds this file.
set -e
"$NOVA" build probe.nv -o probe.exe
./probe.exe
