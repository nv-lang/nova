#!/usr/bin/env bash
# Самотест check-ci-status.sh.
#
# Страж без самотеста у нас считается неработающим (урок LC_ALL=C: страж молча
# давал ноль хитов и выглядел зелёным). Здесь доказываются ШЕСТЬ свойств —
# по три пары «ловит / не лжёт»:
#   1. RED    — красный прогон на хеше: сообщает RED; в --strict роняет (exit 1).
#   2. OK     — все прогоны зелёные: сообщает OK и НЕ роняет даже в --strict.
#   3. STALE  — прогонов на хеше нет, коммит старый: сообщает STALE, в --strict
#               роняет; при свежем коммите (порог не пройден) — НЕ роняет.
#   4. SKIP   — `gh` недоступен: exit 0 и явная строка о причине (падение сети
#               не должно ронять локальный гейт).
#
# Приём: подставной `gh` в начале PATH отдаёт заранее заданный JSON. Настоящая
# сеть в самотесте не участвует.
export LC_ALL=C
set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
GUARD="$ROOT/scripts/guards/check-ci-status.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

FAILED=0
ok()  { echo "  ok: $*"; }
bad() { echo "  ПРОВАЛ: $*"; FAILED=1; }

# Хеш, про который спрашиваем — берём реальный HEAD, чтобы `git show -s` знал
# его дату (для проверки порога STALE).
SHA="$(git -C "$ROOT" rev-parse HEAD)"
SHORT="$(echo "$SHA" | cut -c1-9)"

make_gh() {  # $1 — JSON, который вернёт `gh run list`
    mkdir -p "$TMP/bin"
    cat > "$TMP/bin/gh" <<EOF
#!/usr/bin/env bash
case "\$1" in
  auth) exit 0 ;;
  run)  cat <<'JSON'
$1
JSON
        ;;
  *) exit 0 ;;
esac
EOF
    chmod +x "$TMP/bin/gh"
}

run_guard() {  # $@ — аргументы стража; печатает вывод, возвращает код
    PATH="$TMP/bin:$PATH" bash "$GUARD" "$@" 2>&1
}
run_guard_code() {
    PATH="$TMP/bin:$PATH" bash "$GUARD" "$@" >/dev/null 2>&1; echo $?
}

echo "самотест check-ci-status:"

# ── 1. RED: ловит и роняет в --strict ────────────────────────────────────────
make_gh "[{\"name\":\"nova-gate\",\"status\":\"completed\",\"conclusion\":\"failure\",\"headSha\":\"$SHA\",\"createdAt\":\"2026-08-07T00:00\"}]"
OUT="$(run_guard "$SHA")"
echo "$OUT" | grep -q "RED" && ok "RED — красный прогон распознан" || bad "RED не распознан: $OUT"
[ "$(run_guard_code --strict "$SHA")" = "1" ] \
    && ok "RED — --strict роняет (exit 1)" || bad "RED — --strict не уронил"

# ── 2. OK: не лжёт (зелёное не должно ронять даже в --strict) ────────────────
make_gh "[{\"name\":\"nova-gate\",\"status\":\"completed\",\"conclusion\":\"success\",\"headSha\":\"$SHA\",\"createdAt\":\"2026-08-07T00:00\"}]"
OUT="$(run_guard "$SHA")"
echo "$OUT" | grep -q "OK" && ok "OK — зелёный прогон распознан" || bad "OK не распознан: $OUT"
[ "$(run_guard_code --strict "$SHA")" = "0" ] \
    && ok "OK — --strict НЕ роняет на зелёном" || bad "OK — --strict уронил на зелёном (ложняк!)"

# ── 3. STALE: прогонов на хеше нет ───────────────────────────────────────────
# Коммит HEAD старше порога 0 минут → STALE обязан сработать.
make_gh "[{\"name\":\"nova-gate\",\"status\":\"completed\",\"conclusion\":\"success\",\"headSha\":\"deadbeef00\",\"createdAt\":\"2026-08-07T00:00\"}]"
OUT="$(NOVA_CI_STALE_MIN=0 PATH="$TMP/bin:$PATH" bash "$GUARD" "$SHA" 2>&1)"
echo "$OUT" | grep -q "STALE" && ok "STALE — молчащий CI распознан" || bad "STALE не распознан: $OUT"
CODE="$(NOVA_CI_STALE_MIN=0 PATH="$TMP/bin:$PATH" bash "$GUARD" --strict "$SHA" >/dev/null 2>&1; echo $?)"
[ "$CODE" = "1" ] && ok "STALE — --strict роняет" || bad "STALE — --strict не уронил"
# И не лжёт: при огромном пороге тот же расклад ронять не должен.
CODE="$(NOVA_CI_STALE_MIN=999999 PATH="$TMP/bin:$PATH" bash "$GUARD" --strict "$SHA" >/dev/null 2>&1; echo $?)"
[ "$CODE" = "0" ] && ok "STALE — порог уважается (свежий коммит не роняет)" \
                  || bad "STALE — уронил при непройденном пороге (ложняк!)"

# ── 4. SKIP: gh недоступен → exit 0 и явная причина ──────────────────────────
rm -f "$TMP/bin/gh"
OUT="$(PATH="$TMP/bin:/usr/bin:/bin" bash "$GUARD" --strict "$SHA" 2>&1)"
CODE="$(PATH="$TMP/bin:/usr/bin:/bin" bash "$GUARD" --strict "$SHA" >/dev/null 2>&1; echo $?)"
echo "$OUT" | grep -q "SKIP" && [ "$CODE" = "0" ] \
    && ok "SKIP — без gh не роняет и объясняет причину" \
    || bad "SKIP — без gh повёл себя неверно (code=$CODE): $OUT"

if [ "$FAILED" -eq 0 ]; then
    echo "самотест check-ci-status: OK (RED/OK/STALE/SKIP — ловит и не лжёт)"
    exit 0
fi
echo "самотест check-ci-status: ПРОВАЛЕН"
exit 1
