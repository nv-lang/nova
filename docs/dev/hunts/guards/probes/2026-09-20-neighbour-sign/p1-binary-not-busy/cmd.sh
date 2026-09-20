#!/usr/bin/env bash
# Проба к находке 1: check-binary-not-busy.sh судит НАПИСАНИЕ пути в выводе
# `ps -W`, а не факт удержания файла.
#
# Запуск:  bash cmd.sh <КОРЕНЬ-РЕПОЗИТОРИЯ>
# (абсолютных путей внутри пробы нет: корень приходит аргументом)
set -u
REPO="${1:?ukazhi koren repozitoriya nova pervym argumentom}"
GUARD="$REPO/scripts/guards/check-binary-not-busy.sh"
[ -f "$GUARD" ] || { echo "net $GUARD" >&2; exit 2; }

HERE="$(cd "$(dirname "$0")" && pwd)"
TREE="$HERE/fake-tree"
rm -rf "$TREE"; mkdir -p "$TREE/nova-cli/target/release"
# "binar dereva" - bezobidnyy sistemnyy exe pod imenem nova.exe
cp "$SYSTEMROOT/System32/ping.exe" "$TREE/nova-cli/target/release/nova.exe"
EXE_WIN="$(cygpath -w "$TREE/nova-cli/target/release/nova.exe")"

echo "=== A. derzhateley net - kontrol"
bash "$GUARD" "$TREE"; echo "rc=$?"

echo
echo "=== B. ZAPUSK IZ POWERSHELL (predmet narushen: fayl uderzhan)"
echo "-- windows-put: $EXE_WIN"
powershell -NoProfile -Command "Start-Process -FilePath '$EXE_WIN' -ArgumentList '-n','60','127.0.0.1' -WindowStyle Hidden" >/dev/null 2>&1
sleep 3
echo "-- ps -W vidit process:"
ps -W | grep -i 'fake-tree' | sed 's/^/   /'
echo "-- popytka perezapisat fayl (imenno eto delaet cargo build):"
cp "$SYSTEMROOT/System32/ping.exe" "$TREE/nova-cli/target/release/nova.exe" 2>&1 | sed 's/^/   /'
echo "   rc perezapisi: $?"
echo "-- verdikt strazha:"
bash "$GUARD" "$TREE"; echo "rc=$?"
taskkill //IM nova.exe //F >/dev/null 2>&1
sleep 1

echo
echo "=== C. TOT ZHE binar i to zhe uderzhanie, no ZAPUSK IZ BASH"
"$TREE/nova-cli/target/release/nova.exe" -n 60 127.0.0.1 >/dev/null &
sleep 2
ps -W | grep -i 'fake-tree' | sed 's/^/   /'
bash "$GUARD" "$TREE"; echo "rc=$?"

taskkill //IM nova.exe //F >/dev/null 2>&1
wait 2>/dev/null
exit 0
