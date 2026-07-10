/* SPDX-License-Identifier: MIT OR Apache-2.0 */
/* Plan 116 Ф.2: prototypes границы std/tls ↔ nova_tls_shim (rustls staticlib).
 *
 * PURE PROTOTYPES — без зависимостей, безопасно включать безусловно из
 * nova_rt.h (без этого вызов tls_* в сгенерированном .c был бы implicit
 * declaration: возврат int (32 бита) → ТРУНКАЦИЯ указателя-хендла → SEGV;
 * пойман SEGV-локалайзером на Ф.2). ОПРЕДЕЛЕНИЯ живут в Rust-крейте
 * compiler-codegen/tls_shim (nova_tls_shim.lib / .a) — линкуется УСЛОВНО по
 * факту использования tls_* в CU (test_runner::c_file_uses_tls, механизм
 * brotli/D337); без собранной либы вместо неё компилируется nova_rt/tls_stub.c
 * (Q11-деградация: TLS_ERR_UNSUPPORTED, не link error).
 *
 * Контракт (single source of truth: tls_shim/src/lib.rs; Nova-сторона:
 * std/tls/ffi.nv):
 *   - хендлы — непрозрачные void* (config-билдер / rustls-сессия);
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

void*    tls_client_cfg_new(void);
void*    tls_server_cfg_new(void);
intptr_t tls_cfg_verify_system(void* c);
intptr_t tls_cfg_verify_pem(void* c, const uint8_t* pem, intptr_t len);
intptr_t tls_cfg_verify_pinned(void* c, const uint8_t* hashes, intptr_t count);
intptr_t tls_cfg_verify_insecure(void* c);
intptr_t tls_cfg_alpn_add(void* c, const uint8_t* proto, intptr_t len);
intptr_t tls_cfg_cert_key_pem(void* c, const uint8_t* cert, intptr_t clen,
                              const uint8_t* key, intptr_t klen);
intptr_t tls_cfg_client_auth_pem(void* c, const uint8_t* roots, intptr_t len,
                                 bool required);
void     tls_cfg_free(void* c);

/* ── Session lifecycle ───────────────────────────────────────────────────── */

void* tls_client_new(void* c, const uint8_t* sni, intptr_t sni_len,
                     intptr_t* out_err);
void* tls_server_new(void* c, intptr_t* out_err);
void  tls_free(void* h);

/* ── Handshake state machine (1/0) ───────────────────────────────────────── */

intptr_t tls_is_handshaking(void* h);
intptr_t tls_wants_read(void* h);
intptr_t tls_wants_write(void* h);

/* ── Traffic: ciphertext ↔ session ↔ plaintext ──────────────────────────── */

intptr_t tls_read_tls(void* h, const uint8_t* p, intptr_t len);
intptr_t tls_process(void* h);
intptr_t tls_write_tls(void* h, uint8_t* out, intptr_t cap);
/* n>0 = plaintext; 0 = пока нет данных; -1 = clean close_notify; <-1 = err. */
intptr_t tls_read_plain(void* h, uint8_t* out, intptr_t cap);
intptr_t tls_write_plain(void* h, const uint8_t* p, intptr_t len);
void     tls_send_close_notify(void* h);

/* ── Inspection ──────────────────────────────────────────────────────────── */

intptr_t tls_alpn(void* h, uint8_t* out, intptr_t cap);
intptr_t tls_version(void* h); /* 0x0303 / 0x0304 / 0 */
intptr_t tls_cipher_suite(void* h, uint8_t* out, intptr_t cap);
intptr_t tls_peer_cert_der(void* h, intptr_t i, uint8_t* out, intptr_t cap);

/* ── Error detail ────────────────────────────────────────────────────────── */

intptr_t tls_last_error_kind(void* h);
intptr_t tls_last_error(void* h, uint8_t* out, intptr_t cap);

#endif /* NOVA_TLS_SHIM_H */
