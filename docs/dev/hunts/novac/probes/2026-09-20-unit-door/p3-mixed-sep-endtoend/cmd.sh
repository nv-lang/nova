#!/bin/sh
# Probe 3 -- END TO END. sub/mod/a.nv calls a function declared in its peer
# sub/mod/b.nv. SUBJECT and CONTROL differ ONLY in how one separator of the
# shared directory prefix is spelled: "sub/mod" versus "sub\mod".
#
# NOVAC = full path to novac.exe  (<repo>/novac/target/novac.exe)
# NOVA_STD_PATH = <repo>/std/src   (novac reads the std interface from it)
# Run from the directory that holds this file.
echo "=== CONTROL: uniform separators -- ONE unit ==="
NOVAC_UNIT=1 "$NOVAC" check sub/mod/a.nv sub/mod/b.nv; echo "rc=$?  (no JSON above = clean)"
echo '=== SUBJECT: same two files, prefix spelled "sub\mod" for the first ==='
NOVAC_UNIT=1 "$NOVAC" check 'sub\mod\a.nv' sub/mod/b.nv; echo "rc=$?"
