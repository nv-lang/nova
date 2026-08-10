# -*- coding: utf-8 -*-
u"""Minimal local SOCKS5 server (CONNECT only) + HTTP target.

Lets the bridge be smoke-tested end to end WITHOUT a live external proxy
and WITHOUT credentials: bridge -> this SOCKS5 stub -> local HTTP target,
all on loopback, nothing leaves the machine. See `smoke.sh` in this same
directory for the one-command driver that uses this stub (registry 221.1
#548 -- the flagship example had no way to verify the relay path without a
real, password-protected proxy, so nobody ran it).

Supports both SOCKS5 auth methods (RFC 1928 SS3): 0x00 (no auth) and 0x02
(RFC 1929 username/password) -- the bridge sends credentials only when
`SOCKS5_USER`/`SOCKS5_PASS` are set. A real proxy prefers 0x00 when the
client offers it (only requiring a password where the client is willing to
go without one is server-side behavior, not client-side) -- this stub does
the same.

Usage:  python local_socks5_stub.py <socks_port> <http_port>
Prints lines like `SOCKS: connect host:port ok` to stdout -- that is how a
caller tells whether the bridge reached the proxy at all before the relay
stage.
"""
import socket
import struct
import sys
import threading

try:
    from http.server import BaseHTTPRequestHandler, HTTPServer
except ImportError:  # py2
    from BaseHTTPServer import BaseHTTPRequestHandler, HTTPServer


class Target(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def do_GET(self):
        body = b"PROBE-OK"
        self.send_response(200)
        self.send_header("Content-Type", "text/plain")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *a):
        pass


def pipe(a, b):
    try:
        while True:
            data = a.recv(65536)
            if not data:
                break
            b.sendall(data)
    except Exception:
        pass
    finally:
        for s in (a, b):
            try:
                s.shutdown(socket.SHUT_RDWR)
            except Exception:
                pass
            try:
                s.close()
            except Exception:
                pass


def handle(conn):
    try:
        print("SOCKS: accepted")
        sys.stdout.flush()
        head = conn.recv(2)
        print("SOCKS: greeting %r" % head)
        sys.stdout.flush()
        if len(head) < 2 or head[0:1] != b"\x05":
            conn.close()
            return
        nmeth = head[1] if isinstance(head[1], int) else ord(head[1])
        methods = conn.recv(nmeth)
        print("SOCKS: methods %r" % methods)
        sys.stdout.flush()
        if b"\x00" not in methods and b"\x02" in methods:
            conn.sendall(b"\x05\x02")
            v = conn.recv(1)
            ulen = ord(conn.recv(1))
            conn.recv(ulen)
            plen = ord(conn.recv(1))
            conn.recv(plen)
            conn.sendall(b"\x01\x00")
        else:
            conn.sendall(b"\x05\x00")

        req = conn.recv(4)
        if len(req) < 4:
            conn.close()
            return
        atyp = req[3] if isinstance(req[3], int) else ord(req[3])
        if atyp == 1:
            host = socket.inet_ntoa(conn.recv(4))
        elif atyp == 3:
            ln = ord(conn.recv(1))
            host = conn.recv(ln).decode("ascii")
        else:
            conn.close()
            return
        port = struct.unpack(">H", conn.recv(2))[0]

        try:
            up = socket.create_connection((host, port), 10)
            conn.sendall(b"\x05\x00\x00\x01" + socket.inet_aton("0.0.0.0") + struct.pack(">H", 0))
            print("SOCKS: connect %s:%d ok" % (host, port))
            sys.stdout.flush()
        except Exception as e:
            conn.sendall(b"\x05\x01\x00\x01" + socket.inet_aton("0.0.0.0") + struct.pack(">H", 0))
            print("SOCKS: connect %s:%d FAILED (%s)" % (host, port, e))
            sys.stdout.flush()
            conn.close()
            return

        threading.Thread(target=pipe, args=(conn, up)).start()
        threading.Thread(target=pipe, args=(up, conn)).start()
    except Exception as e:
        print("SOCKS: handler error %s" % e)
        sys.stdout.flush()
        try:
            conn.close()
        except Exception:
            pass


def main():
    socks_port = int(sys.argv[1])
    http_port = int(sys.argv[2])

    httpd = HTTPServer(("127.0.0.1", http_port), Target)
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    print("HTTP target on 127.0.0.1:%d" % http_port)

    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind(("127.0.0.1", socks_port))
    srv.listen(16)
    print("SOCKS5 on 127.0.0.1:%d" % socks_port)
    sys.stdout.flush()
    while True:
        conn, _ = srv.accept()
        threading.Thread(target=handle, args=(conn,), daemon=True).start()


if __name__ == "__main__":
    main()
