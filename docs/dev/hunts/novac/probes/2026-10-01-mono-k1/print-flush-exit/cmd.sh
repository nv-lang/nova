#!/bin/sh
# Probe print-flush-exit (the END of a print statement flushes stdout before a process exit that skips the C buffers (registry 1703): `println` and `print` before `exit_process(3)` of std.os. Oracle `before-exit` `piece|`, rc 3)
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='nova_print_end'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
