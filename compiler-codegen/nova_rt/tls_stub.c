/* SPDX-License-Identifier: MIT OR Apache-2.0
 * Plan 116 Ф.2 — Q11 feature-gate stubs для std/tls, когда staticlib
 * nova_tls_shim НЕ собран для хоста (зеркало brotli_shim feature-gate, D337).
 *
 * Компилируется в CU ТОЛЬКО когда сгенерированный .c использует tls_*-символы
 * И test_runner::detect_tls не нашёл реальную либу (target/tls-cache/ либо
 * compiler-codegen/tls_shim/target/release/). Никогда не line'уется вместе с
 * реальным шимом (symbol clash исключён взаимоисключающей веткой линковки).
 *
 * Поведение: конструкторы возвращают null + TLS_ERR_UNSUPPORTED (-11) в
 * out_err; сеттеры/операции возвращают TLS_ERR_UNSUPPORTED; текст ошибки
 * объясняет, как собрать шим. Nova-сторона классифицирует -11 в
 * TlsError.Internal("unsupported by tls shim: ...") — деградация, не
 * link-error (Q11). Контракт кодов: compiler-codegen/tls_shim/src/lib.rs.
 */

#include <stdint.h>
#include <string.h>
#include <stdbool.h>

#include "tls_shim.h" /* prototypes — сверка сигнатур заглушек с контрактом */

typedef intptr_t nova_int; /* int — signed address-sized (Plan 133) */

#define TLS_ERR_UNSUPPORTED ((nova_int)-11)

static const char TLS_STUB_MSG[] =
    "tls shim not built for this host (cd compiler-codegen/tls_shim && cargo build --release)";

/* ── Config builders ─────────────────────────────────────────────────────── */

void*    tls_client_cfg_new(void) { return 0; }
void*    tls_server_cfg_new(void) { return 0; }
nova_int tls_cfg_verify_system(void* c) { (void)c; return TLS_ERR_UNSUPPORTED; }
nova_int tls_cfg_verify_pem(void* c, const uint8_t* pem, nova_int len) {
    (void)c; (void)pem; (void)len; return TLS_ERR_UNSUPPORTED;
}
nova_int tls_cfg_verify_pinned(void* c, const uint8_t* hashes, nova_int count) {
    (void)c; (void)hashes; (void)count; return TLS_ERR_UNSUPPORTED;
}
nova_int tls_cfg_verify_insecure(void* c) { (void)c; return TLS_ERR_UNSUPPORTED; }
nova_int tls_cfg_alpn_add(void* c, const uint8_t* proto, nova_int len) {
    (void)c; (void)proto; (void)len; return TLS_ERR_UNSUPPORTED;
}
nova_int tls_cfg_cert_key_pem(void* c, const uint8_t* cert, nova_int clen,
                              const uint8_t* key, nova_int klen) {
    (void)c; (void)cert; (void)clen; (void)key; (void)klen; return TLS_ERR_UNSUPPORTED;
}
nova_int tls_cfg_client_auth_pem(void* c, const uint8_t* roots, nova_int len, bool required) {
    (void)c; (void)roots; (void)len; (void)required; return TLS_ERR_UNSUPPORTED;
}
void tls_cfg_free(void* c) { (void)c; }

/* ── Session lifecycle ───────────────────────────────────────────────────── */

void* tls_client_new(void* c, const uint8_t* sni, nova_int sni_len, nova_int* out_err) {
    (void)c; (void)sni; (void)sni_len;
    if (out_err) { *out_err = TLS_ERR_UNSUPPORTED; }
    return 0;
}
void* tls_server_new(void* c, nova_int* out_err) {
    (void)c;
    if (out_err) { *out_err = TLS_ERR_UNSUPPORTED; }
    return 0;
}
void tls_free(void* h) { (void)h; }

/* ── State machine ───────────────────────────────────────────────────────── */

nova_int tls_is_handshaking(void* h) { (void)h; return 0; }
nova_int tls_wants_read(void* h)     { (void)h; return 0; }
nova_int tls_wants_write(void* h)    { (void)h; return 0; }

/* ── Traffic ─────────────────────────────────────────────────────────────── */

nova_int tls_read_tls(void* h, const uint8_t* p, nova_int len) {
    (void)h; (void)p; (void)len; return TLS_ERR_UNSUPPORTED;
}
nova_int tls_process(void* h) { (void)h; return TLS_ERR_UNSUPPORTED; }
nova_int tls_write_tls(void* h, uint8_t* out, nova_int cap) {
    (void)h; (void)out; (void)cap; return TLS_ERR_UNSUPPORTED;
}
nova_int tls_read_plain(void* h, uint8_t* out, nova_int cap) {
    (void)h; (void)out; (void)cap; return TLS_ERR_UNSUPPORTED;
}
nova_int tls_write_plain(void* h, const uint8_t* p, nova_int len) {
    (void)h; (void)p; (void)len; return TLS_ERR_UNSUPPORTED;
}
void tls_send_close_notify(void* h) { (void)h; }

/* ── Inspection ──────────────────────────────────────────────────────────── */

nova_int tls_alpn(void* h, uint8_t* out, nova_int cap) {
    (void)h; (void)out; (void)cap; return 0;
}
nova_int tls_version(void* h) { (void)h; return 0; }
nova_int tls_cipher_suite(void* h, uint8_t* out, nova_int cap) {
    (void)h; (void)out; (void)cap; return 0;
}
nova_int tls_peer_cert_der(void* h, nova_int i, uint8_t* out, nova_int cap) {
    (void)h; (void)i; (void)out; (void)cap; return 0;
}

/* ── Error detail ────────────────────────────────────────────────────────── */

nova_int tls_last_error_kind(void* h) { (void)h; return TLS_ERR_UNSUPPORTED; }
nova_int tls_last_error(void* h, uint8_t* out, nova_int cap) {
    (void)h;
    nova_int len = (nova_int)(sizeof(TLS_STUB_MSG) - 1);
    if (out && cap > 0) {
        nova_int n = cap < len ? cap : len;
        memcpy(out, TLS_STUB_MSG, (size_t)n);
    }
    return len;
}
