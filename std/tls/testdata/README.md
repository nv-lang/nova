# std/tls/testdata — fixture-сертификаты

Самоподписанная пара для smoke/negative-тестов (Plan 116 Ф.3+):

| Файл | Что |
|---|---|
| `localhost_cert.pem` | self-signed ECDSA P-256, `CN=localhost`, SAN `DNS:localhost, IP:127.0.0.1`, срок 100 лет (не протухает в тестах) |
| `localhost_key.pem` | приватный ключ (SEC1 EC PRIVATE KEY) |

Тесты встраивают их через `embed("testdata/…")` (D412) — Fs-эффект не нужен.
**Только для тестов** — ключ публичен by design.

Регенерация (Git Bash / openssl ≥ 3):

```sh
cd std/tls/testdata
openssl ecparam -name prime256v1 -genkey -noout -out localhost_key.pem
openssl req -new -x509 -key localhost_key.pem -out localhost_cert.pem \
  -days 36500 -subj "//CN=localhost" \
  -addext "subjectAltName=DNS:localhost,IP:127.0.0.1" \
  -addext "basicConstraints=critical,CA:FALSE"
```

(`//CN` — MSYS-экранирование `/CN` на Windows.)
