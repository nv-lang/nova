# std/tls/testdata — fixture-сертификаты

Самоподписанная пара для smoke/negative-тестов (Plan 116 Ф.3+):

| Файл | Что |
|---|---|
| `localhost_cert.pem` | server: self-signed ECDSA P-256, `CN=localhost`, SAN `DNS:localhost, IP:127.0.0.1`, срок 100 лет (не протухает в тестах) |
| `localhost_key.pem` | приватный ключ сервера (SEC1 EC PRIVATE KEY) |
| `client_ca_cert.pem` | mTLS: клиентский CA (self-signed, CA:TRUE) — сервер доверяет ему для проверки клиентских сертов |
| `client_ca_key.pem` | ключ клиентского CA (для регенерации leaf'ов) |
| `client_cert.pem` | mTLS: клиентский leaf `CN=nova-test-client`, подписан client CA, EKU=clientAuth |
| `client_key.pem` | приватный ключ клиента |

Тесты встраивают их через `embed("testdata/…")` (D412) — Fs-эффект не нужен.
**Только для тестов** — ключи публичны by design.

SPKI SHA-256 пин `localhost_cert.pem` (для Pinned-тестов, `cert_modes_test.nv`):
`openssl x509 -in localhost_cert.pem -pubkey -noout | openssl pkey -pubin -outform der | openssl dgst -sha256`.

Регенерация (Git Bash / openssl ≥ 3):

```sh
cd std/tls/testdata
# server
openssl ecparam -name prime256v1 -genkey -noout -out localhost_key.pem
openssl req -new -x509 -key localhost_key.pem -out localhost_cert.pem \
  -days 36500 -subj "//CN=localhost" \
  -addext "subjectAltName=DNS:localhost,IP:127.0.0.1" \
  -addext "basicConstraints=critical,CA:FALSE"
# mTLS client CA + leaf
openssl ecparam -name prime256v1 -genkey -noout -out client_ca_key.pem
openssl req -new -x509 -key client_ca_key.pem -out client_ca_cert.pem -days 36500 \
  -subj "//CN=Nova Test Client CA" \
  -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign,cRLSign"
openssl ecparam -name prime256v1 -genkey -noout -out client_key.pem
openssl req -new -key client_key.pem -out client.csr -subj "//CN=nova-test-client"
printf "basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=clientAuth\n" > client_ext.cnf
openssl x509 -req -in client.csr -CA client_ca_cert.pem -CAkey client_ca_key.pem \
  -CAcreateserial -out client_cert.pem -days 36500 -extfile client_ext.cnf
rm -f client.csr client_ext.cnf client_ca_cert.srl
```

(`//CN` — MSYS-экранирование `/CN` на Windows. При смене `localhost_cert.pem`
обнови SPKI-пин в `cert_modes_test.nv` + shim-cross-check.)
