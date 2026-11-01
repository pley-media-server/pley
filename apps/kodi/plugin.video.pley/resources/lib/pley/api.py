"""HTTP to a pley server, or to the service's proxy in front of one."""

from __future__ import annotations

import json
import socket
import urllib.parse

DEFAULT_PORT = 7539
TIMEOUT = 20
# A server that is off answers nothing at all: waiting longer than this to
# connect tells nobody more.
CONNECT_TIMEOUT = 4


class ApiError(Exception):
    """A refusal from the server ({code, message}, or OAuth's {error,
    error_description}), or no answer at all (status 0). `body` is the
    refusal's JSON whole: some say more (when to ask again, the request as
    it is now)."""

    def __init__(self, status: int, code: str, message: str, body: dict | None = None):
        super().__init__(message)
        self.status = status
        self.code = code
        self.message = message
        self.body = body or {}


class Response:
    def __init__(self, status: int, headers: dict, body: bytes):
        self.status = status
        self.headers = headers
        self.body = body

    def json(self):
        return json.loads(self.body.decode("utf-8")) if self.body else None


def normalize(address: str) -> str:
    """A server's address as pley writes its issuer: scheme, lower-case host
    and port only, the default port left out. An address typed without a
    scheme is http, and http without a port is pley's port, 7539."""
    address = address.strip()
    if "://" not in address:
        address = "http://" + address
    parts = urllib.parse.urlsplit(address)
    if parts.scheme not in ("http", "https") or not parts.hostname or parts.username or parts.password:
        raise ValueError("not a server address: " + address)
    if parts.path.strip("/") or parts.query or parts.fragment:
        raise ValueError("a server address has no path: " + address)
    try:
        port = parts.port
    except ValueError:
        raise ValueError("not a server address: " + address)
    default = 443 if parts.scheme == "https" else 80
    if port is None:
        port = default if parts.scheme == "https" else DEFAULT_PORT
    host = parts.hostname.lower()
    if ":" in host:
        host = "[" + host + "]"
    return "{}://{}{}".format(parts.scheme, host, "" if port == default else ":%d" % port)


# What the add-on can't reach a server over (the other apps' words).
UNENCRYPTED = (
    "this server's address isn't on your network and its connection isn't encrypted: "
    "open it by its https address to sign in"
)


class Unencrypted(OSError):
    """Plain http to an address on the internet: refused before anything
    (a code, a token) is sent over it."""

    def __init__(self):
        super().__init__(UNENCRYPTED)


_PRIVATE = None


def is_private_address(address: str) -> bool:
    """Whether `address` (as a socket names its peer) is this computer or on
    its own network: loopback, RFC 1918, link-local, CGNAT (Tailscale) or an
    IPv6 unique local address, where plain http is how pley is reached."""
    import ipaddress

    global _PRIVATE
    if _PRIVATE is None:
        _PRIVATE = [
            ipaddress.ip_network(n)
            for n in (
                "127.0.0.0/8", "10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16", "169.254.0.0/16",
                "100.64.0.0/10", "::1/128", "fc00::/7", "fe80::/10",
            )
        ]
    try:
        ip = ipaddress.ip_address(address.split("%", 1)[0])
    except ValueError:
        return False
    if ip.version == 6 and ip.ipv4_mapped is not None:
        ip = ip.ipv4_mapped
    return any(ip.version == net.version and ip in net for net in _PRIVATE)


def check_unencrypted(scheme: str, sock) -> None:
    """Over plain http, the connection must be to a private address
    (checked on the socket itself, so a name that turns out to be on the
    internet is refused too): tokens and sign-in codes never cross the
    internet unencrypted. Raises Unencrypted, the socket closed."""
    if scheme == "https":
        return
    try:
        peer = sock.getpeername()[0]
    except (OSError, IndexError, TypeError):
        peer = ""
    if not is_private_address(peer):
        sock.close()
        raise Unencrypted()


def connect(base: str, timeout: float = TIMEOUT):
    """An http.client connection to `base`, made within CONNECT_TIMEOUT, that
    then waits up to `timeout` for answers. Plain http only to addresses on
    this network (check_unencrypted)."""
    # Loaded when first needed: with ssl and email it is most of what a
    # listing loads, and the plugin's own requests go without it (local).
    import http.client

    parts = urllib.parse.urlsplit(base)
    connect_timeout = min(CONNECT_TIMEOUT, timeout)
    if parts.scheme == "https":
        conn = http.client.HTTPSConnection(parts.hostname, parts.port, timeout=connect_timeout, context=tls())
    else:
        conn = http.client.HTTPConnection(parts.hostname, parts.port, timeout=connect_timeout)
    conn.connect()
    check_unencrypted(parts.scheme, conn.sock)
    conn.sock.settimeout(timeout)
    return conn


_tls = []


def tls():
    """One TLS context for every https connection: making one reads the
    system's certificates again (about 70 ms each, on Windows)."""
    if not _tls:
        import ssl

        _tls.append(ssl.create_default_context())
    return _tls[0]


def error_of(response: Response) -> ApiError:
    try:
        body = response.json() or {}
    except ValueError:
        body = {}
    if not isinstance(body, dict):
        body = {}
    code = body.get("code") or body.get("error") or "http_%d" % response.status
    message = body.get("message") or body.get("error_description") or "the server answered %d" % response.status
    return ApiError(response.status, code, message, body)


class Client:
    """Requests to `base` (a server, or the proxy's address with its secret
    path), with `token` as a bearer token when given."""

    def __init__(
        self, base: str, user_agent: str, token: str | None = None, timeout: float = TIMEOUT, local: bool = False
    ):
        self.base = base.rstrip("/")
        self.prefix = urllib.parse.urlsplit(self.base).path
        self.user_agent = user_agent
        self.token = token
        self.timeout = timeout
        # The service's proxy, on this computer: plain HTTP over a socket.
        self.local = local

    def request(
        self,
        method: str,
        path: str,
        query: dict | None = None,
        json_body=None,
        form: dict | None = None,
        headers: dict | None = None,
    ) -> Response:
        target = self.prefix + path
        if query:
            pairs = [(k, v) for k, v in query.items() if v is not None]
            target += ("&" if "?" in target else "?") + urllib.parse.urlencode(pairs)
        sent = {"User-Agent": self.user_agent, "Accept": "application/json"}
        if self.token:
            sent["Authorization"] = "Bearer " + self.token
        body = None
        if json_body is not None:
            body = json.dumps(json_body).encode("utf-8")
            sent["Content-Type"] = "application/json"
        elif form is not None:
            body = urllib.parse.urlencode(form).encode("utf-8")
            sent["Content-Type"] = "application/x-www-form-urlencoded"
        sent.update(headers or {})
        if self.local:
            return local_request(self.base, method, target, sent, body, self.timeout)
        import http.client

        conn = None
        try:
            conn = connect(self.base, self.timeout)
            conn.request(method, target, body=body, headers=sent)
            answer = conn.getresponse()
            data = answer.read()
            return Response(answer.status, {k.lower(): v for k, v in answer.getheaders()}, data)
        except Unencrypted as e:
            raise ApiError(0, "unencrypted", str(e))
        except (OSError, http.client.HTTPException) as e:
            raise ApiError(0, "unreachable", str(e))
        finally:
            if conn is not None:
                conn.close()

    def json(self, method: str, path: str, **kwargs):
        response = self.request(method, path, **kwargs)
        if response.status >= 400:
            raise error_of(response)
        try:
            return response.json()
        except ValueError:
            # A web page, a router's: not pley.
            raise ApiError(response.status, "not_pley", "the answer isn't Pley's")

    def measure(self, path: str, query: dict, test, deadline: float, clock=None) -> int:
        """GETs `path`, handing each piece of its body to `test` (an
        autoquality.SpeedTest) as it comes, until `test.done` or the clock
        reaches `deadline`; then the connection is dropped, which ends the
        answer on the server. The answer's status (0 for none)."""
        import time

        clock = clock or time.monotonic
        target = self.prefix + path + "?" + urllib.parse.urlencode(query)
        headers = {"User-Agent": self.user_agent, "Accept": "application/octet-stream", "Connection": "close"}
        if self.token:
            headers["Authorization"] = "Bearer " + self.token
        parts = urllib.parse.urlsplit(self.base)
        port = parts.port or (443 if parts.scheme == "https" else 80)

        def left():
            return max(deadline - clock(), 0.01)

        sock = None
        try:
            sock = socket.create_connection((parts.hostname, port), timeout=min(CONNECT_TIMEOUT, left()))
            check_unencrypted(parts.scheme, sock)
            if parts.scheme == "https":
                sock = tls().wrap_socket(sock, server_hostname=parts.hostname)
            lines = ["GET %s HTTP/1.1" % target, "Host: %s" % parts.netloc] + ["%s: %s" % kv for kv in headers.items()]
            sock.settimeout(left())
            sock.sendall(("\r\n".join(lines) + "\r\n\r\n").encode("latin-1"))
            head = b""
            while b"\r\n\r\n" not in head:
                sock.settimeout(left())
                piece = sock.recv(65536)
                if not piece:
                    return 0
                head += piece
                if len(head) > 65536:
                    return 0
            head, _, rest = head.partition(b"\r\n\r\n")
            try:
                status = int(head.split(b" ")[1])
            except (IndexError, ValueError):
                return 0
            if status != 200:
                return status
            test.chunk(clock(), len(rest))
            while not test.done(clock()) and clock() < deadline:
                sock.settimeout(max(test.until(deadline) - clock(), 0.01))
                try:
                    piece = sock.recv(65536)
                except socket.timeout:
                    break
                if not piece:
                    break
                test.chunk(clock(), len(piece))
            return status
        except OSError:
            return 0
        finally:
            if sock is not None:
                sock.close()

    def get(self, path: str, **query):
        return self.json("GET", path, query=query or None)

    def post(self, path: str, body=None):
        return self.json("POST", path, json_body=body if body is not None else {})

    def put(self, path: str, body=None):
        return self.json("PUT", path, json_body=body if body is not None else {})

    def patch(self, path: str, body=None):
        return self.json("PATCH", path, json_body=body if body is not None else {})

    def delete(self, path: str):
        return self.json("DELETE", path)


def path_in(base: str, url: str) -> str:
    """The proxy's address for an address the server gave ("/api/…")."""
    parts = urllib.parse.urlsplit(url)
    return base + parts.path + ("?" + parts.query if parts.query else "")


def local_request(base: str, method: str, target: str, headers: dict, body, timeout: float) -> Response:
    """One request to the service's proxy: HTTP/1.1 with the connection
    closed after the answer, which the proxy always sends whole (with its
    length, or ending with the connection)."""
    parts = urllib.parse.urlsplit(base)
    lines = ["%s %s HTTP/1.1" % (method, target), "Host: %s:%d" % (parts.hostname, parts.port), "Connection: close"]
    lines += ["%s: %s" % (name, value) for name, value in headers.items()]
    if body is not None:
        lines.append("Content-Length: %d" % len(body))
    request = ("\r\n".join(lines) + "\r\n\r\n").encode("latin-1") + (body or b"")
    chunks = []
    try:
        with socket.create_connection((parts.hostname, parts.port), timeout=CONNECT_TIMEOUT) as sock:
            sock.settimeout(timeout)
            sock.sendall(request)
            while True:
                chunk = sock.recv(65536)
                if not chunk:
                    break
                chunks.append(chunk)
    except OSError as e:
        # The proxy is the service's, on this computer: it isn't running.
        raise ApiError(0, "service_off", str(e))
    head, _, rest = b"".join(chunks).partition(b"\r\n\r\n")
    status_line, *header_lines = head.decode("latin-1").split("\r\n")
    try:
        status = int(status_line.split(" ")[1])
    except (IndexError, ValueError):
        raise ApiError(0, "service_off", "no answer from Pley's service")
    answer_headers = {}
    for line in header_lines:
        name, _, value = line.partition(":")
        answer_headers[name.strip().lower()] = value.strip()
    length = answer_headers.get("content-length")
    return Response(status, answer_headers, rest[: int(length)] if length and length.isdigit() else rest)


def subtitle_name(url: str, language, forced: bool) -> str:
    """The server's subtitle address `url` (/api/files/7/subtitles/3.vtt,
    3.sup) named for Kodi: 3.eng.forced.vtt."""
    parts = []
    if language and str(language).isalpha():
        parts.append(str(language).lower())
    if forced:
        parts.append("forced")
    extension = url[-4:]
    if not parts or extension not in (".vtt", ".sup"):
        return url
    return url[: -len(extension)] + "." + ".".join(parts) + extension


def together(*calls):
    """The answers of `calls` (functions), asked at once: a slow server's
    round trips overlap. The first refusal is raised."""
    import threading

    answers = [None] * len(calls)
    refusals = []

    def ask(at, call):
        try:
            answers[at] = call()
        except ApiError as e:
            refusals.append(e)

    threads = [threading.Thread(target=ask, args=(at, call), daemon=True) for at, call in enumerate(calls)]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join(70)
    if refusals:
        raise refusals[0]
    return answers
