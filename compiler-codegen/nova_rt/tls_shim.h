/* SPDX-License-Identifier: MIT OR Apache-2.0 */
/* Plan 195 Ф.1: prototypes границы std/tls ↔ nova_rt/tls_c_shim.c (mbedTLS
 * backend; заменяет Rust rustls-staticlib бэкенд плана 116 — см. плана 195
 * "TLS: rustls (Rust) → C-TLS-библиотека").
 *
 * PURE PROTOTYPES — без зависимостей, безопасно включать безусловно из
 * nova_rt.h (без этого вызов tls_* в сгенерированном .c был бы implicit
 * declaration: возврат int (32 бита) → ТРУНКАЦИЯ указателя-хендла → SEGV;
 * пойман SEGV-локалайзером на Ф.2 плана 116). ОПРЕДЕЛЕНИЯ живут в
 * compiler-codegen/nova_rt/tls_c_shim.c — компилируется УСЛОВНО по факту
 * использования tls_* в CU (test_runner::c_file_uses_tls, механизм
 * brotli/D337); тот же файл содержит Q11-деградацию (TLS_ERR_UNSUPPORTED,
 * не link error) для хостов без установленного mbedTLS — единый TU,
 * `#ifdef NOVA_USE_MBEDTLS` переключает real/stub (mirror brotli_shim.c).
 *
 * Контракт (single source of truth: tls_c_shim.c; Nova-сторона:
 * std/tls/ffi.nv):
 *   - хендлы — непрозрачные intptr_t, несущие указатель шима (config-
 *     билдер / rustls-сессия); intptr_t, не void*: Nova-newtype над int
 *     (CTlsHandle(int), brotli-прецедент) — см. std/tls/ffi.nv;
 *   - int = intptr_t (nova_int, Plan 133);
 *   - буферы (ptr, len) — шим копирует/потребляет в пределах вызова, Nova
 *     []u8 не удерживается;
 *   - <0 = стабильные коды TLS_ERR_* (-1 internal, -2 badarg, -3 cert-invalid,
 *     -4 cert-expired, -5 hostname-mismatch, -6 unsupported-version,
 *     -7 handshake, -8 alpn, -9 peer-misbehaved, -10 invalid-pem,
 *     -11 unsupported, -12 invalid-sni);
 *   - строковые выходы: возврат = ПОЛНАЯ длина, копируется min(cap, len).
 */
#ifndef NOVA_TLS_SHIM_H
#define NOVA_TLS_SHIM_H

#include <stdint.h>
#include <stdbool.h>

/* ── Config builders (эфемерные: *_new потребляет билдер, и на ошибке) ──── */

intptr_t tls_client_cfg_new(void);
intptr_t tls_server_cfg_new(void);
intptr_t tls_cfg_verify_system(intptr_t c);
intptr_t tls_cfg_verify_pem(intptr_t c, const uint8_t* pem, intptr_t len);
intptr_t tls_cfg_verify_pinned(intptr_t c, const uint8_t* hashes, intptr_t count);
intptr_t tls_cfg_verify_insecure(intptr_t c);
intptr_t tls_cfg_alpn_add(intptr_t c, const uint8_t* proto, intptr_t len);
intptr_t tls_cfg_cert_key_pem(intptr_t c, const uint8_t* cert, intptr_t clen,
                              const uint8_t* key, intptr_t klen);
intptr_t tls_cfg_client_auth_pem(intptr_t c, const uint8_t* roots, intptr_t len,
                                 bool required);
void     tls_cfg_free(intptr_t c);

/* ── Session lifecycle ───────────────────────────────────────────────────── */

intptr_t tls_client_new(intptr_t c, const uint8_t* sni, intptr_t sni_len,
                     intptr_t* out_err);
intptr_t tls_server_new(intptr_t c, intptr_t* out_err);
void  tls_free(intptr_t h);

/* ── Handshake state machine (1/0) ───────────────────────────────────────── */

intptr_t tls_is_handshaking(intptr_t h);
intptr_t tls_wants_read(intptr_t h);
intptr_t tls_wants_write(intptr_t h);

/* ── Traffic: ciphertext ↔ session ↔ plaintext ──────────────────────────── */

intptr_t tls_read_tls(intptr_t h, const uint8_t* p, intptr_t len);
intptr_t tls_process(intptr_t h);
intptr_t tls_write_tls(intptr_t h, uint8_t* out, intptr_t cap);
/* n>0 = plaintext; 0 = пока нет данных; -1 = clean close_notify; <-1 = err. */
intptr_t tls_read_plain(intptr_t h, uint8_t* out, intptr_t cap);
intptr_t tls_write_plain(intptr_t h, const uint8_t* p, intptr_t len);
void     tls_send_close_notify(intptr_t h);

/* ── Inspection ──────────────────────────────────────────────────────────── */

intptr_t tls_alpn(intptr_t h, uint8_t* out, intptr_t cap);
intptr_t tls_version(intptr_t h); /* 0x0303 / 0x0304 / 0 */
intptr_t tls_cipher_suite(intptr_t h, uint8_t* out, intptr_t cap);
intptr_t tls_peer_cert_der(intptr_t h, intptr_t i, uint8_t* out, intptr_t cap);

/* ── Error detail ────────────────────────────────────────────────────────── */

intptr_t tls_last_error_kind(intptr_t h);
intptr_t tls_last_error(intptr_t h, uint8_t* out, intptr_t cap);

#endif /* NOVA_TLS_SHIM_H */
