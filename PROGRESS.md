# PROGRESS — перевод ///-комментариев std на английский

Канон check: FAIL: 26. Счёт кириллицы в /// — python-сканером (grep из брифа
на этом хосте ловит ложные срабатывания на — и →, не используем).

- [x] prelude (0 строк русской прозы; осталось 17 имён-маркеров Ф.* и §«Bootstrap-ограничения» — законные остатки) — коммиты 208564949, 9e25cf263
- [x] collections/vec — остатки прозы (4 стр.) — 556f6d01c
- [x] io/fs: io/core, fs/fs, fs/readfs — 44743be2d
- [x] collections — остальное (bloom, deque, hashmap, linkedlist, lru, queue, priority_queue, range/core, set, vec/core) — 6b13a1985
- [x] net: tcp, udp — fb9eb6a54
- [x] time 1/5: cron, parse, tz, tzif, weekday_month — e81895052
- [x] time 5/7: civil/date, civil/period — 68923c18f
- [x] time 6/7: civil/zoned, duration/timestamp, duration/monotonic — 721e8a729
- [x] time 7/7 (финал): duration/core, duration/time_effect — 9c20e4715
- [x] time (518)
- [ ] encoding (137 — параллельный агент)
- [x] runtime (393→0; остались только маркеры Ф.2/Ф.4/Ф.4R/Ш0/Ш1/Ш3/§2/§6/§10R-Д3/Ф.0.5) — aef6fe374, 1162678cc
- [x] checksums (18→0) — ed257c2b4
- [x] concurrency/rate_limiter + cancellation (9) — ed257c2b4
- [x] crypto/hmac + md5 (9) — ed257c2b4
- [x] text.nv (1) — ed257c2b4
- [x] concurrency (43→0; остался маркер Ф.10 в timer.nv:57) — ed257c2b4, c063eddf7, 954fad47a
- [x] crypto (26→0) — ed257c2b4, c063eddf7
- [x] text (24→0) — c063eddf7, 954fad47a
- [x] data/semver_range + semver (23→0; остался §1а в semver_range.nv:64) — c063eddf7, 954fad47a
- [x] testing/handlers/core + math/statistics + _experimental — c063eddf7
- [x] identifiers/data/path (uuid_namespace, ulid, sql, path) — fe8c31bba
- [x] identifiers (uuid, snowflake 46→0) — 3f66abcfe
- [x] math (complex, int128 101→0; остался маркер Ф.2) — 08aac019d
- [x] root (bench, sort, reflect 70→0) — 2d8be0eab
- [x] testing/property (38→0) — 13480f918
- [x] encoding (137→0) — закрыта полностью: csv/hex/ini/toml (7761c5ebc), base64/url/utf16 (3650bfbec), json (9f95fc254), serde/serde+serde/json (9c89f55c9). field_attrs_test.nv — без правок (маркер Ф.1).
- [x] mop-up (12 строк): time/duration core-модуль-док, path/path-модуль-док, //-комментарии в примерах доков (glob, sync, diff, civil/offset, civil/period, monotonic, time_effect) — fd2d14ec4
- [x] Готово ПОЛНОСТЬЮ: прозы кириллицы в /// = 0. Остаток — только легитимные маркеры
  (Ф.*/П.*/§*/Ш*/[M-…]/D-ссылки/якоря-файлов) + §«Bootstrap-ограничения» в prelude/errors.nv:107,134.
