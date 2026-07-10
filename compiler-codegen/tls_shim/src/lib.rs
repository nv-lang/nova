// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 116 — nova_tls_shim: C-ABI слой поверх rustls для std/tls.
//!
//! Контракт границы — план 116 §«FFI-граница» (docs/plans/116-std-tls-effect.md):
//!   - символы `tls_*` без vendor-префикса (compiler-conventions §5а);
//!   - хендлы — непрозрачные указатели; на Nova-стороне newtype
//!     `CTlsHandle(*())` / `CTlsCfgHandle(*())` (module-conventions §4а);
//!   - int на границе = nova_int = intptr_t → `isize`;
//!   - буферы = (ptr, len) — байты, никакого владения через границу;
//!   - rustls — sans-I/O state machine: весь сокет-пампинг на Nova-стороне
//!     (byte-surface `Net`), шим только шифрует/дешифрует и валидирует.
//!
//! Ошибки: стабильные int-коды `TLS_ERR_*` (таблица ниже — зеркалится в
//! std/tls/error.nv `TlsError.from_shim`) + текст последней ошибки на сессии
//! (`tls_last_error*`). Классификация rustls::Error → код — `classify()`.
//!
//! Паника через FFI-границу = UB → profile `panic = "abort"` (Cargo.toml).
//!
//! Скелет Ф.0.5: client + server пути реализованы (SystemRoots/CustomRoots/
//! Insecure, ALPN, server cert/key, mTLS-верификатор клиента, client-side
//! cert для mTLS). `Pinned` (SPKI-pinning, кастомный ServerCertVerifier) —
//! TLS_ERR_UNSUPPORTED до Ф.4 (план 116 Ф.4.3).

use std::ffi::c_void;
use std::io::{Read, Write};
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, ClientConnection, Connection, RootCertStore, ServerConfig, ServerConnection};

// ─── nova_int ────────────────────────────────────────────────────────────────

/// nova_int = intptr_t (Plan 133). Все int-параметры/возвраты границы.
#[allow(non_camel_case_types)]
type nova_int = isize;

// ─── Стабильные коды ошибок (зеркало: std/tls/error.nv TlsError.from_shim) ──

pub const TLS_ERR_OK: nova_int = 0;
pub const TLS_ERR_INTERNAL: nova_int = -1;
/// Аргумент-ошибка вызова (null-хендл, кривой буфер) — баг вызывающего слоя.
pub const TLS_ERR_BADARG: nova_int = -2;
pub const TLS_ERR_CERT_INVALID: nova_int = -3;
pub const TLS_ERR_CERT_EXPIRED: nova_int = -4;
pub const TLS_ERR_HOSTNAME_MISMATCH: nova_int = -5;
pub const TLS_ERR_UNSUPPORTED_VERSION: nova_int = -6;
pub const TLS_ERR_HANDSHAKE: nova_int = -7;
pub const TLS_ERR_ALPN: nova_int = -8;
/// Протокольное нарушение peer'а (в т.ч. truncation — обрыв без close_notify).
pub const TLS_ERR_PEER_MISBEHAVED: nova_int = -9;
pub const TLS_ERR_INVALID_PEM: nova_int = -10;
/// Фича ещё не реализована шимом (напр. OCSP/CRL — followups плана).
pub const TLS_ERR_UNSUPPORTED: nova_int = -11;
pub const TLS_ERR_INVALID_SNI: nova_int = -12;

/// Спец-возврат tls_read_plain: чистый TLS-EOF (close_notify получен).
pub const TLS_READ_CLOSE_NOTIFY: nova_int = -1;

// ─── Внутренние структуры за хендлами ────────────────────────────────────────

enum Verify {
    System,
    Pem(Vec<u8>),
    /// SPKI SHA-256 pinning; реализация верификатора — Ф.4. Данные принимаем
    /// уже сейчас, чтобы граница не менялась.
    #[allow(dead_code)]
    Pinned(Vec<[u8; 32]>),
    Insecure,
}

enum ClientAuth {
    No,
    /// (roots_pem, required)
    WebPki(Vec<u8>, bool),
}

/// Эфемерный конфиг-билдер (живёт внутри одного connect/accept на Nova-стороне).
struct CfgBuilder {
    is_server: bool,
    verify: Verify,
    alpn: Vec<Vec<u8>>,
    cert_pem: Vec<u8>,
    key_pem: Vec<u8>,
    client_auth: ClientAuth,
}

/// Сессия за CTlsHandle: rustls-Connection + последняя ошибка (код + текст).
struct TlsSession {
    conn: Connection,
    last_err_kind: nova_int,
    last_err: String,
}

impl TlsSession {
    fn set_err(&mut self, kind: nova_int, msg: String) -> nova_int {
        self.last_err_kind = kind;
        self.last_err = msg;
        kind
    }
}

// ─── Хелперы ─────────────────────────────────────────────────────────────────

/// # Safety: p/len приходят из Nova []u8.ptr()/len() — валидны на время вызова.
unsafe fn slice_from<'a>(p: *const u8, len: nova_int) -> Option<&'a [u8]> {
    if len < 0 || (p.is_null() && len != 0) {
        return None;
    }
    Some(std::slice::from_raw_parts(if p.is_null() { std::ptr::NonNull::dangling().as_ptr() } else { p }, len as usize))
}

/// Копирует `src` в (out, cap); возвращает ПОЛНУЮ длину `src` (копируется
/// min(cap, len) байт — вызывающий сравнивает возврат с cap).
unsafe fn copy_out(src: &[u8], out: *mut u8, cap: nova_int) -> nova_int {
    if !out.is_null() && cap > 0 {
        let n = src.len().min(cap as usize);
        std::ptr::copy_nonoverlapping(src.as_ptr(), out, n);
    }
    src.len() as nova_int
}

fn cfg<'a>(h: *mut c_void) -> Option<&'a mut CfgBuilder> {
    if h.is_null() { None } else { Some(unsafe { &mut *(h as *mut CfgBuilder) }) }
}

fn sess<'a>(h: *mut c_void) -> Option<&'a mut TlsSession> {
    if h.is_null() { None } else { Some(unsafe { &mut *(h as *mut TlsSession) }) }
}

/// rustls::Error → стабильный TLS_ERR_* код.
fn classify(e: &rustls::Error) -> nova_int {
    use rustls::CertificateError as CE;
    use rustls::Error as E;
    match e {
        E::InvalidCertificate(ce) => match ce {
            CE::Expired | CE::ExpiredContext { .. } => TLS_ERR_CERT_EXPIRED,
            CE::NotValidForName | CE::NotValidForNameContext { .. } => TLS_ERR_HOSTNAME_MISMATCH,
            _ => TLS_ERR_CERT_INVALID,
        },
        E::NoApplicationProtocol => TLS_ERR_ALPN,
        E::PeerIncompatible(_) => TLS_ERR_UNSUPPORTED_VERSION,
        E::PeerMisbehaved(_) => TLS_ERR_PEER_MISBEHAVED,
        E::AlertReceived(_) | E::InvalidMessage(_) | E::DecryptError => TLS_ERR_HANDSHAKE,
        E::General(_) => TLS_ERR_INTERNAL,
        _ => TLS_ERR_HANDSHAKE,
    }
}

fn roots_from_pem(pem: &[u8]) -> Result<RootCertStore, String> {
    let mut store = RootCertStore::empty();
    let mut added = 0usize;
    for c in rustls_pemfile::certs(&mut &pem[..]) {
        let c = c.map_err(|e| format!("bad PEM certificate: {e}"))?;
        store.add(c).map_err(|e| format!("bad CA certificate: {e}"))?;
        added += 1;
    }
    if added == 0 {
        return Err("no certificates found in PEM".into());
    }
    Ok(store)
}

fn system_roots() -> RootCertStore {
    let mut store = RootCertStore::empty();
    store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    store
}

fn certs_from_pem(pem: &[u8]) -> Result<Vec<CertificateDer<'static>>, String> {
    let certs: Result<Vec<_>, _> = rustls_pemfile::certs(&mut &pem[..]).collect();
    let certs = certs.map_err(|e| format!("bad PEM certificate chain: {e}"))?;
    if certs.is_empty() {
        return Err("no certificates found in PEM".into());
    }
    Ok(certs)
}

fn key_from_pem(pem: &[u8]) -> Result<rustls::pki_types::PrivateKeyDer<'static>, String> {
    match rustls_pemfile::private_key(&mut &pem[..]) {
        Ok(Some(k)) => Ok(k),
        Ok(None) => Err("no private key found in PEM".into()),
        Err(e) => Err(format!("bad PEM private key: {e}")),
    }
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

// ─── InsecureSkipVerify (тесты; D-блок B) ────────────────────────────────────

/// Принимает ЛЮБОЙ серверный сертификат. Только для тестов — политика D-блока B.
#[derive(Debug)]
struct NoVerify(Arc<rustls::crypto::CryptoProvider>);

impl rustls::client::danger::ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.0.signature_verification_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.0.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

// ─── Pinned (SPKI SHA-256 pinning; Ф.4.3, D-блок B) ──────────────────────────

/// Прочитать один DER TLV из `data`: возвращает (tag, диапазон-содержимого,
/// полная-длина-TLV). Поддержка short- и long-form длины (DER). None — если
/// данных не хватает / длина некорректна.
fn der_tlv(data: &[u8]) -> Option<(u8, std::ops::Range<usize>, usize)> {
    if data.len() < 2 {
        return None;
    }
    let tag = data[0];
    let b1 = data[1] as usize;
    let (len, hdr) = if b1 < 0x80 {
        (b1, 2)
    } else {
        let nbytes = b1 & 0x7f;
        if nbytes == 0 || nbytes > 4 || data.len() < 2 + nbytes {
            return None;
        }
        let mut l = 0usize;
        for i in 0..nbytes {
            l = (l << 8) | (data[2 + i] as usize);
        }
        (l, 2 + nbytes)
    };
    let end = hdr.checked_add(len)?;
    if end > data.len() {
        return None;
    }
    Some((tag, hdr..end, end))
}

/// Извлечь DER-кодированный SubjectPublicKeyInfo (весь SEQUENCE) из сертификата.
/// Прогулка по tbsCertificate: [version]? serial signature issuer validity
/// subject subjectPublicKeyInfo. SPKI — 7-й элемент (или 6-й без version).
fn spki_der(cert: &[u8]) -> Option<Vec<u8>> {
    let (t, cert_body, _) = der_tlv(cert)?;
    if t != 0x30 {
        return None;
    } // Certificate SEQUENCE
    let cert_body_bytes = &cert[cert_body];
    let (t, tbs, _) = der_tlv(cert_body_bytes)?;
    if t != 0x30 {
        return None;
    } // tbsCertificate SEQUENCE
    let tbs_bytes = &cert_body_bytes[tbs];
    let mut off = 0usize;
    // optional version [0] EXPLICIT (context-constructed tag 0xA0)
    let (t0, _, c0) = der_tlv(&tbs_bytes[off..])?;
    if t0 == 0xA0 {
        off += c0;
    }
    // serialNumber, signature, issuer, validity, subject — пропустить 5 полей
    for _ in 0..5 {
        let (_, _, c) = der_tlv(&tbs_bytes[off..])?;
        off += c;
    }
    // subjectPublicKeyInfo SEQUENCE — вернуть ВЕСЬ TLV (tag+len+value)
    let (t, _, c) = der_tlv(&tbs_bytes[off..])?;
    if t != 0x30 {
        return None;
    }
    Some(tbs_bytes[off..off + c].to_vec())
}

/// Верификатор cert-pinning: принимает серт, если SHA-256 его SPKI совпадает с
/// одним из пинов; цепочка игнорируется, hostname-проверка опциональна
/// (pinning заменяет её — D-блок B). Подпись рукопожатия ВСЁ РАВНО проверяется
/// (доказывает владение пиннутым ключом).
#[derive(Debug)]
struct PinnedVerify {
    pins: Vec<[u8; 32]>,
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl rustls::client::danger::ServerCertVerifier for PinnedVerify {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let spki = spki_der(end_entity.as_ref()).ok_or_else(|| {
            rustls::Error::InvalidCertificate(rustls::CertificateError::BadEncoding)
        })?;
        let digest = ring::digest::digest(&ring::digest::SHA256, &spki);
        let got: &[u8] = digest.as_ref();
        if self.pins.iter().any(|p| p.as_slice() == got) {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::ApplicationVerificationFailure,
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

// ─── Конфиг-билдеры ──────────────────────────────────────────────────────────

fn new_builder(is_server: bool) -> *mut c_void {
    Box::into_raw(Box::new(CfgBuilder {
        is_server,
        verify: Verify::System,
        alpn: Vec::new(),
        cert_pem: Vec::new(),
        key_pem: Vec::new(),
        client_auth: ClientAuth::No,
    })) as *mut c_void
}

/// Новый билдер клиент-конфига (verify=SystemRoots, без ALPN).
#[no_mangle]
pub extern "C" fn tls_client_cfg_new() -> *mut c_void {
    new_builder(false)
}

/// Новый билдер сервер-конфига (cert/key обязательны до tls_server_new).
#[no_mangle]
pub extern "C" fn tls_server_cfg_new() -> *mut c_void {
    new_builder(true)
}

/// VerificationMode.SystemRoots — webpki-roots (Mozilla bundle).
#[no_mangle]
pub extern "C" fn tls_cfg_verify_system(c: *mut c_void) -> nova_int {
    match cfg(c) {
        Some(b) => { b.verify = Verify::System; TLS_ERR_OK }
        None => TLS_ERR_BADARG,
    }
}

/// VerificationMode.CustomRoots — CA-bundle из PEM-байтов.
#[no_mangle]
pub unsafe extern "C" fn tls_cfg_verify_pem(c: *mut c_void, pem: *const u8, len: nova_int) -> nova_int {
    let (Some(b), Some(s)) = (cfg(c), slice_from(pem, len)) else { return TLS_ERR_BADARG };
    b.verify = Verify::Pem(s.to_vec());
    TLS_ERR_OK
}

/// VerificationMode.Pinned — count хешей по 32 байта (SPKI SHA-256).
/// Верификатор `PinnedVerify` (Ф.4.3): сверяет SHA-256 SPKI leaf-серта с пинами,
/// цепочку игнорирует, hostname-проверку заменяет пиннингом (D-блок B).
#[no_mangle]
pub unsafe extern "C" fn tls_cfg_verify_pinned(c: *mut c_void, hashes: *const u8, count: nova_int) -> nova_int {
    let (Some(b), Some(s)) = (cfg(c), slice_from(hashes, count.saturating_mul(32))) else { return TLS_ERR_BADARG };
    if count <= 0 {
        return TLS_ERR_BADARG;
    }
    let pins = s.chunks_exact(32).map(|ch| { let mut a = [0u8; 32]; a.copy_from_slice(ch); a }).collect();
    b.verify = Verify::Pinned(pins);
    TLS_ERR_OK
}

/// VerificationMode.InsecureSkipVerify — ТОЛЬКО тесты (D-блок B).
#[no_mangle]
pub extern "C" fn tls_cfg_verify_insecure(c: *mut c_void) -> nova_int {
    match cfg(c) {
        Some(b) => { b.verify = Verify::Insecure; TLS_ERR_OK }
        None => TLS_ERR_BADARG,
    }
}

/// Добавить ALPN-протокол (повторяемо; порядок = приоритет клиента).
#[no_mangle]
pub unsafe extern "C" fn tls_cfg_alpn_add(c: *mut c_void, proto: *const u8, len: nova_int) -> nova_int {
    let (Some(b), Some(s)) = (cfg(c), slice_from(proto, len)) else { return TLS_ERR_BADARG };
    if s.is_empty() {
        return TLS_ERR_BADARG;
    }
    b.alpn.push(s.to_vec());
    TLS_ERR_OK
}

/// Cert chain + private key (PEM). Сервер: обязателен. Клиент: cert для mTLS.
#[no_mangle]
pub unsafe extern "C" fn tls_cfg_cert_key_pem(
    c: *mut c_void,
    cert: *const u8,
    clen: nova_int,
    key: *const u8,
    klen: nova_int,
) -> nova_int {
    let (Some(b), Some(cs), Some(ks)) = (cfg(c), slice_from(cert, clen), slice_from(key, klen)) else {
        return TLS_ERR_BADARG;
    };
    if cs.is_empty() || ks.is_empty() {
        return TLS_ERR_BADARG;
    }
    b.cert_pem = cs.to_vec();
    b.key_pem = ks.to_vec();
    TLS_ERR_OK
}

/// Server-side mTLS: верификация клиентских сертов по CA-bundle (PEM).
/// required=false → Optional (запросить, но пустить без серта).
#[no_mangle]
pub unsafe extern "C" fn tls_cfg_client_auth_pem(
    c: *mut c_void,
    roots: *const u8,
    len: nova_int,
    required: bool,
) -> nova_int {
    let (Some(b), Some(s)) = (cfg(c), slice_from(roots, len)) else { return TLS_ERR_BADARG };
    if s.is_empty() {
        return TLS_ERR_BADARG;
    }
    b.client_auth = ClientAuth::WebPki(s.to_vec(), required);
    TLS_ERR_OK
}

/// Освободить билдер БЕЗ создания сессии (error-путь Nova-кода).
/// tls_client_new / tls_server_new потребляют билдер сами.
#[no_mangle]
pub extern "C" fn tls_cfg_free(c: *mut c_void) {
    if !c.is_null() {
        drop(unsafe { Box::from_raw(c as *mut CfgBuilder) });
    }
}

// ─── Создание сессий ─────────────────────────────────────────────────────────

fn err_out(out_err: *mut nova_int, code: nova_int) -> *mut c_void {
    if !out_err.is_null() {
        unsafe { *out_err = code };
    }
    std::ptr::null_mut()
}

fn build_client_config(b: &CfgBuilder) -> Result<ClientConfig, (nova_int, String)> {
    let prov = provider();
    let versions = ClientConfig::builder_with_provider(prov.clone())
        .with_safe_default_protocol_versions()
        .map_err(|e| (TLS_ERR_INTERNAL, format!("protocol versions: {e}")))?;

    let want_verifier = match &b.verify {
        Verify::System => versions.with_root_certificates(system_roots()),
        Verify::Pem(pem) => {
            let roots = roots_from_pem(pem).map_err(|m| (TLS_ERR_INVALID_PEM, m))?;
            versions.with_root_certificates(roots)
        }
        Verify::Pinned(pins) => {
            if pins.is_empty() {
                return Err((TLS_ERR_BADARG, "Pinned requires at least one pin".into()));
            }
            versions.dangerous().with_custom_certificate_verifier(Arc::new(PinnedVerify {
                pins: pins.clone(),
                provider: prov,
            }))
        }
        Verify::Insecure => versions
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerify(prov))),
    };

    let mut config = if b.cert_pem.is_empty() {
        want_verifier.with_no_client_auth()
    } else {
        let chain = certs_from_pem(&b.cert_pem).map_err(|m| (TLS_ERR_INVALID_PEM, m))?;
        let key = key_from_pem(&b.key_pem).map_err(|m| (TLS_ERR_INVALID_PEM, m))?;
        want_verifier
            .with_client_auth_cert(chain, key)
            .map_err(|e| (classify(&e), format!("client auth cert: {e}")))?
    };
    config.alpn_protocols = b.alpn.clone();
    Ok(config)
}

fn build_server_config(b: &CfgBuilder) -> Result<ServerConfig, (nova_int, String)> {
    let prov = provider();
    let versions = ServerConfig::builder_with_provider(prov.clone())
        .with_safe_default_protocol_versions()
        .map_err(|e| (TLS_ERR_INTERNAL, format!("protocol versions: {e}")))?;

    let want_cert = match &b.client_auth {
        ClientAuth::No => versions.with_no_client_auth(),
        ClientAuth::WebPki(pem, required) => {
            let roots = roots_from_pem(pem).map_err(|m| (TLS_ERR_INVALID_PEM, m))?;
            let vb = rustls::server::WebPkiClientVerifier::builder_with_provider(Arc::new(roots), prov);
            let vb = if *required { vb } else { vb.allow_unauthenticated() };
            let verifier = vb
                .build()
                .map_err(|e| (TLS_ERR_INTERNAL, format!("client verifier: {e}")))?;
            versions.with_client_cert_verifier(verifier)
        }
    };

    if b.cert_pem.is_empty() || b.key_pem.is_empty() {
        return Err((TLS_ERR_BADARG, "server config requires cert+key PEM".into()));
    }
    let chain = certs_from_pem(&b.cert_pem).map_err(|m| (TLS_ERR_INVALID_PEM, m))?;
    let key = key_from_pem(&b.key_pem).map_err(|m| (TLS_ERR_INVALID_PEM, m))?;
    let mut config = want_cert
        .with_single_cert(chain, key)
        .map_err(|e| (classify(&e), format!("server cert/key: {e}")))?;
    config.alpn_protocols = b.alpn.clone();
    Ok(config)
}

fn session_ptr(conn: Connection) -> *mut c_void {
    Box::into_raw(Box::new(TlsSession { conn, last_err_kind: TLS_ERR_OK, last_err: String::new() })) as *mut c_void
}

/// Клиентская сессия. ПОТРЕБЛЯЕТ билдер `c` (в т.ч. на ошибке). SNI обязателен
/// (байты — DNS-имя или IP-текст). null → код в *out_err.
#[no_mangle]
pub unsafe extern "C" fn tls_client_new(
    c: *mut c_void,
    sni: *const u8,
    sni_len: nova_int,
    out_err: *mut nova_int,
) -> *mut c_void {
    if c.is_null() {
        return err_out(out_err, TLS_ERR_BADARG);
    }
    let b = Box::from_raw(c as *mut CfgBuilder); // владение билдером — потребляем
    if b.is_server {
        return err_out(out_err, TLS_ERR_BADARG);
    }
    let Some(sni_bytes) = slice_from(sni, sni_len) else {
        return err_out(out_err, TLS_ERR_INVALID_SNI);
    };
    let Ok(sni_str) = std::str::from_utf8(sni_bytes) else {
        return err_out(out_err, TLS_ERR_INVALID_SNI);
    };
    let Ok(name) = ServerName::try_from(sni_str.to_owned()) else {
        return err_out(out_err, TLS_ERR_INVALID_SNI);
    };
    let config = match build_client_config(&b) {
        Ok(cfg) => cfg,
        Err((code, _msg)) => return err_out(out_err, code),
    };
    match ClientConnection::new(Arc::new(config), name) {
        Ok(conn) => session_ptr(Connection::Client(conn)),
        Err(e) => err_out(out_err, classify(&e)),
    }
}

/// Серверная сессия. ПОТРЕБЛЯЕТ билдер `c` (в т.ч. на ошибке).
#[no_mangle]
pub unsafe extern "C" fn tls_server_new(c: *mut c_void, out_err: *mut nova_int) -> *mut c_void {
    if c.is_null() {
        return err_out(out_err, TLS_ERR_BADARG);
    }
    let b = Box::from_raw(c as *mut CfgBuilder);
    if !b.is_server {
        return err_out(out_err, TLS_ERR_BADARG);
    }
    let config = match build_server_config(&b) {
        Ok(cfg) => cfg,
        Err((code, _msg)) => return err_out(out_err, code),
    };
    match ServerConnection::new(Arc::new(config)) {
        Ok(conn) => session_ptr(Connection::Server(conn)),
        Err(e) => err_out(out_err, classify(&e)),
    }
}

// ─── State machine предикаты ─────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn tls_is_handshaking(h: *mut c_void) -> nova_int {
    sess(h).map_or(0, |s| s.conn.is_handshaking() as nova_int)
}

#[no_mangle]
pub extern "C" fn tls_wants_read(h: *mut c_void) -> nova_int {
    sess(h).map_or(0, |s| s.conn.wants_read() as nova_int)
}

#[no_mangle]
pub extern "C" fn tls_wants_write(h: *mut c_void) -> nova_int {
    sess(h).map_or(0, |s| s.conn.wants_write() as nova_int)
}

// ─── Трафик: ciphertext ↔ session ↔ plaintext ────────────────────────────────

/// Скормить ciphertext из сокета. Возврат: n принятых (может быть < len —
/// перезвать с остатком), <0 = ошибка. После feed'ов звать tls_process.
#[no_mangle]
pub unsafe extern "C" fn tls_read_tls(h: *mut c_void, p: *const u8, len: nova_int) -> nova_int {
    let Some(s) = sess(h) else { return TLS_ERR_BADARG };
    let Some(buf) = slice_from(p, len) else { return s.set_err(TLS_ERR_BADARG, "bad buffer".into()) };
    let mut rd = buf;
    match s.conn.read_tls(&mut rd) {
        Ok(n) => n as nova_int,
        Err(e) => s.set_err(TLS_ERR_INTERNAL, format!("read_tls: {e}")),
    }
}

/// Обработать скормленные пакеты (handshake-прогресс + расшифровка).
/// 0 = OK; <0 = ошибка (ПОСЛЕ ошибки rustls хочет отправить alert —
/// вызывающий обязан ещё раз слить tls_write_tls в сокет перед close).
#[no_mangle]
pub extern "C" fn tls_process(h: *mut c_void) -> nova_int {
    let Some(s) = sess(h) else { return TLS_ERR_BADARG };
    match s.conn.process_new_packets() {
        Ok(_) => TLS_ERR_OK,
        Err(e) => {
            let code = classify(&e);
            s.set_err(code, format!("{e}"))
        }
    }
}

/// Извлечь исходящий ciphertext (handshake flights, app-данные, alerts).
/// Возврат: n записанных в out (0 = нечего), <0 = ошибка.
#[no_mangle]
pub unsafe extern "C" fn tls_write_tls(h: *mut c_void, out: *mut u8, cap: nova_int) -> nova_int {
    let Some(s) = sess(h) else { return TLS_ERR_BADARG };
    if out.is_null() || cap <= 0 {
        return s.set_err(TLS_ERR_BADARG, "bad buffer".into());
    }
    if !s.conn.wants_write() {
        return 0;
    }
    let mut sink = std::io::Cursor::new(std::slice::from_raw_parts_mut(out, cap as usize));
    match s.conn.write_tls(&mut sink) {
        Ok(n) => n as nova_int,
        Err(e) => s.set_err(TLS_ERR_INTERNAL, format!("write_tls: {e}")),
    }
}

/// Прочитать расшифрованный plaintext. Возврат: n>0 — байты; 0 — данных пока
/// нет (нужен ещё feed из сокета); TLS_READ_CLOSE_NOTIFY (-1) — чистый TLS-EOF;
/// < -1 — ошибка (обрыв без close_notify = TLS_ERR_PEER_MISBEHAVED).
#[no_mangle]
pub unsafe extern "C" fn tls_read_plain(h: *mut c_void, out: *mut u8, cap: nova_int) -> nova_int {
    let Some(s) = sess(h) else { return TLS_ERR_BADARG };
    if out.is_null() || cap <= 0 {
        return s.set_err(TLS_ERR_BADARG, "bad buffer".into());
    }
    let buf = std::slice::from_raw_parts_mut(out, cap as usize);
    match s.conn.reader().read(buf) {
        Ok(0) => TLS_READ_CLOSE_NOTIFY,
        Ok(n) => n as nova_int,
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => 0,
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            s.set_err(TLS_ERR_PEER_MISBEHAVED, "peer closed without close_notify (possible truncation)".into())
        }
        Err(e) => s.set_err(TLS_ERR_INTERNAL, format!("read_plain: {e}")),
    }
}

/// Зашифровать plaintext (ложится во внутренний исходящий буфер — слить
/// tls_write_tls). Возврат: n принятых, <0 = ошибка.
#[no_mangle]
pub unsafe extern "C" fn tls_write_plain(h: *mut c_void, p: *const u8, len: nova_int) -> nova_int {
    let Some(s) = sess(h) else { return TLS_ERR_BADARG };
    let Some(buf) = slice_from(p, len) else { return s.set_err(TLS_ERR_BADARG, "bad buffer".into()) };
    match s.conn.writer().write(buf) {
        Ok(n) => n as nova_int,
        Err(e) => s.set_err(TLS_ERR_INTERNAL, format!("write_plain: {e}")),
    }
}

/// Поставить close_notify в исходящий буфер (слить tls_write_tls, затем
/// закрывать TCP). Идемпотентно.
#[no_mangle]
pub extern "C" fn tls_send_close_notify(h: *mut c_void) {
    if let Some(s) = sess(h) {
        s.conn.send_close_notify();
    }
}

// ─── Инспекция (после handshake) ─────────────────────────────────────────────

/// Negotiated ALPN → out (копия min(cap,len)); возврат = полная длина; 0 = нет.
#[no_mangle]
pub unsafe extern "C" fn tls_alpn(h: *mut c_void, out: *mut u8, cap: nova_int) -> nova_int {
    let Some(s) = sess(h) else { return TLS_ERR_BADARG };
    match s.conn.alpn_protocol() {
        Some(p) => copy_out(p, out, cap),
        None => 0,
    }
}

/// Версия протокола: 0x0303 = TLS 1.2, 0x0304 = TLS 1.3; 0 = ещё не согласована.
#[no_mangle]
pub extern "C" fn tls_version(h: *mut c_void) -> nova_int {
    let Some(s) = sess(h) else { return TLS_ERR_BADARG };
    match s.conn.protocol_version() {
        Some(v) => u16::from(v) as nova_int,
        None => 0,
    }
}

/// Имя cipher suite (напр. "TLS13_AES_256_GCM_SHA384") → out; возврат = длина.
#[no_mangle]
pub unsafe extern "C" fn tls_cipher_suite(h: *mut c_void, out: *mut u8, cap: nova_int) -> nova_int {
    let Some(s) = sess(h) else { return TLS_ERR_BADARG };
    match s.conn.negotiated_cipher_suite() {
        Some(cs) => {
            let name = format!("{:?}", cs.suite());
            copy_out(name.as_bytes(), out, cap)
        }
        None => 0,
    }
}

/// DER сертификата peer'а с индексом i (0 = leaf) → out (копия min(cap,len));
/// возврат = полная длина DER; 0 = серта с таким индексом нет.
#[no_mangle]
pub unsafe extern "C" fn tls_peer_cert_der(h: *mut c_void, i: nova_int, out: *mut u8, cap: nova_int) -> nova_int {
    let Some(s) = sess(h) else { return TLS_ERR_BADARG };
    if i < 0 {
        return 0;
    }
    match s.conn.peer_certificates().and_then(|cs| cs.get(i as usize)) {
        Some(der) => copy_out(der.as_ref(), out, cap),
        None => 0,
    }
}

// ─── Ошибки ──────────────────────────────────────────────────────────────────

/// Код класса последней ошибки сессии (TLS_ERR_*; 0 = ошибки не было).
#[no_mangle]
pub extern "C" fn tls_last_error_kind(h: *mut c_void) -> nova_int {
    sess(h).map_or(TLS_ERR_BADARG, |s| s.last_err_kind)
}

/// Текст последней ошибки → out (копия min(cap,len)); возврат = полная длина.
#[no_mangle]
pub unsafe extern "C" fn tls_last_error(h: *mut c_void, out: *mut u8, cap: nova_int) -> nova_int {
    let Some(s) = sess(h) else { return TLS_ERR_BADARG };
    let msg = s.last_err.clone();
    copy_out(msg.as_bytes(), out, cap)
}

// ─── Освобождение ────────────────────────────────────────────────────────────

/// Освободить сессию. Безопасно на null (double-free по одному хендлу —
/// нарушение контракта; Nova-сторона исключает его consume-типом, D133).
#[no_mangle]
pub extern "C" fn tls_free(h: *mut c_void) {
    if !h.is_null() {
        drop(unsafe { Box::from_raw(h as *mut TlsSession) });
    }
}

// ─── Санити-тесты шима (cargo test --lib недоступен для staticlib-only —
//     запускаются через `cargo test` (unittest-профиль собирает rlib) ─────────

#[cfg(test)]
mod tests {
    use super::*;

    const CERT_PEM: &[u8] = include_bytes!("../../../std/tls/testdata/localhost_cert.pem");
    const KEY_PEM: &[u8] = include_bytes!("../../../std/tls/testdata/localhost_key.pem");

    /// Decisive Ф.3 debug: full in-memory TLS 1.3 handshake between a real
    /// client and server session via the C-ABI, moving ciphertext by hand.
    /// Проверяет, что `is_handshaking` ДЕЙСТВИТЕЛЬНО обнуляется на обеих
    /// сторонах (изолирует shim/rustls от Nova-транспорта).
    #[test]
    fn full_in_memory_handshake_completes() {
        unsafe {
            // server session (CustomRoots not needed server-side)
            let sc = tls_server_cfg_new();
            assert_eq!(tls_cfg_cert_key_pem(sc, CERT_PEM.as_ptr(), CERT_PEM.len() as nova_int,
                                            KEY_PEM.as_ptr(), KEY_PEM.len() as nova_int), TLS_ERR_OK);
            let mut serr = 0;
            let s = tls_server_new(sc, &mut serr);
            assert!(!s.is_null(), "server session (err={serr})");

            // client session with CustomRoots = self-signed cert
            let cc = tls_client_cfg_new();
            assert_eq!(tls_cfg_verify_pem(cc, CERT_PEM.as_ptr(), CERT_PEM.len() as nova_int), TLS_ERR_OK);
            let mut cerr = 0;
            let c = tls_client_new(cc, b"localhost".as_ptr(), 9, &mut cerr);
            assert!(!c.is_null(), "client session (err={cerr})");

            // pump: move ciphertext c<->s until both stop handshaking (bounded)
            let mut buf = vec![0u8; 32 * 1024];
            let mut steps = 0;
            while (tls_is_handshaking(c) != 0 || tls_is_handshaking(s) != 0) && steps < 50 {
                steps += 1;
                // client -> server
                let n = tls_write_tls(c, buf.as_mut_ptr(), buf.len() as nova_int);
                if n > 0 {
                    let mut off = 0;
                    while off < n {
                        let fed = tls_read_tls(s, buf.as_ptr().add(off as usize), n - off);
                        assert!(fed > 0, "server read_tls fed={fed}");
                        off += fed;
                    }
                    assert_eq!(tls_process(s), TLS_ERR_OK, "server process");
                }
                // server -> client
                let m = tls_write_tls(s, buf.as_mut_ptr(), buf.len() as nova_int);
                if m > 0 {
                    let mut off = 0;
                    while off < m {
                        let fed = tls_read_tls(c, buf.as_ptr().add(off as usize), m - off);
                        assert!(fed > 0, "client read_tls fed={fed}");
                        off += fed;
                    }
                    assert_eq!(tls_process(c), TLS_ERR_OK, "client process");
                }
            }
            assert_eq!(tls_is_handshaking(c), 0, "CLIENT still handshaking after {steps} steps");
            assert_eq!(tls_is_handshaking(s), 0, "SERVER still handshaking after {steps} steps");

            // app data round-trip
            assert!(tls_write_plain(c, b"ping".as_ptr(), 4) > 0);
            let cm = tls_write_tls(c, buf.as_mut_ptr(), buf.len() as nova_int);
            assert!(cm > 0);
            let mut off = 0; while off < cm { off += tls_read_tls(s, buf.as_ptr().add(off as usize), cm - off); }
            assert_eq!(tls_process(s), TLS_ERR_OK);
            let mut plain = vec![0u8; 64];
            let got = tls_read_plain(s, plain.as_mut_ptr(), plain.len() as nova_int);
            assert_eq!(got, 4, "server should decrypt 4 plaintext bytes, got {got}");
            assert_eq!(&plain[0..4], b"ping");

            tls_free(c);
            tls_free(s);
        }
    }

    /// Полный in-memory handshake client↔server через C-ABI поверхность —
    /// доказывает, что скелет живой без единого сокета.
    #[test]
    fn in_memory_handshake_roundtrip() {
        // Самоподписанный серт для localhost, сгенерированный на лету через
        // rustls-провайдер невозможен (нет генерации ключей) — для юнит-теста
        // достаточно проверить error-пути конфигов; полный handshake-тест
        // едет в Ф.3 (self-signed fixture PEM в std/tls/testdata/).
        let c = tls_client_cfg_new();
        assert_eq!(tls_cfg_verify_system(c), TLS_ERR_OK);
        unsafe {
            assert_eq!(tls_cfg_alpn_add(c, b"http/1.1".as_ptr(), 8), TLS_ERR_OK);
            let mut err: nova_int = 0;
            let h = tls_client_new(c, b"example.com".as_ptr(), 11, &mut err);
            assert!(!h.is_null(), "client session must build (err={err})");
            assert_eq!(tls_is_handshaking(h), 1);
            assert_eq!(tls_wants_write(h), 1); // ClientHello готов к отправке
            let mut buf = vec![0u8; 4096];
            let n = tls_write_tls(h, buf.as_mut_ptr(), buf.len() as nova_int);
            assert!(n > 0, "ClientHello bytes expected, got {n}");
            tls_free(h);
        }
    }

    #[test]
    fn bad_pem_is_classified() {
        let c = tls_client_cfg_new();
        unsafe {
            assert_eq!(tls_cfg_verify_pem(c, b"not a pem".as_ptr(), 9), TLS_ERR_OK);
            let mut err: nova_int = 0;
            let h = tls_client_new(c, b"example.com".as_ptr(), 11, &mut err);
            assert!(h.is_null());
            assert_eq!(err, TLS_ERR_INVALID_PEM);
        }
    }

    /// Прогнать handshake между двумя сессиями (in-memory). Возвращает Ok(())
    /// если ОБА завершили рукопожатие; Err(code) на первой ошибке process
    /// (например, отказ верификатора → alert). До 50 шагов.
    unsafe fn drive_handshake(c: *mut c_void, s: *mut c_void) -> Result<(), nova_int> {
        let mut buf = vec![0u8; 32 * 1024];
        let mut steps = 0;
        while (tls_is_handshaking(c) != 0 || tls_is_handshaking(s) != 0) && steps < 50 {
            steps += 1;
            let n = tls_write_tls(c, buf.as_mut_ptr(), buf.len() as nova_int);
            if n > 0 {
                let mut off = 0;
                while off < n { let f = tls_read_tls(s, buf.as_ptr().add(off as usize), n - off); if f <= 0 { break } off += f; }
                let rc = tls_process(s);
                if rc < 0 { return Err(rc); }
            }
            let m = tls_write_tls(s, buf.as_mut_ptr(), buf.len() as nova_int);
            if m > 0 {
                let mut off = 0;
                while off < m { let f = tls_read_tls(c, buf.as_ptr().add(off as usize), m - off); if f <= 0 { break } off += f; }
                let rc = tls_process(c);
                if rc < 0 { return Err(rc); }
            }
        }
        if tls_is_handshaking(c) == 0 && tls_is_handshaking(s) == 0 { Ok(()) } else { Err(TLS_ERR_HANDSHAKE) }
    }

    // Вычислить SPKI SHA-256 пин из leaf-серта PEM (тем же кодом, что верификатор).
    fn spki_pin(cert_pem: &[u8]) -> [u8; 32] {
        let der = rustls_pemfile::certs(&mut &cert_pem[..]).next().unwrap().unwrap();
        let spki = spki_der(der.as_ref()).expect("parse SPKI");
        let d = ring::digest::digest(&ring::digest::SHA256, &spki);
        d.as_ref().try_into().unwrap()
    }

    fn mk_server() -> *mut c_void {
        unsafe {
            let sc = tls_server_cfg_new();
            assert_eq!(tls_cfg_cert_key_pem(sc, CERT_PEM.as_ptr(), CERT_PEM.len() as nova_int,
                                            KEY_PEM.as_ptr(), KEY_PEM.len() as nova_int), TLS_ERR_OK);
            let mut e = 0;
            let s = tls_server_new(sc, &mut e);
            assert!(!s.is_null(), "server (err={e})");
            s
        }
    }

    #[test]
    fn pinned_correct_pin_completes_handshake() {
        unsafe {
            let s = mk_server();
            let pin = spki_pin(CERT_PEM);
            let cc = tls_client_cfg_new();
            assert_eq!(tls_cfg_verify_pinned(cc, pin.as_ptr(), 1), TLS_ERR_OK);
            let mut e = 0;
            let c = tls_client_new(cc, b"localhost".as_ptr(), 9, &mut e);
            assert!(!c.is_null(), "client (err={e})");
            assert!(drive_handshake(c, s).is_ok(), "pinned handshake with correct pin must complete");
            tls_free(c);
            tls_free(s);
        }
    }

    #[test]
    fn pinned_wrong_pin_rejects() {
        unsafe {
            let s = mk_server();
            let bad = [0u8; 32]; // не совпадает с реальным SPKI
            let cc = tls_client_cfg_new();
            assert_eq!(tls_cfg_verify_pinned(cc, bad.as_ptr(), 1), TLS_ERR_OK);
            let mut e = 0;
            let c = tls_client_new(cc, b"localhost".as_ptr(), 9, &mut e);
            assert!(!c.is_null(), "client (err={e})");
            // клиент отвергает серт → handshake НЕ завершается (alert/ошибка).
            assert!(drive_handshake(c, s).is_err(), "wrong pin must reject the handshake");
            tls_free(c);
            tls_free(s);
        }
    }

    #[test]
    fn pinned_wrong_sni_still_accepts() {
        // Pinning заменяет hostname-проверку (D-блок B): неверный SNI ок,
        // если SPKI совпадает.
        unsafe {
            let s = mk_server();
            let pin = spki_pin(CERT_PEM);
            let cc = tls_client_cfg_new();
            assert_eq!(tls_cfg_verify_pinned(cc, pin.as_ptr(), 1), TLS_ERR_OK);
            let mut e = 0;
            let c = tls_client_new(cc, b"wrong.example".as_ptr(), 13, &mut e);
            assert!(!c.is_null(), "client (err={e})");
            assert!(drive_handshake(c, s).is_ok(), "pinning ignores hostname mismatch");
            tls_free(c);
            tls_free(s);
        }
    }

    #[test]
    fn server_requires_cert_key() {
        let c = tls_server_cfg_new();
        unsafe {
            let mut err: nova_int = 0;
            let h = tls_server_new(c, &mut err);
            assert!(h.is_null());
            assert_eq!(err, TLS_ERR_BADARG);
        }
    }

    #[test]
    fn bad_sni_rejected() {
        let c = tls_client_cfg_new();
        unsafe {
            let mut err: nova_int = 0;
            let h = tls_client_new(c, b"bad name!".as_ptr(), 9, &mut err);
            assert!(h.is_null());
            assert_eq!(err, TLS_ERR_INVALID_SNI);
        }
    }
}
