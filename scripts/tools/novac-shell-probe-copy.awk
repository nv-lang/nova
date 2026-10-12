# SPDX-License-Identifier: MIT OR Apache-2.0
# novac-regen-shell.sh's only source rewrite: a pkgless probe has one module.
# Normalize checkout CRLF at the input boundary, not in the generated C.
{ sub(/\r$/, "") }
$0 == "module probe.shell_probe" {
    modules++
    print "module shell_probe"
    next
}
{ print }
END {
    if (modules != 1) {
        printf "novac-shell-probe-copy: FAIL — expected one module probe.shell_probe, got %d\n", modules > "/dev/stderr"
        exit 1
    }
}
