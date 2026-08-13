#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# smoke.sh — one-command, no-secrets, no-network relay check for the
# http_proxy_chain flagship example (registry 221.1 #548).
#
# The `nova build --strict-effects` + `nova lint` gates only prove the
# example COMPILES; they do not exercise a single byte of the relay path
# (`pipe_bidirectional`/`pump`). Until now the only way to prove the relay
# actually WORKS was a live, password-protected SOCKS5 proxy on the
# internet — which nobody runs routinely, so a real relay regression
# (registry #548: the SOCKS5 tunnel was closed by the compiler the instant
# the handshake succeeded, before a single byte crossed it) sat on `main`
# behind a green gate. This script closes that gap: `local_socks5_stub.py`
# (same directory) is a minimal local SOCKS5 server + HTTP target, so the
# whole path — browser-shaped curl -> bridge -> SOCKS5 -> origin — runs on
# loopback only, with no external proxy and no credentials.
#
# Usage:
#   bash examples/flagship/http_proxy_chain/tools/smoke.sh
#
# Exits 0 and prints "SMOKE: PASS" iff BOTH request paths (the CONNECT path
# used for HTTPS, and the plain-HTTP-over-proxy path) deliver the target's
# exact response body end to end through the bridge. Any other outcome is
# `SMOKE: FAIL` with the observed curl/stub output attached, and a non-zero
# exit code.

set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXAMPLE_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
REPO_ROOT="$(cd "${EXAMPLE_DIR}/../../.." && pwd)"

SOCKS_PORT="${SMOKE_SOCKS_PORT:-18596}"
TARGET_PORT="${SMOKE_TARGET_PORT:-18597}"
BRIDGE_PORT="${SMOKE_BRIDGE_PORT:-18598}"

WORKDIR="$(mktemp -d)"
BRIDGE_BIN="${WORKDIR}/bridge"
case "$(uname -s 2>/dev/null || echo unknown)" in
    MINGW*|MSYS*|CYGWIN*) BRIDGE_BIN="${BRIDGE_BIN}.exe" ;;
esac

STUB_PID=""
BRIDGE_PID=""
FAIL=0

cleanup() {
    [ -n "${BRIDGE_PID}" ] && kill "${BRIDGE_PID}" >/dev/null 2>&1
    [ -n "${STUB_PID}" ] && kill "${STUB_PID}" >/dev/null 2>&1
    wait >/dev/null 2>&1
    rm -rf "${WORKDIR}"
}
trap cleanup EXIT

fail() {
    echo "SMOKE: FAIL — $1"
    FAIL=1
}

# ── Locate a `nova` binary ──────────────────────────────────────────────
NOVA_BIN="${NOVA_BIN:-}"
if [ -z "${NOVA_BIN}" ]; then
    for candidate in \
        "${REPO_ROOT}/nova-cli/target/release/nova.exe" \
        "${REPO_ROOT}/nova-cli/target/release/nova" \
        "${REPO_ROOT}/nova-cli/target/debug/nova.exe" \
        "${REPO_ROOT}/nova-cli/target/debug/nova"
    do
        if [ -x "${candidate}" ]; then
            NOVA_BIN="${candidate}"
            break
        fi
    done
fi
if [ -z "${NOVA_BIN}" ]; then
    command -v nova >/dev/null 2>&1 && NOVA_BIN="$(command -v nova)"
fi
if [ -z "${NOVA_BIN}" ]; then
    echo "SMOKE: FAIL — no nova binary found (build one: cd nova-cli && cargo build --release, or set NOVA_BIN)"
    exit 1
fi
echo "smoke: using nova binary: ${NOVA_BIN}"

# ── Build the bridge ─────────────────────────────────────────────────────
echo "smoke: building ${EXAMPLE_DIR}/src/main.nv"
if ! "${NOVA_BIN}" build "${EXAMPLE_DIR}/src/main.nv" --strict-effects -o "${BRIDGE_BIN}" >"${WORKDIR}/build.log" 2>&1; then
    cat "${WORKDIR}/build.log"
    echo "SMOKE: FAIL — build failed"
    exit 1
fi

# ── Start the local SOCKS5 stub + HTTP target ────────────────────────────
# `python3` first: on most Linux distributions (and therefore on CI) a bare
# `python` does not exist at all, and this smoke is meant to run in the gate on
# BOTH platforms. Windows/MSYS ships `python`, so the fallback keeps it working
# there.
# Проверяем ЗАПУСКОМ, а не наличием в PATH: в Windows есть псевдо-`python3`
# (App Execution Alias), который присутствует в PATH, но вместо интерпретатора
# печатает приглашение поставить Python из магазина. `command -v` его
# принимает — и смоук падал бы с пустым логом стенда.
PY_BIN=""
for _c in python3 python; do
    if "$_c" -c "pass" >/dev/null 2>&1; then PY_BIN="$_c"; break; fi
done
if [ -z "${PY_BIN}" ]; then
    echo "SMOKE: FAIL — neither python3 nor python found on PATH"
    exit 1
fi
"${PY_BIN}" "${SCRIPT_DIR}/local_socks5_stub.py" "${SOCKS_PORT}" "${TARGET_PORT}" \
    >"${WORKDIR}/stub.log" 2>&1 &
STUB_PID=$!
sleep 1

# ── Start the bridge, pointed at the local stub ──────────────────────────
SOCKS5_PROXY="127.0.0.1:${SOCKS_PORT}" "${BRIDGE_BIN}" "${BRIDGE_PORT}" \
    >"${WORKDIR}/bridge.log" 2>&1 &
BRIDGE_PID=$!
sleep 1

check_body() {
    local label="$1"; shift
    local body
    body="$("$@" 2>"${WORKDIR}/curl_${label}.err")"
    if [ "${body}" = "PROBE-OK" ]; then
        echo "smoke: ${label} path — PASS (body: ${body})"
    else
        fail "${label} path — expected body 'PROBE-OK', got '${body}'"
        echo "  --- curl stderr (${label}) ---"
        sed 's/^/  /' "${WORKDIR}/curl_${label}.err"
    fi
}

# Plain-HTTP-over-proxy path (Ф.3): curl sends an absolute-URI GET, no CONNECT.
check_body plain \
    curl -s -m 15 -x "http://127.0.0.1:${BRIDGE_PORT}" "http://127.0.0.1:${TARGET_PORT}/"

# CONNECT path (Ф.2, primary — what an HTTPS proxy setting actually uses):
# --proxytunnel forces CONNECT even for a plain http:// target URL.
check_body connect \
    curl -s -m 15 --proxytunnel -x "http://127.0.0.1:${BRIDGE_PORT}" "http://127.0.0.1:${TARGET_PORT}/"

if [ "${FAIL}" -ne 0 ]; then
    # Stop the writers BEFORE reading their logs. Both processes write to a
    # FILE, so their stdout is block-buffered, and a still-running process has
    # told the file nothing yet. Until 2026-08-13 this block read the logs with
    # the bridge still alive and printed an empty "bridge log" every time --
    # which was then read on CI as evidence that the bridge produced no output
    # at all (registry #591, #605). It produces plenty: it prints its listening
    # line at startup. The emptiness was ours, not the bridge's.
    [ -n "${BRIDGE_PID}" ] && kill "${BRIDGE_PID}" >/dev/null 2>&1
    [ -n "${STUB_PID}" ] && kill "${STUB_PID}" >/dev/null 2>&1
    # Give both a moment to flush and exit; `wait` cannot be used here because
    # these are background jobs of THIS shell and we still want the exit below.
    sleep 1
    echo "--- SOCKS5 stub log ---"
    cat "${WORKDIR}/stub.log"
    echo "--- bridge log ---"
    cat "${WORKDIR}/bridge.log"
    exit 1
fi

echo "SMOKE: PASS"
exit 0
