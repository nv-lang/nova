/* SPDX-License-Identifier: MIT OR Apache-2.0 */
/* Plan 195 Ф.1: C shim over mbedTLS — DEFINITIONS. Replaces the Rust
 * (rustls) staticlib backend (Plan 116) with a pure-C backend so std/tls
 * builds via clang + a prebuilt .lib, no cargo/Rust anywhere in the path.
 *
 * Mirrors the brotli_shim.c pattern (D337): compiled ONLY when the
 * generated `.c` references a tls_* symbol (test_runner.rs conditional-link
 * gate, `c_file_uses_tls`). Two modes, selected by one macro:
 *
 *   NOVA_USE_MBEDTLS defined   → real TLS over vcpkg-installed mbedTLS
 *                                (mbedtls.lib/mbedx509.lib/mbedcrypto.lib).
 *   NOVA_USE_MBEDTLS undefined → feature-gate STUBS (Q11): mbedTLS is not
 *                                available for this host, so *_new()
 *                                returns 0 + TLS_ERR_UNSUPPORTED and the
 *                                Nova wrapper degrades to
 *                                TlsError.Internal("unsupported…") at
 *                                runtime — NEVER a link error.
 *
 * Contract (single source of truth — mirrored on the Nova side by
 * std/tls/ffi.nv, and previously by tls_shim/src/lib.rs before this plan):
 *   - symbols `tls_*`, no vendor prefix (compiler-conventions §5а);
 *   - handles are opaque intptr_t values (Nova newtype over int, D133);
 *   - int on the boundary = nova_int = intptr_t (Plan 133);
 *   - buffers are (ptr, len) — the shim copies/consumes within the call,
 *     never retains a Nova `[]u8` past the call;
 *   - <0 returns are stable TLS_ERR_* codes (table in tls_shim.h);
 *   - string outputs: return = FULL length, copies min(cap, len).
 *
 * Sans-I/O design (Plan 116 D-блок A, unchanged by the backend swap): the
 * shim ONLY encrypts/decrypts/validates. ALL socket traffic is pumped on
 * the Nova side (std/tls/stream.nv) over the byte-surface `Net` effect.
 * mbedTLS's BIO callbacks (`nova_tls_send_cb`/`nova_tls_recv_cb`) read/write
 * two in-memory byte queues (`in_buf`/`out_buf`) rather than a real socket —
 * `tls_read_tls`/`tls_write_tls` are the Nova-side pump's only access to
 * those queues, so mbedTLS never blocks, never touches a file descriptor.
 *
 * VerificationMode.SystemRoots means "bundled Mozilla CA root list" (NOT
 * the OS certificate store) — this is the pre-existing contract from the
 * rustls/webpki-roots backend (see tls_mozilla_roots.h), preserved as-is so
 * behavior (incl. the T4-SystemRoots-NEG self-signed-rejection test) is
 * unchanged across the backend swap.
 */
#include "tls_shim.h"

#include <stdlib.h>
#include <string.h>

#ifdef NOVA_USE_MBEDTLS

#include "mbedtls/ssl.h"
#include "mbedtls/entropy.h"
#include "mbedtls/ctr_drbg.h"
#include "mbedtls/x509_crt.h"
#include "mbedtls/pk.h"
#include "mbedtls/error.h"
#include "mbedtls/sha256.h"
#include "tls_mozilla_roots.h"

typedef intptr_t nova_int;

/* ── Stable error codes (mirror std/tls/error.nv + tls_shim.h) ──────────── */

#define TLS_ERR_OK                    0
#define TLS_ERR_INTERNAL             (-1)
#define TLS_ERR_BADARG               (-2)
#define TLS_ERR_CERT_INVALID         (-3)
#define TLS_ERR_CERT_EXPIRED         (-4)
#define TLS_ERR_HOSTNAME_MISMATCH    (-5)
#define TLS_ERR_UNSUPPORTED_VERSION  (-6)
#define TLS_ERR_HANDSHAKE            (-7)
#define TLS_ERR_ALPN                 (-8)
#define TLS_ERR_PEER_MISBEHAVED      (-9)
#define TLS_ERR_INVALID_PEM          (-10)
#define TLS_ERR_UNSUPPORTED          (-11)
#define TLS_ERR_INVALID_SNI          (-12)
#define TLS_READ_CLOSE_NOTIFY        (-1)

enum {
    NOVA_TLS_VERIFY_SYSTEM = 0,
    NOVA_TLS_VERIFY_PEM = 1,
    NOVA_TLS_VERIFY_PINNED = 2,
    NOVA_TLS_VERIFY_INSECURE = 3,
};

enum {
    NOVA_TLS_CLIENT_AUTH_NONE = 0,
    NOVA_TLS_CLIENT_AUTH_OPTIONAL = 1,
    NOVA_TLS_CLIENT_AUTH_REQUIRED = 2,
};

/* ── Config builder: ephemeral, consumed by tls_client_new/tls_server_new ── */

typedef struct NovaTlsCfgBuilder {
    int is_server;
    int verify_mode;
    unsigned char *roots_pem;   size_t roots_len;   /* CustomRoots PEM (+NUL) */
    unsigned char (*pins)[32]; size_t pin_count;    /* Pinned SPKI SHA-256 */
    char **alpn;                size_t alpn_count;   /* NULL-terminated on grow */
    unsigned char *cert_pem;    size_t cert_len;     /* (+NUL) */
    unsigned char *key_pem;     size_t key_len;      /* (+NUL) */
    int client_auth_mode;
    unsigned char *client_auth_roots_pem; size_t client_auth_roots_len; /* (+NUL) */
} NovaTlsCfgBuilder;

/* ── Session: mbedTLS context + sans-I/O ciphertext queues ──────────────── */

typedef struct NovaTlsSession {
    mbedtls_ssl_context ssl;
    mbedtls_ssl_config conf;
    mbedtls_x509_crt ca_chain;
    mbedtls_x509_crt own_cert;
    mbedtls_pk_context own_key;
    mbedtls_entropy_context entropy;
    mbedtls_ctr_drbg_context ctr_drbg;
    char **alpn_storage;                 /* owned; NULL-terminated; freed at tls_free */
    unsigned char (*pins)[32];            /* Pinned SPKI SHA-256 pins (owned copy) */
    size_t pin_count;
    unsigned char *in_buf;  size_t in_len, in_cap, in_pos;  /* ciphertext IN (fed by tls_read_tls) */
    unsigned char *out_buf; size_t out_len, out_cap;        /* ciphertext OUT (drained by tls_write_tls) */
    int last_err_kind;
    char last_err[256];
} NovaTlsSession;

/* ── Small helpers ────────────────────────────────────────────────────────── */

static void nova_tls_buf_ensure(unsigned char **buf, size_t *cap, size_t need) {
    if (need <= *cap) { return; }
    size_t nc = *cap ? *cap : 4096;
    while (nc < need) { nc *= 2; }
    unsigned char *nb = (unsigned char *)realloc(*buf, nc);
    if (!nb) { return; } /* OOM: *cap unchanged, caller's growth check below will catch it */
    *buf = nb;
    *cap = nc;
}

static nova_int nova_tls_copy_out(const unsigned char *src, size_t srclen, uint8_t *out, nova_int cap) {
    if (out != NULL && cap > 0 && srclen > 0) {
        size_t n = srclen < (size_t)cap ? srclen : (size_t)cap;
        memcpy(out, src, n);
    }
    return (nova_int)srclen;
}

static void nova_tls_set_err(NovaTlsSession *s, int kind, const char *msg) {
    s->last_err_kind = kind;
    if (msg) {
        size_t n = strlen(msg);
        if (n >= sizeof(s->last_err)) { n = sizeof(s->last_err) - 1; }
        memcpy(s->last_err, msg, n);
        s->last_err[n] = '\0';
    } else {
        s->last_err[0] = '\0';
    }
}

static unsigned char *nova_tls_dup_nul(const unsigned char *p, size_t len) {
    unsigned char *out = (unsigned char *)malloc(len + 1);
    if (!out) { return NULL; }
    if (len) { memcpy(out, p, len); }
    out[len] = 0;
    return out;
}

/* ── DER TLV walk / SPKI extraction (SHA-256 pinning) ────────────────────────
 * Mirrors the previous rustls-shim's `spki_der()` (tls_shim/src/lib.rs,
 * cross-checked against openssl in cert_modes_test.nv): walks
 * Certificate ::= SEQUENCE { tbsCertificate, ... } then
 * tbsCertificate ::= SEQUENCE { version?, serial, signature, issuer,
 * validity, subject, subjectPublicKeyInfo, ... } and returns the RAW
 * subjectPublicKeyInfo TLV bytes exactly as they appear in the certificate
 * (not a re-serialization) — byte-identical to what `openssl x509 -pubkey`
 * extracts, so SPKI-SHA256 pins computed with openssl match here too. */

static int nova_der_tlv(const unsigned char *data, size_t len, unsigned char *tag,
                         size_t *content_off, size_t *content_len, size_t *tlv_len) {
    if (len < 2) { return 0; }
    unsigned char t = data[0];
    size_t b1 = data[1];
    size_t l, hdr;
    if (b1 < 0x80) {
        l = b1;
        hdr = 2;
    } else {
        size_t nbytes = b1 & 0x7f;
        if (nbytes == 0 || nbytes > 4 || len < 2 + nbytes) { return 0; }
        l = 0;
        for (size_t i = 0; i < nbytes; i++) { l = (l << 8) | data[2 + i]; }
        hdr = 2 + nbytes;
    }
    size_t end = hdr + l;
    if (end < hdr || end > len) { return 0; }
    *tag = t;
    *content_off = hdr;
    *content_len = l;
    *tlv_len = end;
    return 1;
}

static unsigned char *nova_spki_der(const unsigned char *cert, size_t cert_len, size_t *out_len) {
    unsigned char tag;
    size_t coff, clen, tlvlen;
    if (!nova_der_tlv(cert, cert_len, &tag, &coff, &clen, &tlvlen) || tag != 0x30) { return NULL; }
    const unsigned char *cert_body = cert + coff;
    size_t cert_body_len = clen;
    if (!nova_der_tlv(cert_body, cert_body_len, &tag, &coff, &clen, &tlvlen) || tag != 0x30) { return NULL; }
    const unsigned char *tbs = cert_body + coff;
    size_t tbs_len = clen;
    size_t off = 0;
    if (!nova_der_tlv(tbs + off, tbs_len - off, &tag, &coff, &clen, &tlvlen)) { return NULL; }
    if (tag == 0xA0) { off += tlvlen; } /* optional [0] version — skip if present */
    for (int i = 0; i < 5; i++) { /* serialNumber, signature, issuer, validity, subject */
        if (!nova_der_tlv(tbs + off, tbs_len - off, &tag, &coff, &clen, &tlvlen)) { return NULL; }
        off += tlvlen;
    }
    if (!nova_der_tlv(tbs + off, tbs_len - off, &tag, &coff, &clen, &tlvlen) || tag != 0x30) { return NULL; }
    unsigned char *out = (unsigned char *)malloc(tlvlen);
    if (!out) { return NULL; }
    memcpy(out, tbs + off, tlvlen);
    *out_len = tlvlen;
    return out;
}

/* ── SNI validation (mbedtls_ssl_set_hostname doesn't validate syntax) ───────
 * Accepts an IP literal, or a syntactically valid DNS name (dot-separated
 * labels, 1-63 chars each, alnum/hyphen, no leading/trailing hyphen). Close
 * enough to rustls's ServerName::try_from to keep shim_link_test.nv's
 * "bad name!" (space + '!') rejected with TLS_ERR_INVALID_SNI. */

static int nova_tls_is_ip_literal(const char *s, size_t len) {
    int dots = 0, colons = 0;
    for (size_t i = 0; i < len; i++) {
        char c = s[i];
        if (c == '.') { dots++; }
        else if (c == ':') { colons++; }
        else if (!((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F'))) { return 0; }
    }
    return (dots > 0 && colons == 0) || colons > 0;
}

static int nova_tls_label_ok(const char *s, size_t len) {
    if (len == 0 || len > 63) { return 0; }
    for (size_t i = 0; i < len; i++) {
        char c = s[i];
        int ok = (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '-';
        if (!ok) { return 0; }
    }
    if (s[0] == '-' || s[len - 1] == '-') { return 0; }
    return 1;
}

static int nova_tls_valid_sni(const char *s, size_t len) {
    if (len == 0 || len > 253) { return 0; }
    if (nova_tls_is_ip_literal(s, len)) { return 1; }
    size_t start = 0;
    for (size_t i = 0; i <= len; i++) {
        if (i == len || s[i] == '.') {
            if (!nova_tls_label_ok(s + start, i - start)) { return 0; }
            start = i + 1;
        }
    }
    return 1;
}

/* ── Sans-I/O BIO callbacks: in-memory ciphertext queues ─────────────────── */

static int nova_tls_send_cb(void *ctx, const unsigned char *buf, size_t len) {
    NovaTlsSession *s = (NovaTlsSession *)ctx;
    size_t before_cap = s->out_cap;
    nova_tls_buf_ensure(&s->out_buf, &s->out_cap, s->out_len + len);
    if (s->out_cap < s->out_len + len && s->out_cap == before_cap) {
        return MBEDTLS_ERR_SSL_INTERNAL_ERROR; /* OOM */
    }
    memcpy(s->out_buf + s->out_len, buf, len);
    s->out_len += len;
    return (int)len;
}

static int nova_tls_recv_cb(void *ctx, unsigned char *buf, size_t len) {
    NovaTlsSession *s = (NovaTlsSession *)ctx;
    size_t avail = s->in_len - s->in_pos;
    if (avail == 0 || len == 0) { return MBEDTLS_ERR_SSL_WANT_READ; }
    size_t n = avail < len ? avail : len;
    memcpy(buf, s->in_buf + s->in_pos, n);
    s->in_pos += n;
    return (int)n;
}

/* ── Error classification: mbedTLS rc → stable TLS_ERR_* ─────────────────── */

static int nova_tls_classify(int rc, mbedtls_ssl_context *ssl) {
    if (rc == MBEDTLS_ERR_X509_CERT_VERIFY_FAILED) {
        uint32_t flags = mbedtls_ssl_get_verify_result(ssl);
        if (flags & MBEDTLS_X509_BADCERT_CN_MISMATCH) { return TLS_ERR_HOSTNAME_MISMATCH; }
        if (flags & (MBEDTLS_X509_BADCERT_EXPIRED | MBEDTLS_X509_BADCERT_FUTURE)) { return TLS_ERR_CERT_EXPIRED; }
        return TLS_ERR_CERT_INVALID;
    }
    if (rc == MBEDTLS_ERR_SSL_NO_APPLICATION_PROTOCOL) { return TLS_ERR_ALPN; }
    if (rc == MBEDTLS_ERR_SSL_BAD_PROTOCOL_VERSION) { return TLS_ERR_UNSUPPORTED_VERSION; }
    if (rc == MBEDTLS_ERR_SSL_CONN_EOF) { return TLS_ERR_PEER_MISBEHAVED; }
    return TLS_ERR_HANDSHAKE;
}

static void nova_tls_set_err_from_rc(NovaTlsSession *s, int kind, int rc) {
    char buf[128];
    mbedtls_strerror(rc, buf, sizeof(buf));
    nova_tls_set_err(s, kind, buf);
}

/* ── Pinned verifier (SPKI SHA-256; Ф.4.3 parity) ─────────────────────────
 * authmode=OPTIONAL for Pinned sessions (see tls_client_new) — NOT NONE:
 * with VERIFY_NONE mbedTLS's mbedtls_ssl_verify_certificate() skips chain
 * verification ENTIRELY (short-circuits before ever building the chain),
 * so a custom f_vrfy callback registered via mbedtls_ssl_conf_verify would
 * simply never run (found by 195 Ф.1 test failure: pinned_wrong_pin still
 * completed the handshake). OPTIONAL performs verification (invoking this
 * callback per cert) but does not itself reject the handshake based on the
 * resulting flags — independent of that, a NONZERO return from THIS
 * callback is always fatal regardless of authmode, so we force-fail on pin
 * mismatch and force-accept (return 0) on match, ignoring whatever
 * chain-trust flags mbedTLS computed on its own (a self-signed leaf is
 * expected and fine here — pinning replaces chain trust AND hostname
 * verification, D-блок B). */
static int nova_tls_verify_pinned_cb(void *ctx, mbedtls_x509_crt *crt, int depth, uint32_t *flags) {
    NovaTlsSession *s = (NovaTlsSession *)ctx;
    (void)flags;
    if (depth != 0) { return 0; /* only the leaf is pinned; ignore intermediates */ }
    size_t spki_len = 0;
    unsigned char *spki = nova_spki_der(crt->raw.p, crt->raw.len, &spki_len);
    if (!spki) { return 1; /* can't parse -> fail closed */ }
    unsigned char digest[32];
    mbedtls_sha256(spki, spki_len, digest, 0);
    free(spki);
    for (size_t i = 0; i < s->pin_count; i++) {
        if (memcmp(digest, s->pins[i], 32) == 0) { return 0; }
    }
    return 1; /* no pin matched -> fatal, regardless of authmode */
}

/* ── Handshake stepping (shared by tls_client_new/tls_server_new/tls_process) ─
 * A single call drives mbedtls_ssl_handshake() as far as it can go given
 * whatever ciphertext is currently buffered in `in_buf`: it may emit a
 * flight into `out_buf` (via the send callback) and/or consume buffered
 * input (via the recv callback), stopping when it needs MORE input
 * (WANT_READ — not an error, the Nova-side pump feeds more and calls
 * again) or fails outright. WANT_WRITE should never happen (our send
 * callback never blocks) but is looped past defensively. */
static nova_int nova_tls_step(NovaTlsSession *s) {
    if (mbedtls_ssl_is_handshake_over(&s->ssl)) { return TLS_ERR_OK; }
    for (;;) {
        int rc = mbedtls_ssl_handshake(&s->ssl);
        if (rc == 0) { return TLS_ERR_OK; }
        if (rc == MBEDTLS_ERR_SSL_WANT_READ) { return TLS_ERR_OK; }
        if (rc == MBEDTLS_ERR_SSL_WANT_WRITE) { continue; }
        {
            int code = nova_tls_classify(rc, &s->ssl);
            nova_tls_set_err_from_rc(s, code, rc);
            return code;
        }
    }
}

/* ── mbedTLS bootstrap shared by client/server session construction ─────── */

static const char NOVA_TLS_ENTROPY_PERS[] = "nova_tls";

static int nova_tls_setup_common(NovaTlsSession *s, int endpoint) {
    mbedtls_ssl_config_init(&s->conf);
    mbedtls_entropy_init(&s->entropy);
    mbedtls_ctr_drbg_init(&s->ctr_drbg);
    mbedtls_ssl_init(&s->ssl);
    mbedtls_x509_crt_init(&s->ca_chain);
    mbedtls_x509_crt_init(&s->own_cert);
    mbedtls_pk_init(&s->own_key);

    int rc = mbedtls_ctr_drbg_seed(&s->ctr_drbg, mbedtls_entropy_func, &s->entropy,
                                    (const unsigned char *)NOVA_TLS_ENTROPY_PERS,
                                    sizeof(NOVA_TLS_ENTROPY_PERS) - 1);
    if (rc != 0) { return rc; }
    rc = mbedtls_ssl_config_defaults(&s->conf, endpoint, MBEDTLS_SSL_TRANSPORT_STREAM, MBEDTLS_SSL_PRESET_DEFAULT);
    if (rc != 0) { return rc; }
    mbedtls_ssl_conf_rng(&s->conf, mbedtls_ctr_drbg_random, &s->ctr_drbg);
    /* D-блок B (plan 116, unchanged): TLS 1.2 accepted, 1.0/1.1 rejected by
     * design. TLS 1.3 is negotiated when both peers support it (mbedTLS
     * 3.6's default build here has MBEDTLS_SSL_PROTO_TLS1_3 on) — no max
     * pinned, so 1.3 is preferred automatically when available. */
    mbedtls_ssl_conf_min_tls_version(&s->conf, MBEDTLS_SSL_VERSION_TLS1_2);
    return 0;
}

/* ── tls_free (also used as the single cleanup path for construction errors,
 *    since every mbedTLS sub-object is safe to free whether or not it was
 *    ever actually parsed/set up — mbedtls_x509_crt_init()'d-but-empty and
 *    mbedtls_ssl_init()'d-but-never-setup() contexts free cleanly). ────── */

void tls_free(intptr_t h) {
    NovaTlsSession *s = (NovaTlsSession *)(intptr_t)h;
    if (!s) { return; }
    mbedtls_ssl_free(&s->ssl);
    mbedtls_ssl_config_free(&s->conf);
    mbedtls_x509_crt_free(&s->ca_chain);
    mbedtls_x509_crt_free(&s->own_cert);
    mbedtls_pk_free(&s->own_key);
    mbedtls_ctr_drbg_free(&s->ctr_drbg);
    mbedtls_entropy_free(&s->entropy);
    if (s->alpn_storage) {
        for (size_t i = 0; s->alpn_storage[i]; i++) { free(s->alpn_storage[i]); }
        free(s->alpn_storage);
    }
    free(s->pins);
    free(s->in_buf);
    free(s->out_buf);
    free(s);
}

/* ── Config builders ──────────────────────────────────────────────────────── */

static NovaTlsCfgBuilder *nova_tls_cfg_new(int is_server) {
    NovaTlsCfgBuilder *b = (NovaTlsCfgBuilder *)calloc(1, sizeof(NovaTlsCfgBuilder));
    if (!b) { return NULL; }
    b->is_server = is_server;
    b->verify_mode = NOVA_TLS_VERIFY_SYSTEM;
    return b;
}

intptr_t tls_client_cfg_new(void) { return (intptr_t)nova_tls_cfg_new(0); }
intptr_t tls_server_cfg_new(void) { return (intptr_t)nova_tls_cfg_new(1); }

nova_int tls_cfg_verify_system(intptr_t c) {
    NovaTlsCfgBuilder *b = (NovaTlsCfgBuilder *)(intptr_t)c;
    if (!b) { return TLS_ERR_BADARG; }
    b->verify_mode = NOVA_TLS_VERIFY_SYSTEM;
    return TLS_ERR_OK;
}

nova_int tls_cfg_verify_pem(intptr_t c, const uint8_t *pem, nova_int len) {
    NovaTlsCfgBuilder *b = (NovaTlsCfgBuilder *)(intptr_t)c;
    if (!b || len < 0 || (pem == NULL && len != 0)) { return TLS_ERR_BADARG; }
    free(b->roots_pem);
    b->roots_pem = nova_tls_dup_nul(pem, (size_t)len);
    if (!b->roots_pem) { return TLS_ERR_INTERNAL; }
    b->roots_len = (size_t)len + 1; /* includes trailing NUL (mbedtls PEM contract) */
    b->verify_mode = NOVA_TLS_VERIFY_PEM;
    return TLS_ERR_OK;
}

nova_int tls_cfg_verify_pinned(intptr_t c, const uint8_t *hashes, nova_int count) {
    NovaTlsCfgBuilder *b = (NovaTlsCfgBuilder *)(intptr_t)c;
    if (!b || count <= 0 || hashes == NULL) { return TLS_ERR_BADARG; }
    unsigned char (*pins)[32] = (unsigned char (*)[32])malloc((size_t)count * 32);
    if (!pins) { return TLS_ERR_INTERNAL; }
    memcpy(pins, hashes, (size_t)count * 32);
    free(b->pins);
    b->pins = pins;
    b->pin_count = (size_t)count;
    b->verify_mode = NOVA_TLS_VERIFY_PINNED;
    return TLS_ERR_OK;
}

nova_int tls_cfg_verify_insecure(intptr_t c) {
    NovaTlsCfgBuilder *b = (NovaTlsCfgBuilder *)(intptr_t)c;
    if (!b) { return TLS_ERR_BADARG; }
    b->verify_mode = NOVA_TLS_VERIFY_INSECURE;
    return TLS_ERR_OK;
}

nova_int tls_cfg_alpn_add(intptr_t c, const uint8_t *proto, nova_int len) {
    NovaTlsCfgBuilder *b = (NovaTlsCfgBuilder *)(intptr_t)c;
    if (!b || len <= 0 || proto == NULL) { return TLS_ERR_BADARG; }
    char **na = (char **)realloc(b->alpn, (b->alpn_count + 2) * sizeof(char *));
    if (!na) { return TLS_ERR_INTERNAL; }
    b->alpn = na;
    char *str = (char *)malloc((size_t)len + 1);
    if (!str) { return TLS_ERR_INTERNAL; }
    memcpy(str, proto, (size_t)len);
    str[len] = 0;
    b->alpn[b->alpn_count] = str;
    b->alpn_count++;
    b->alpn[b->alpn_count] = NULL;
    return TLS_ERR_OK;
}

nova_int tls_cfg_cert_key_pem(intptr_t c, const uint8_t *cert, nova_int clen,
                               const uint8_t *key, nova_int klen) {
    NovaTlsCfgBuilder *b = (NovaTlsCfgBuilder *)(intptr_t)c;
    if (!b || clen <= 0 || klen <= 0 || cert == NULL || key == NULL) { return TLS_ERR_BADARG; }
    unsigned char *cert_copy = nova_tls_dup_nul(cert, (size_t)clen);
    unsigned char *key_copy = nova_tls_dup_nul(key, (size_t)klen);
    if (!cert_copy || !key_copy) {
        free(cert_copy);
        free(key_copy);
        return TLS_ERR_INTERNAL;
    }
    free(b->cert_pem);
    free(b->key_pem);
    b->cert_pem = cert_copy;
    b->cert_len = (size_t)clen + 1;
    b->key_pem = key_copy;
    b->key_len = (size_t)klen + 1;
    return TLS_ERR_OK;
}

nova_int tls_cfg_client_auth_pem(intptr_t c, const uint8_t *roots, nova_int len, bool required) {
    NovaTlsCfgBuilder *b = (NovaTlsCfgBuilder *)(intptr_t)c;
    if (!b || len <= 0 || roots == NULL) { return TLS_ERR_BADARG; }
    free(b->client_auth_roots_pem);
    b->client_auth_roots_pem = nova_tls_dup_nul(roots, (size_t)len);
    if (!b->client_auth_roots_pem) { return TLS_ERR_INTERNAL; }
    b->client_auth_roots_len = (size_t)len + 1;
    b->client_auth_mode = required ? NOVA_TLS_CLIENT_AUTH_REQUIRED : NOVA_TLS_CLIENT_AUTH_OPTIONAL;
    return TLS_ERR_OK;
}

static void nova_tls_cfg_free_fields(NovaTlsCfgBuilder *b) {
    free(b->roots_pem);
    free(b->pins);
    if (b->alpn) {
        for (size_t i = 0; i < b->alpn_count; i++) { free(b->alpn[i]); }
        free(b->alpn);
    }
    free(b->cert_pem);
    free(b->key_pem);
    free(b->client_auth_roots_pem);
}

void tls_cfg_free(intptr_t c) {
    NovaTlsCfgBuilder *b = (NovaTlsCfgBuilder *)(intptr_t)c;
    if (!b) { return; }
    nova_tls_cfg_free_fields(b);
    free(b);
}

/* ── Session construction (config builder is CONSUMED — freed either way) ── */

intptr_t tls_client_new(intptr_t c, const uint8_t *sni, intptr_t sni_len, intptr_t *out_err) {
    NovaTlsCfgBuilder *b = (NovaTlsCfgBuilder *)(intptr_t)c;
    if (!b) {
        if (out_err) { *out_err = TLS_ERR_BADARG; }
        return 0;
    }
    if (b->is_server) {
        nova_tls_cfg_free_fields(b);
        free(b);
        if (out_err) { *out_err = TLS_ERR_BADARG; }
        return 0;
    }
    if (!sni || sni_len <= 0 || !nova_tls_valid_sni((const char *)sni, (size_t)sni_len)) {
        nova_tls_cfg_free_fields(b);
        free(b);
        if (out_err) { *out_err = TLS_ERR_INVALID_SNI; }
        return 0;
    }
    char *sni_cstr = (char *)malloc((size_t)sni_len + 1);
    if (!sni_cstr) {
        nova_tls_cfg_free_fields(b);
        free(b);
        if (out_err) { *out_err = TLS_ERR_INTERNAL; }
        return 0;
    }
    memcpy(sni_cstr, sni, (size_t)sni_len);
    sni_cstr[sni_len] = 0;

    NovaTlsSession *s = (NovaTlsSession *)calloc(1, sizeof(NovaTlsSession));
    if (!s) {
        free(sni_cstr);
        nova_tls_cfg_free_fields(b);
        free(b);
        if (out_err) { *out_err = TLS_ERR_INTERNAL; }
        return 0;
    }

    int rc = nova_tls_setup_common(s, MBEDTLS_SSL_IS_CLIENT);
    int err_code = TLS_ERR_INTERNAL;
    int need_hostname = 0;

    if (rc == 0) {
        if (b->verify_mode == NOVA_TLS_VERIFY_SYSTEM) {
            rc = mbedtls_x509_crt_parse(&s->ca_chain, (const unsigned char *)NOVA_TLS_MOZILLA_ROOTS_PEM,
                                         sizeof(NOVA_TLS_MOZILLA_ROOTS_PEM));
            if (rc < 0) {
                err_code = TLS_ERR_INTERNAL;
            } else {
                mbedtls_ssl_conf_ca_chain(&s->conf, &s->ca_chain, NULL);
                mbedtls_ssl_conf_authmode(&s->conf, MBEDTLS_SSL_VERIFY_REQUIRED);
                need_hostname = 1;
                rc = 0;
            }
        } else if (b->verify_mode == NOVA_TLS_VERIFY_PEM) {
            rc = mbedtls_x509_crt_parse(&s->ca_chain, b->roots_pem, b->roots_len);
            if (rc != 0) {
                err_code = TLS_ERR_INVALID_PEM;
            } else {
                mbedtls_ssl_conf_ca_chain(&s->conf, &s->ca_chain, NULL);
                mbedtls_ssl_conf_authmode(&s->conf, MBEDTLS_SSL_VERIFY_REQUIRED);
                need_hostname = 1;
            }
        } else if (b->verify_mode == NOVA_TLS_VERIFY_PINNED) {
            if (b->pin_count == 0) {
                rc = -1;
                err_code = TLS_ERR_BADARG;
            } else {
                s->pins = (unsigned char (*)[32])malloc(b->pin_count * 32);
                if (!s->pins) {
                    rc = -1;
                    err_code = TLS_ERR_INTERNAL;
                } else {
                    memcpy(s->pins, b->pins, b->pin_count * 32);
                    s->pin_count = b->pin_count;
                    mbedtls_ssl_conf_authmode(&s->conf, MBEDTLS_SSL_VERIFY_OPTIONAL);
                    mbedtls_ssl_conf_verify(&s->conf, nova_tls_verify_pinned_cb, s);
                }
            }
        } else { /* INSECURE */
            mbedtls_ssl_conf_authmode(&s->conf, MBEDTLS_SSL_VERIFY_NONE);
        }

        if (rc == 0 && b->alpn_count > 0) {
            s->alpn_storage = b->alpn; /* move ownership */
            b->alpn = NULL;
            b->alpn_count = 0;
            rc = mbedtls_ssl_conf_alpn_protocols(&s->conf, (const char **)s->alpn_storage);
            if (rc != 0) { err_code = TLS_ERR_ALPN; }
        }

        if (rc == 0 && b->cert_pem && b->cert_len > 1) {
            rc = mbedtls_x509_crt_parse(&s->own_cert, b->cert_pem, b->cert_len);
            if (rc == 0) {
                rc = mbedtls_pk_parse_key(&s->own_key, b->key_pem, b->key_len, NULL, 0,
                                          mbedtls_ctr_drbg_random, &s->ctr_drbg);
            }
            if (rc != 0) {
                err_code = TLS_ERR_INVALID_PEM;
            } else {
                rc = mbedtls_ssl_conf_own_cert(&s->conf, &s->own_cert, &s->own_key);
                if (rc != 0) { err_code = TLS_ERR_INTERNAL; }
            }
        }

        if (rc == 0) {
            rc = mbedtls_ssl_setup(&s->ssl, &s->conf);
            if (rc != 0) { err_code = TLS_ERR_INTERNAL; }
        }
        if (rc == 0 && need_hostname) {
            rc = mbedtls_ssl_set_hostname(&s->ssl, sni_cstr);
            if (rc != 0) { err_code = TLS_ERR_INTERNAL; }
        }
        if (rc == 0) {
            mbedtls_ssl_set_bio(&s->ssl, s, nova_tls_send_cb, nova_tls_recv_cb, NULL);
        }
    } else {
        err_code = TLS_ERR_INTERNAL;
    }

    free(sni_cstr);
    nova_tls_cfg_free_fields(b);
    free(b);

    if (rc != 0) {
        if (out_err) { *out_err = err_code; }
        tls_free((intptr_t)s);
        return 0;
    }

    nova_tls_step(s); /* eagerly produce ClientHello (matches prior rustls behavior) */
    return (intptr_t)s;
}

intptr_t tls_server_new(intptr_t c, intptr_t *out_err) {
    NovaTlsCfgBuilder *b = (NovaTlsCfgBuilder *)(intptr_t)c;
    if (!b) {
        if (out_err) { *out_err = TLS_ERR_BADARG; }
        return 0;
    }
    if (!b->is_server || !b->cert_pem || !b->key_pem || b->cert_len <= 1 || b->key_len <= 1) {
        nova_tls_cfg_free_fields(b);
        free(b);
        if (out_err) { *out_err = TLS_ERR_BADARG; }
        return 0;
    }

    NovaTlsSession *s = (NovaTlsSession *)calloc(1, sizeof(NovaTlsSession));
    if (!s) {
        nova_tls_cfg_free_fields(b);
        free(b);
        if (out_err) { *out_err = TLS_ERR_INTERNAL; }
        return 0;
    }

    int rc = nova_tls_setup_common(s, MBEDTLS_SSL_IS_SERVER);
    int err_code = TLS_ERR_INTERNAL;

    if (rc == 0) {
        rc = mbedtls_x509_crt_parse(&s->own_cert, b->cert_pem, b->cert_len);
        if (rc == 0) {
            rc = mbedtls_pk_parse_key(&s->own_key, b->key_pem, b->key_len, NULL, 0,
                                      mbedtls_ctr_drbg_random, &s->ctr_drbg);
        }
        if (rc != 0) {
            err_code = TLS_ERR_INVALID_PEM;
        } else {
            rc = mbedtls_ssl_conf_own_cert(&s->conf, &s->own_cert, &s->own_key);
            if (rc != 0) { err_code = TLS_ERR_INTERNAL; }
        }

        if (rc == 0 && b->client_auth_mode != NOVA_TLS_CLIENT_AUTH_NONE) {
            rc = mbedtls_x509_crt_parse(&s->ca_chain, b->client_auth_roots_pem, b->client_auth_roots_len);
            if (rc != 0) {
                err_code = TLS_ERR_INVALID_PEM;
            } else {
                mbedtls_ssl_conf_ca_chain(&s->conf, &s->ca_chain, NULL);
                mbedtls_ssl_conf_authmode(&s->conf,
                    b->client_auth_mode == NOVA_TLS_CLIENT_AUTH_REQUIRED
                        ? MBEDTLS_SSL_VERIFY_REQUIRED : MBEDTLS_SSL_VERIFY_OPTIONAL);
            }
        }

        if (rc == 0 && b->alpn_count > 0) {
            s->alpn_storage = b->alpn;
            b->alpn = NULL;
            b->alpn_count = 0;
            rc = mbedtls_ssl_conf_alpn_protocols(&s->conf, (const char **)s->alpn_storage);
            if (rc != 0) { err_code = TLS_ERR_ALPN; }
        }

        if (rc == 0) {
            rc = mbedtls_ssl_setup(&s->ssl, &s->conf);
            if (rc != 0) { err_code = TLS_ERR_INTERNAL; }
        }
        if (rc == 0) {
            mbedtls_ssl_set_bio(&s->ssl, s, nova_tls_send_cb, nova_tls_recv_cb, NULL);
        }
    } else {
        err_code = TLS_ERR_INTERNAL;
    }

    nova_tls_cfg_free_fields(b);
    free(b);

    if (rc != 0) {
        if (out_err) { *out_err = err_code; }
        tls_free((intptr_t)s);
        return 0;
    }

    nova_tls_step(s); /* server has nothing to send yet; will WANT_READ for ClientHello */
    return (intptr_t)s;
}

/* ── Handshake state machine predicates (1/0) ────────────────────────────── */

nova_int tls_is_handshaking(intptr_t h) {
    NovaTlsSession *s = (NovaTlsSession *)(intptr_t)h;
    if (!s) { return 0; }
    return mbedtls_ssl_is_handshake_over(&s->ssl) ? 0 : 1;
}

nova_int tls_wants_read(intptr_t h) {
    NovaTlsSession *s = (NovaTlsSession *)(intptr_t)h;
    if (!s) { return 0; }
    if (s->out_len > 0) { return 0; }
    return mbedtls_ssl_is_handshake_over(&s->ssl) ? 0 : 1;
}

nova_int tls_wants_write(intptr_t h) {
    NovaTlsSession *s = (NovaTlsSession *)(intptr_t)h;
    if (!s) { return 0; }
    return s->out_len > 0 ? 1 : 0;
}

/* ── Traffic: ciphertext ↔ session ↔ plaintext ───────────────────────────── */

nova_int tls_read_tls(intptr_t h, const uint8_t *p, nova_int len) {
    NovaTlsSession *s = (NovaTlsSession *)(intptr_t)h;
    if (!s) { return TLS_ERR_BADARG; }
    if (len < 0 || (p == NULL && len != 0)) {
        nova_tls_set_err(s, TLS_ERR_BADARG, "bad buffer");
        return TLS_ERR_BADARG;
    }
    if (len == 0) { return 0; }
    if (s->in_pos > 0) {
        size_t rem = s->in_len - s->in_pos;
        if (rem > 0) { memmove(s->in_buf, s->in_buf + s->in_pos, rem); }
        s->in_len = rem;
        s->in_pos = 0;
    }
    size_t before_cap = s->in_cap;
    nova_tls_buf_ensure(&s->in_buf, &s->in_cap, s->in_len + (size_t)len);
    if (s->in_cap < s->in_len + (size_t)len && s->in_cap == before_cap) {
        nova_tls_set_err(s, TLS_ERR_INTERNAL, "out of memory");
        return TLS_ERR_INTERNAL;
    }
    memcpy(s->in_buf + s->in_len, p, (size_t)len);
    s->in_len += (size_t)len;
    return len; /* always accepts everything in one call */
}

nova_int tls_process(intptr_t h) {
    NovaTlsSession *s = (NovaTlsSession *)(intptr_t)h;
    if (!s) { return TLS_ERR_BADARG; }
    if (!mbedtls_ssl_is_handshake_over(&s->ssl)) { return nova_tls_step(s); }
    return TLS_ERR_OK; /* app-data decrypt happens lazily inside tls_read_plain */
}

nova_int tls_write_tls(intptr_t h, uint8_t *out, nova_int cap) {
    NovaTlsSession *s = (NovaTlsSession *)(intptr_t)h;
    if (!s) { return TLS_ERR_BADARG; }
    if (out == NULL || cap <= 0) {
        nova_tls_set_err(s, TLS_ERR_BADARG, "bad buffer");
        return TLS_ERR_BADARG;
    }
    if (s->out_len == 0) { return 0; }
    size_t n = s->out_len < (size_t)cap ? s->out_len : (size_t)cap;
    memcpy(out, s->out_buf, n);
    if (n < s->out_len) { memmove(s->out_buf, s->out_buf + n, s->out_len - n); }
    s->out_len -= n;
    return (nova_int)n;
}

nova_int tls_read_plain(intptr_t h, uint8_t *out, nova_int cap) {
    NovaTlsSession *s = (NovaTlsSession *)(intptr_t)h;
    if (!s) { return TLS_ERR_BADARG; }
    if (out == NULL || cap <= 0) {
        nova_tls_set_err(s, TLS_ERR_BADARG, "bad buffer");
        return TLS_ERR_BADARG;
    }
    int rc = mbedtls_ssl_read(&s->ssl, out, (size_t)cap);
    if (rc > 0) { return (nova_int)rc; }
    if (rc == MBEDTLS_ERR_SSL_PEER_CLOSE_NOTIFY) { return TLS_READ_CLOSE_NOTIFY; }
    if (rc == 0 || rc == MBEDTLS_ERR_SSL_WANT_READ) { return 0; }
    {
        int code = nova_tls_classify(rc, &s->ssl);
        nova_tls_set_err_from_rc(s, code, rc);
        return code;
    }
}

nova_int tls_write_plain(intptr_t h, const uint8_t *p, nova_int len) {
    NovaTlsSession *s = (NovaTlsSession *)(intptr_t)h;
    if (!s) { return TLS_ERR_BADARG; }
    if (len < 0 || (p == NULL && len != 0)) {
        nova_tls_set_err(s, TLS_ERR_BADARG, "bad buffer");
        return TLS_ERR_BADARG;
    }
    if (len == 0) { return 0; }
    int rc = mbedtls_ssl_write(&s->ssl, p, (size_t)len);
    if (rc >= 0) { return (nova_int)rc; }
    if (rc == MBEDTLS_ERR_SSL_WANT_READ || rc == MBEDTLS_ERR_SSL_WANT_WRITE) {
        /* Should not happen: our BIO callbacks never block. Surface as an
         * error rather than 0 (Nova's write_all treats n<=0 as failure). */
        nova_tls_set_err(s, TLS_ERR_INTERNAL, "unexpected WANT_READ/WRITE from mbedtls_ssl_write");
        return TLS_ERR_INTERNAL;
    }
    {
        int code = nova_tls_classify(rc, &s->ssl);
        nova_tls_set_err_from_rc(s, code, rc);
        return code;
    }
}

void tls_send_close_notify(intptr_t h) {
    NovaTlsSession *s = (NovaTlsSession *)(intptr_t)h;
    if (!s) { return; }
    for (int i = 0; i < 4; i++) {
        int rc = mbedtls_ssl_close_notify(&s->ssl);
        if (rc != MBEDTLS_ERR_SSL_WANT_WRITE) { break; }
    }
}

/* ── Inspection (post-handshake) ──────────────────────────────────────────── */

nova_int tls_alpn(intptr_t h, uint8_t *out, nova_int cap) {
    NovaTlsSession *s = (NovaTlsSession *)(intptr_t)h;
    if (!s) { return TLS_ERR_BADARG; }
    const char *p = mbedtls_ssl_get_alpn_protocol(&s->ssl);
    if (!p) { return 0; }
    return nova_tls_copy_out((const unsigned char *)p, strlen(p), out, cap);
}

nova_int tls_version(intptr_t h) {
    NovaTlsSession *s = (NovaTlsSession *)(intptr_t)h;
    if (!s) { return TLS_ERR_BADARG; }
    if (!mbedtls_ssl_is_handshake_over(&s->ssl)) { return 0; }
    /* MBEDTLS_SSL_VERSION_TLS1_2/1_3 enum values ARE the wire codes 0x0303/0x0304. */
    return (nova_int)mbedtls_ssl_get_version_number(&s->ssl);
}

nova_int tls_cipher_suite(intptr_t h, uint8_t *out, nova_int cap) {
    NovaTlsSession *s = (NovaTlsSession *)(intptr_t)h;
    if (!s) { return TLS_ERR_BADARG; }
    const char *name = mbedtls_ssl_get_ciphersuite(&s->ssl);
    if (!name) { return 0; }
    return nova_tls_copy_out((const unsigned char *)name, strlen(name), out, cap);
}

nova_int tls_peer_cert_der(intptr_t h, nova_int i, uint8_t *out, nova_int cap) {
    NovaTlsSession *s = (NovaTlsSession *)(intptr_t)h;
    if (!s) { return TLS_ERR_BADARG; }
    if (i < 0) { return 0; }
    const mbedtls_x509_crt *cert = mbedtls_ssl_get_peer_cert(&s->ssl);
    for (nova_int k = 0; cert && k < i; k++) { cert = cert->next; }
    if (!cert) { return 0; }
    return nova_tls_copy_out(cert->raw.p, cert->raw.len, out, cap);
}

/* ── Error detail ─────────────────────────────────────────────────────────── */

nova_int tls_last_error_kind(intptr_t h) {
    NovaTlsSession *s = (NovaTlsSession *)(intptr_t)h;
    return s ? s->last_err_kind : TLS_ERR_BADARG;
}

nova_int tls_last_error(intptr_t h, uint8_t *out, nova_int cap) {
    NovaTlsSession *s = (NovaTlsSession *)(intptr_t)h;
    if (!s) { return TLS_ERR_BADARG; }
    return nova_tls_copy_out((const unsigned char *)s->last_err, strlen(s->last_err), out, cap);
}

#else /* !NOVA_USE_MBEDTLS — Q11 feature-gate stubs (mbedTLS not built for this host) */

typedef intptr_t nova_int;

#define TLS_ERR_UNSUPPORTED ((nova_int)-11)

static const char TLS_STUB_MSG[] =
    "tls shim not built for this host (mbedTLS not found — see compiler-codegen/vcpkg.json)";

intptr_t tls_client_cfg_new(void) { return 0; }
intptr_t tls_server_cfg_new(void) { return 0; }
nova_int tls_cfg_verify_system(intptr_t c) { (void)c; return TLS_ERR_UNSUPPORTED; }
nova_int tls_cfg_verify_pem(intptr_t c, const uint8_t *pem, nova_int len) {
    (void)c; (void)pem; (void)len; return TLS_ERR_UNSUPPORTED;
}
nova_int tls_cfg_verify_pinned(intptr_t c, const uint8_t *hashes, nova_int count) {
    (void)c; (void)hashes; (void)count; return TLS_ERR_UNSUPPORTED;
}
nova_int tls_cfg_verify_insecure(intptr_t c) { (void)c; return TLS_ERR_UNSUPPORTED; }
nova_int tls_cfg_alpn_add(intptr_t c, const uint8_t *proto, nova_int len) {
    (void)c; (void)proto; (void)len; return TLS_ERR_UNSUPPORTED;
}
nova_int tls_cfg_cert_key_pem(intptr_t c, const uint8_t *cert, nova_int clen,
                              const uint8_t *key, nova_int klen) {
    (void)c; (void)cert; (void)clen; (void)key; (void)klen; return TLS_ERR_UNSUPPORTED;
}
nova_int tls_cfg_client_auth_pem(intptr_t c, const uint8_t *roots, nova_int len, bool required) {
    (void)c; (void)roots; (void)len; (void)required; return TLS_ERR_UNSUPPORTED;
}
void tls_cfg_free(intptr_t c) { (void)c; }

intptr_t tls_client_new(intptr_t c, const uint8_t *sni, nova_int sni_len, nova_int *out_err) {
    (void)c; (void)sni; (void)sni_len;
    if (out_err) { *out_err = TLS_ERR_UNSUPPORTED; }
    return 0;
}
intptr_t tls_server_new(intptr_t c, nova_int *out_err) {
    (void)c;
    if (out_err) { *out_err = TLS_ERR_UNSUPPORTED; }
    return 0;
}
void tls_free(intptr_t h) { (void)h; }

nova_int tls_is_handshaking(intptr_t h) { (void)h; return 0; }
nova_int tls_wants_read(intptr_t h)     { (void)h; return 0; }
nova_int tls_wants_write(intptr_t h)    { (void)h; return 0; }

nova_int tls_read_tls(intptr_t h, const uint8_t *p, nova_int len) {
    (void)h; (void)p; (void)len; return TLS_ERR_UNSUPPORTED;
}
nova_int tls_process(intptr_t h) { (void)h; return TLS_ERR_UNSUPPORTED; }
nova_int tls_write_tls(intptr_t h, uint8_t *out, nova_int cap) {
    (void)h; (void)out; (void)cap; return TLS_ERR_UNSUPPORTED;
}
nova_int tls_read_plain(intptr_t h, uint8_t *out, nova_int cap) {
    (void)h; (void)out; (void)cap; return TLS_ERR_UNSUPPORTED;
}
nova_int tls_write_plain(intptr_t h, const uint8_t *p, nova_int len) {
    (void)h; (void)p; (void)len; return TLS_ERR_UNSUPPORTED;
}
void tls_send_close_notify(intptr_t h) { (void)h; }

nova_int tls_alpn(intptr_t h, uint8_t *out, nova_int cap) {
    (void)h; (void)out; (void)cap; return 0;
}
nova_int tls_version(intptr_t h) { (void)h; return 0; }
nova_int tls_cipher_suite(intptr_t h, uint8_t *out, nova_int cap) {
    (void)h; (void)out; (void)cap; return 0;
}
nova_int tls_peer_cert_der(intptr_t h, nova_int i, uint8_t *out, nova_int cap) {
    (void)h; (void)i; (void)out; (void)cap; return 0;
}

nova_int tls_last_error_kind(intptr_t h) { (void)h; return TLS_ERR_UNSUPPORTED; }
nova_int tls_last_error(intptr_t h, uint8_t *out, nova_int cap) {
    (void)h;
    nova_int len = (nova_int)(sizeof(TLS_STUB_MSG) - 1);
    if (out && cap > 0) {
        nova_int n = cap < len ? cap : len;
        memcpy(out, TLS_STUB_MSG, (size_t)n);
    }
    return len;
}

#endif /* NOVA_USE_MBEDTLS */
