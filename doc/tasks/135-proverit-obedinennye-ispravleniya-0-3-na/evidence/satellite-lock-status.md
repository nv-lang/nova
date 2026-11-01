# Состояние спутников после измерений

Только чтение; файлы не откатывались, новые сборки не запускались. Изменения не включаются в коммит задачи.

## nova-duckdb

```text
?? .vscode/
?? examples/01-hello/nova.lock.toml
?? examples/02-appender-history/nova.lock.toml
?? examples/03-with-tls/nova.lock.toml
```
### examples/01-hello/nova.lock.toml

```diff
diff --git a/examples/01-hello/nova.lock.toml b/examples/01-hello/nova.lock.toml
new file mode 100644
index 0000000..6b7dbc9
--- /dev/null
+++ b/examples/01-hello/nova.lock.toml
@@ -0,0 +1,13 @@
+# nova.lock.toml — generated automatically (Plan 03.1 / D78).
+# Pins exact dependency versions for reproducible builds.
+# Do not edit by hand; commit this file to the repository.
+
+version = 1
+
+[[package]]
+name = "duckdb"
+source = "git"
+git = "https://github.com/nv-lang/nova-duckdb"
+pin = "version:>=0.1.0, <0.2.0"
+version = "0.1.1"
+commit = "ba7b8a1b39cbf05a9c8de03aac1d5f3f06653d97"
```
### examples/02-appender-history/nova.lock.toml

```diff
diff --git a/examples/02-appender-history/nova.lock.toml b/examples/02-appender-history/nova.lock.toml
new file mode 100644
index 0000000..6b7dbc9
--- /dev/null
+++ b/examples/02-appender-history/nova.lock.toml
@@ -0,0 +1,13 @@
+# nova.lock.toml — generated automatically (Plan 03.1 / D78).
+# Pins exact dependency versions for reproducible builds.
+# Do not edit by hand; commit this file to the repository.
+
+version = 1
+
+[[package]]
+name = "duckdb"
+source = "git"
+git = "https://github.com/nv-lang/nova-duckdb"
+pin = "version:>=0.1.0, <0.2.0"
+version = "0.1.1"
+commit = "ba7b8a1b39cbf05a9c8de03aac1d5f3f06653d97"
```
### examples/03-with-tls/nova.lock.toml

```diff
diff --git a/examples/03-with-tls/nova.lock.toml b/examples/03-with-tls/nova.lock.toml
new file mode 100644
index 0000000..0d51643
--- /dev/null
+++ b/examples/03-with-tls/nova.lock.toml
@@ -0,0 +1,21 @@
+# nova.lock.toml — generated automatically (Plan 03.1 / D78).
+# Pins exact dependency versions for reproducible builds.
+# Do not edit by hand; commit this file to the repository.
+
+version = 1
+
+[[package]]
+name = "duckdb"
+source = "git"
+git = "https://github.com/nv-lang/nova-duckdb"
+pin = "version:>=0.2.0, <0.3.0"
+version = "0.2.2"
+commit = "674b12aac4d2bbdb817a9116ed3267d4d60c78fb"
+
+[[package]]
+name = "tls"
+source = "git"
+git = "https://github.com/nv-lang/nova-tls"
+pin = "version:>=0.2.0, <0.3.0"
+version = "0.2.1"
+commit = "01d323b6423d69fd5c54216667134313033f8644"
```
## nova-polaris

```text
 M examples/01-hello/nova.lock.toml
 M examples/02-routing/nova.lock.toml
 M examples/03-json-api/nova.lock.toml
 M examples/04-middleware/nova.lock.toml
 M examples/05-auth/nova.lock.toml
 M examples/06-static-site/nova.lock.toml
 M examples/07-sse-stream/nova.lock.toml
 M examples/08-websocket-echo/nova.lock.toml
 M examples/09-graceful/nova.lock.toml
 M examples/10-mini-service/nova.lock.toml
 M examples/11-json-logs/nova.lock.toml
 M examples/12-https/nova.lock.toml
```
### examples/01-hello/nova.lock.toml

```diff
diff --git a/examples/01-hello/nova.lock.toml b/examples/01-hello/nova.lock.toml
index 6e4c960..4d5a6eb 100644
--- a/examples/01-hello/nova.lock.toml
+++ b/examples/01-hello/nova.lock.toml
@@ -15,7 +15,7 @@ commit = "c4c387fdf145280b7253ede7bf992fe6fbb35359"
 name = "http"
 source = "git"
 git = "https://github.com/nv-lang/nova-http"
-pin = "version:>=0.1.0, <0.2.0"
+pin = "version:>=0.2.0, <0.3.0"
 commit = "53638fba444a1addfe8ab4af0a515d4cdb3e1acd"
 
 [[package]]
```
### examples/02-routing/nova.lock.toml

```diff
diff --git a/examples/02-routing/nova.lock.toml b/examples/02-routing/nova.lock.toml
index 6e4c960..4d5a6eb 100644
--- a/examples/02-routing/nova.lock.toml
+++ b/examples/02-routing/nova.lock.toml
@@ -15,7 +15,7 @@ commit = "c4c387fdf145280b7253ede7bf992fe6fbb35359"
 name = "http"
 source = "git"
 git = "https://github.com/nv-lang/nova-http"
-pin = "version:>=0.1.0, <0.2.0"
+pin = "version:>=0.2.0, <0.3.0"
 commit = "53638fba444a1addfe8ab4af0a515d4cdb3e1acd"
 
 [[package]]
```
### examples/03-json-api/nova.lock.toml

```diff
diff --git a/examples/03-json-api/nova.lock.toml b/examples/03-json-api/nova.lock.toml
index 6e4c960..4d5a6eb 100644
--- a/examples/03-json-api/nova.lock.toml
+++ b/examples/03-json-api/nova.lock.toml
@@ -15,7 +15,7 @@ commit = "c4c387fdf145280b7253ede7bf992fe6fbb35359"
 name = "http"
 source = "git"
 git = "https://github.com/nv-lang/nova-http"
-pin = "version:>=0.1.0, <0.2.0"
+pin = "version:>=0.2.0, <0.3.0"
 commit = "53638fba444a1addfe8ab4af0a515d4cdb3e1acd"
 
 [[package]]
```
### examples/04-middleware/nova.lock.toml

```diff
diff --git a/examples/04-middleware/nova.lock.toml b/examples/04-middleware/nova.lock.toml
index 6e4c960..4d5a6eb 100644
--- a/examples/04-middleware/nova.lock.toml
+++ b/examples/04-middleware/nova.lock.toml
@@ -15,7 +15,7 @@ commit = "c4c387fdf145280b7253ede7bf992fe6fbb35359"
 name = "http"
 source = "git"
 git = "https://github.com/nv-lang/nova-http"
-pin = "version:>=0.1.0, <0.2.0"
+pin = "version:>=0.2.0, <0.3.0"
 commit = "53638fba444a1addfe8ab4af0a515d4cdb3e1acd"
 
 [[package]]
```
### examples/05-auth/nova.lock.toml

```diff
diff --git a/examples/05-auth/nova.lock.toml b/examples/05-auth/nova.lock.toml
index 6e4c960..4d5a6eb 100644
--- a/examples/05-auth/nova.lock.toml
+++ b/examples/05-auth/nova.lock.toml
@@ -15,7 +15,7 @@ commit = "c4c387fdf145280b7253ede7bf992fe6fbb35359"
 name = "http"
 source = "git"
 git = "https://github.com/nv-lang/nova-http"
-pin = "version:>=0.1.0, <0.2.0"
+pin = "version:>=0.2.0, <0.3.0"
 commit = "53638fba444a1addfe8ab4af0a515d4cdb3e1acd"
 
 [[package]]
```
### examples/06-static-site/nova.lock.toml

```diff
diff --git a/examples/06-static-site/nova.lock.toml b/examples/06-static-site/nova.lock.toml
index 6e4c960..4d5a6eb 100644
--- a/examples/06-static-site/nova.lock.toml
+++ b/examples/06-static-site/nova.lock.toml
@@ -15,7 +15,7 @@ commit = "c4c387fdf145280b7253ede7bf992fe6fbb35359"
 name = "http"
 source = "git"
 git = "https://github.com/nv-lang/nova-http"
-pin = "version:>=0.1.0, <0.2.0"
+pin = "version:>=0.2.0, <0.3.0"
 commit = "53638fba444a1addfe8ab4af0a515d4cdb3e1acd"
 
 [[package]]
```
### examples/07-sse-stream/nova.lock.toml

```diff
diff --git a/examples/07-sse-stream/nova.lock.toml b/examples/07-sse-stream/nova.lock.toml
index 6e4c960..4d5a6eb 100644
--- a/examples/07-sse-stream/nova.lock.toml
+++ b/examples/07-sse-stream/nova.lock.toml
@@ -15,7 +15,7 @@ commit = "c4c387fdf145280b7253ede7bf992fe6fbb35359"
 name = "http"
 source = "git"
 git = "https://github.com/nv-lang/nova-http"
-pin = "version:>=0.1.0, <0.2.0"
+pin = "version:>=0.2.0, <0.3.0"
 commit = "53638fba444a1addfe8ab4af0a515d4cdb3e1acd"
 
 [[package]]
```
### examples/08-websocket-echo/nova.lock.toml

```diff
diff --git a/examples/08-websocket-echo/nova.lock.toml b/examples/08-websocket-echo/nova.lock.toml
index 6e4c960..4d5a6eb 100644
--- a/examples/08-websocket-echo/nova.lock.toml
+++ b/examples/08-websocket-echo/nova.lock.toml
@@ -15,7 +15,7 @@ commit = "c4c387fdf145280b7253ede7bf992fe6fbb35359"
 name = "http"
 source = "git"
 git = "https://github.com/nv-lang/nova-http"
-pin = "version:>=0.1.0, <0.2.0"
+pin = "version:>=0.2.0, <0.3.0"
 commit = "53638fba444a1addfe8ab4af0a515d4cdb3e1acd"
 
 [[package]]
```
### examples/09-graceful/nova.lock.toml

```diff
diff --git a/examples/09-graceful/nova.lock.toml b/examples/09-graceful/nova.lock.toml
index 6e4c960..4d5a6eb 100644
--- a/examples/09-graceful/nova.lock.toml
+++ b/examples/09-graceful/nova.lock.toml
@@ -15,7 +15,7 @@ commit = "c4c387fdf145280b7253ede7bf992fe6fbb35359"
 name = "http"
 source = "git"
 git = "https://github.com/nv-lang/nova-http"
-pin = "version:>=0.1.0, <0.2.0"
+pin = "version:>=0.2.0, <0.3.0"
 commit = "53638fba444a1addfe8ab4af0a515d4cdb3e1acd"
 
 [[package]]
```
### examples/10-mini-service/nova.lock.toml

```diff
diff --git a/examples/10-mini-service/nova.lock.toml b/examples/10-mini-service/nova.lock.toml
index 6e4c960..4d5a6eb 100644
--- a/examples/10-mini-service/nova.lock.toml
+++ b/examples/10-mini-service/nova.lock.toml
@@ -15,7 +15,7 @@ commit = "c4c387fdf145280b7253ede7bf992fe6fbb35359"
 name = "http"
 source = "git"
 git = "https://github.com/nv-lang/nova-http"
-pin = "version:>=0.1.0, <0.2.0"
+pin = "version:>=0.2.0, <0.3.0"
 commit = "53638fba444a1addfe8ab4af0a515d4cdb3e1acd"
 
 [[package]]
```
### examples/11-json-logs/nova.lock.toml

```diff
diff --git a/examples/11-json-logs/nova.lock.toml b/examples/11-json-logs/nova.lock.toml
index 6e4c960..4d5a6eb 100644
--- a/examples/11-json-logs/nova.lock.toml
+++ b/examples/11-json-logs/nova.lock.toml
@@ -15,7 +15,7 @@ commit = "c4c387fdf145280b7253ede7bf992fe6fbb35359"
 name = "http"
 source = "git"
 git = "https://github.com/nv-lang/nova-http"
-pin = "version:>=0.1.0, <0.2.0"
+pin = "version:>=0.2.0, <0.3.0"
 commit = "53638fba444a1addfe8ab4af0a515d4cdb3e1acd"
 
 [[package]]
```
### examples/12-https/nova.lock.toml

```diff
diff --git a/examples/12-https/nova.lock.toml b/examples/12-https/nova.lock.toml
index a57a3c5..a8cc85f 100644
--- a/examples/12-https/nova.lock.toml
+++ b/examples/12-https/nova.lock.toml
@@ -15,7 +15,7 @@ commit = "c4c387fdf145280b7253ede7bf992fe6fbb35359"
 name = "http"
 source = "git"
 git = "https://github.com/nv-lang/nova-http"
-pin = "version:>=0.1.0, <0.2.0"
+pin = "version:>=0.2.0, <0.3.0"
 commit = "53638fba444a1addfe8ab4af0a515d4cdb3e1acd"
 
 [[package]]
```
## nova-postgres

```text
(чисто)
```
## nova-sdl

```text
(чисто)
```
## nova-tls

```text
(чисто)
```
## nova-http

```text
(чисто)
```
## nova-bignum

```text
(чисто)
```
## nova-compress

```text
(чисто)
```
## nova-socks

```text
(чисто)
```
