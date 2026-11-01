"""The service's proxy on 127.0.0.1: everything Kodi asks pley for goes
through it, and it adds the access token.

pley takes an app's token in the Authorization header only, never in an
address, and it lasts an hour. Kodi's player and its artwork cache only
know addresses: the proxy gives them addresses of its own,
http://127.0.0.1:<port>/<secret>/api/…, and asks the server with the
current token, renewing it when the server refuses it. The secret keeps
other programs (and web pages) on this computer out.
"""

from __future__ import annotations

import base64
import hmac
import http.client
import json
import mimetypes
import os
import re
import socket
import threading
import time
import urllib.parse

# Python 3.8's http.server reads the system's MIME types as it loads (on
# Windows from the registry, about half a second of a Kodi start); the
# proxy never uses them.
mimetypes.inited = True
import http.server  # noqa: E402

from . import api
from .auth import SignedOut, Tokens

# What passes from Kodi to the server, and back.
REQUEST_HEADERS = ("range", "if-range", "if-none-match", "if-modified-since", "content-type", "accept")
RESPONSE_HEADERS = (
    "content-type",
    "content-length",
    "content-range",
    "accept-ranges",
    "etag",
    "last-modified",
    "cache-control",
    "x-pley-complete",
)
# A live channel's: nothing of a length or of ranges.
LIVE_HEADERS = ("content-type", "cache-control")
MAX_BODY = 1 << 20
CHUNK = 256 * 1024
# Segments of a conversion can take a while to be made.
UPSTREAM_TIMEOUT = 120
# What the server knows of this Kodi's network, for Automatic's start
# (playback.starting_ceiling): kept this long, so a play doesn't wait for it.
NETWORK = "/api/playback/network"
NETWORK_KEEP = 600.0


def new_secret() -> str:
    """192 random bits, as secrets.token_urlsafe(24) makes them (without
    loading what secrets loads)."""
    return base64.urlsafe_b64encode(os.urandom(24)).decode("ascii").rstrip("=")


class Proxy:
    """Serves on 127.0.0.1:`port` (another one when it is taken) until
    `stop`. `on_stopped(message)` hears of streams an administrator
    stopped; `on_busy(message, connections, path)` of a live channel
    refused because every connection of its source is in use, `on_ended()`
    of one the server ended. `meter` (autoquality.Meter) counts what streams bring, for
    Automatic."""

    def __init__(
        self, tokens: Tokens, user_agent: str, secret: str, port: int = 0, on_stopped=None, meter=None, on_busy=None, on_ended=None
    ):
        self.tokens = tokens
        self.user_agent = user_agent
        self.secret = secret
        self.on_stopped = on_stopped
        self.on_busy = on_busy
        self.on_ended = on_ended
        self.meter = meter
        self.server = None
        self.thread = None
        self.wanted_port = port
        # Every socket in use, Kodi's and the server's: Kodi waits for each
        # of the service's threads when it quits, and kills the service
        # after 5 seconds.
        self.lock = threading.Lock()
        self.sockets = set()
        self.network = NetworkMemo()

    @property
    def port(self) -> int:
        return self.server.server_address[1]

    @property
    def base(self) -> str:
        return "http://127.0.0.1:%d/%s" % (self.port, self.secret)

    def start(self) -> int:
        handler = type("Handler", (_Handler,), {"proxy": self})
        try:
            self.server = _Server(("127.0.0.1", self.wanted_port), handler)
        except OSError:
            self.server = _Server(("127.0.0.1", 0), handler)
        self.thread = threading.Thread(target=self.server.serve_forever, name="pley-proxy", daemon=True)
        self.thread.start()
        return self.port

    def stop(self) -> None:
        if self.server:
            self.server.shutdown()
            self.server.server_close()
        with self.lock:
            open_now, self.sockets = self.sockets, set()
        for sock in open_now:
            hard_close(sock)
        if self.thread:
            self.thread.join(5)

    def opened(self, sock) -> None:
        with self.lock:
            self.sockets.add(sock)

    def closed(self, sock) -> None:
        with self.lock:
            self.sockets.discard(sock)


class NetworkMemo:
    """The server's answers about this Kodi's network (GET and PUT
    /api/playback/network) as (status, body), for each sign-in (its
    identity) and client id, kept NETWORK_KEEP seconds. Every ceiling
    Automatic sets is told to the server through the proxy, whose answer
    replaces what is kept; one the server didn't take leaves nothing kept,
    so the next play asks again."""

    def __init__(self, clock=time.monotonic, keep: float = NETWORK_KEEP):
        self.clock = clock
        self.keep = keep
        self.lock = threading.Lock()
        self.kept = {}

    def get(self, key):
        with self.lock:
            found = self.kept.get(key)
            if found is None or self.clock() - found[0] > self.keep:
                self.kept.pop(key, None)
                return None
            return found[1]

    def put(self, key, answer) -> None:
        with self.lock:
            self.kept[key] = (self.clock(), answer)

    def drop(self, key) -> None:
        with self.lock:
            self.kept.pop(key, None)


def network_key(method: str, path: str, body, identity):
    """What a request about the network is kept under: (identity, client
    id); None for any other request, or one without a client id."""
    route, _, query = path.partition("?")
    if route != NETWORK or method not in ("GET", "PUT") or identity is None:
        return None
    if method == "GET":
        client = urllib.parse.parse_qs(query).get("client_id", [""])[0]
    else:
        try:
            client = json.loads((body or b"").decode("utf-8")).get("client_id") or ""
        except (ValueError, AttributeError):
            return None
    return (identity, client) if isinstance(client, str) and client else None


class _Server(http.server.ThreadingHTTPServer):
    daemon_threads = True
    # Kodi restarted at once finds its port again (Linux holds a closed
    # one for a minute without this; on Windows it would let two take it).
    allow_reuse_address = os.name != "nt"

    def process_request(self, request, client_address):
        thread = threading.Thread(
            target=self.process_request_thread, args=(request, client_address), name="pley-proxy-request", daemon=True
        )
        thread.start()

    def handle_error(self, request, client_address):
        # Connections Kodi or the proxy closed: nothing to report.
        pass


class _Upstream:
    """One request to the server, its socket known to the proxy until it
    is closed (http.client hands the socket to the answer and forgets it)."""

    def __init__(self, proxy: Proxy, server: str):
        self.proxy = proxy
        self.server = server
        self.conn = None
        self.sock = None
        self.response = None

    def send(self, method: str, path: str, body, headers: dict, token: str) -> http.client.HTTPResponse:
        sent = dict(headers)
        sent["Authorization"] = "Bearer " + token
        # Connected within seconds, or the server is out of reach; answers
        # may take longer (a conversion's segments).
        self.conn = api.connect(self.server, UPSTREAM_TIMEOUT)
        self.sock = self.conn.sock
        self.proxy.opened(self.sock)
        self.conn.request(method, path, body=body, headers=sent)
        self.response = self.conn.getresponse()
        return self.response

    def close(self) -> None:
        if self.response is not None:
            self.response.close()
        if self.conn is not None:
            self.conn.close()
        if self.sock is not None:
            self.proxy.closed(self.sock)
            self.sock.close()


class _Handler(http.server.BaseHTTPRequestHandler):
    proxy: Proxy
    protocol_version = "HTTP/1.1"
    # Idle connections from Kodi end after a minute.
    timeout = 60

    def setup(self):
        super().setup()
        self.proxy.opened(self.connection)

    def finish(self):
        try:
            super().finish()
        finally:
            self.proxy.closed(self.connection)

    def do_GET(self):
        self.forward()

    def do_HEAD(self):
        self.forward()

    def do_POST(self):
        self.forward()

    def do_PUT(self):
        self.forward()

    def do_PATCH(self):
        self.forward()

    def do_DELETE(self):
        self.forward()

    def log_message(self, format, *args):
        pass

    def answer(self, status: int, body: dict) -> None:
        data = json.dumps(body).encode("utf-8")
        try:
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            if self.command != "HEAD":
                self.wfile.write(data)
        except OSError:
            # Kodi is gone (or the proxy is stopping).
            self.close_connection = True

    def forward(self) -> None:
        proxy = self.proxy
        host = (self.headers.get("Host") or "").lower()
        if host not in ("127.0.0.1:%d" % proxy.port, "localhost:%d" % proxy.port):
            return self.answer(403, {"code": "forbidden", "message": "not from this computer"})
        prefix = "/" + proxy.secret + "/"
        # In time that doesn't tell how much of it was right.
        if not hmac.compare_digest(self.path[: len(prefix)].encode("utf-8"), prefix.encode("utf-8")):
            return self.answer(404, {"code": "not_found", "message": "not found"})
        rest = self.path[len(prefix) - 1 :]
        if rest == "/status":
            data = proxy.tokens.current()
            return self.answer(200, {"signed_in": bool(data), "server": data["server"] if data else None})
        if rest == "/sign-out" and self.command == "POST":
            # Here, under the refresh lock: a refresh can't bring it back.
            proxy.tokens.sign_out()
            return self.answer(200, {"signed_in": False, "server": None})
        if rest == "/switch-profile" and self.command == "POST":
            # Here too, as the service alone renews the tokens: {profile, pin}.
            return self.switch_profile()
        if not rest.startswith("/api/"):
            return self.answer(404, {"code": "not_found", "message": "not found"})
        rest = server_path(rest)
        length = self.body_length()
        if length is None:
            return self.answer(400, {"code": "bad_request", "message": "bad Content-Length"})
        if length > MAX_BODY:
            return self.answer(413, {"code": "bad_request", "message": "too large"})
        body = self.rfile.read(length) if length else None
        network = network_key(self.command, rest, body, proxy.tokens.identity())
        if network is not None:
            if self.command == "GET":
                kept = proxy.network.get(network)
                if kept is not None:
                    return self.send_network(*kept)
            else:
                proxy.network.drop(network)
        headers = {"User-Agent": proxy.user_agent}
        for name in REQUEST_HEADERS:
            if self.headers.get(name):
                headers[name] = self.headers[name]
        upstream = None
        try:
            try:
                token = proxy.tokens.access()
                server = proxy.tokens.server()
                upstream = _Upstream(proxy, server)
                response = upstream.send(self.command, rest, body, headers, token)
                if response.status == 401:
                    upstream.close()
                    token = proxy.tokens.refreshed(token)
                    upstream = _Upstream(proxy, server)
                    response = upstream.send(self.command, rest, body, headers, token)
            except SignedOut:
                return self.answer(401, {"code": "signed_out", "message": "Pley is signed out"})
            except (api.ApiError, OSError, http.client.HTTPException) as e:
                return self.answer(502, {"code": "unreachable", "message": str(e)})
            if network is not None:
                return self.relay_network(response, network)
            self.relay(response, rest)
        finally:
            if upstream:
                upstream.close()

    def body_length(self):
        """The request's Content-Length: 0 without one, None when it isn't a
        whole number from 0 (a negative one would read to the end)."""
        given = (self.headers.get("Content-Length") or "0").strip()
        return int(given) if given.isdigit() and len(given) <= 12 else None

    def switch_profile(self) -> None:
        """Acts as another profile from now on (Tokens.switch_profile): 200,
        or the refusal as {code, message}, its code the server's reason."""
        length = self.body_length()
        try:
            if length is None:
                raise ValueError("bad Content-Length")
            asked = json.loads(self.rfile.read(min(length, 4096)).decode("utf-8")) if length else {}
            profile = int(asked["profile"])
        except (ValueError, KeyError, TypeError, AttributeError):
            return self.answer(400, {"code": "bad_request", "message": "which profile?"})
        pin = asked.get("pin")
        try:
            self.proxy.tokens.switch_profile(profile, str(pin) if pin else None)
        except SignedOut:
            return self.answer(401, {"code": "signed_out", "message": "Pley is signed out"})
        except api.ApiError as e:
            if e.status == 0:
                return self.answer(502, {"code": "unreachable", "message": e.message})
            return self.answer(400 if e.status < 500 else 502, {"code": e.code, "message": e.message})
        except (OSError, http.client.HTTPException) as e:
            return self.answer(502, {"code": "unreachable", "message": str(e)})
        return self.answer(200, {"switched": True})

    def location(self, location):
        """Where the server sends Kodi (a collection's picture falling back to
        its titles'), as an address of the proxy's; None for elsewhere."""
        if not location:
            return None
        server = (self.proxy.tokens.server() or "").rstrip("/")
        if server and location.startswith(server + "/"):
            location = location[len(server) :]
        if not location.startswith("/api/"):
            return None
        return "/" + self.proxy.secret + location

    def relay_network(self, response: http.client.HTTPResponse, key) -> None:
        """An answer about the network, kept when it is the server's word:
        its answer, or a 404 from a server from before it (asked again
        only once that is old)."""
        try:
            data = response.read()
        except (OSError, http.client.HTTPException) as e:
            return self.answer(502, {"code": "unreachable", "message": str(e)})
        if response.status == 404 or (response.status == 200 and is_object(data)):
            self.proxy.network.put(key, (response.status, data))
        self.send_network(response.status, data)

    def send_network(self, status: int, data: bytes) -> None:
        try:
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
        except OSError:
            self.close_connection = True

    def relay(self, response: http.client.HTTPResponse, path: str = "") -> None:
        try:
            # A stream stopped by the server; a channel whose source has no
            # connection free, between the plugin's asking and Kodi's.
            heard = self.proxy.on_stopped if response.status == 410 else None
            if response.status == 409 and is_live_stream(path):
                heard = self.proxy.on_busy
            if heard:
                data = response.read()
                try:
                    body = json.loads(data.decode("utf-8"))
                    body = body if isinstance(body, dict) else {}
                except ValueError:
                    body = {}
                if response.status == 409:
                    # What holds each connection, and the channel asked for.
                    heard(body.get("message"), body.get("connections") or [], path)
                else:
                    heard(body.get("message"))
                self.send_response(response.status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)
                return
            self.send_response(response.status)
            live = is_live_stream(path)
            # A live channel has no length and no ranges: Kodi's player
            # then neither pauses nor seeks it (Live TV has neither).
            length = None if live else response.getheader("Content-Length")
            for name in LIVE_HEADERS if live else RESPONSE_HEADERS:
                value = response.getheader(name)
                if value is not None:
                    self.send_header(name, value)
            location = self.location(response.getheader("Location"))
            if location:
                self.send_header("Location", location)
            if length is None and self.command != "HEAD" and response.status not in (204, 304):
                # Without a length the answer ends when the connection does.
                self.send_header("Connection", "close")
                self.close_connection = True
            self.end_headers()
            if self.command == "HEAD" or response.status in (204, 304):
                return
            # A live channel goes on as it comes: a read waits for no more
            # than the server has sent (a radio channel takes a quarter of a
            # minute to send a CHUNK), and Automatic doesn't measure it.
            read = response.read1 if live else response.read
            # A stream's bytes are counted as they come, and its whole
            # answer over the time it took, Kodi's taking them included.
            meter = self.proxy.meter if response.status in (200, 206) and is_media(response) and not live else None
            began = time.monotonic()
            size = 0
            while True:
                chunk = read(CHUNK)
                if not chunk:
                    break
                if meter is not None:
                    meter.chunk(path, len(chunk))
                    size += len(chunk)
                self.wfile.write(chunk)
            if meter is not None:
                meter.answered(path, size, time.monotonic() - began)
            if live and response.status == 200 and self.proxy.on_ended:
                # The server ended the channel (an administrator stopped
                # it, the provider gave up): said before Kodi reads the
                # end, which would play the next channel of its queue.
                self.proxy.on_ended()
        except (OSError, http.client.HTTPException):
            # Kodi let go (a seek, a stop), the server did, or the proxy is
            # stopping: the connection ends, and Kodi asks again from where
            # it got to.
            self.close_connection = True


def is_object(data: bytes) -> bool:
    try:
        return isinstance(json.loads(data.decode("utf-8")), dict)
    except ValueError:
        return False


def is_live_stream(path: str) -> bool:
    """A live channel's stream (livetv.stream_path), which never ends."""
    route = path.partition("?")[0]
    return route.startswith("/api/live/channels/") and route.endswith("/stream.ts")


def is_media(response: http.client.HTTPResponse) -> bool:
    """A stream's answer (a file, a segment), not artwork or the API's."""
    kind = (response.getheader("Content-Type") or "").split(";")[0].strip().lower()
    return kind.startswith(("video/", "audio/")) or kind in ("application/octet-stream", "application/mp4")


# A subtitle named for Kodi (3.eng.forced.vtt, 4.eng.sup): Kodi reads a
# side-loaded track's language and flags from its name.
SUBTITLE_NAME = re.compile(r"^(/api/files/\d+/subtitles/\d+)\.[A-Za-z.]+(\.vtt|\.sup)(\?.*)?$")


def server_path(path: str) -> str:
    """The server's own path for one the proxy handed out."""
    named = SUBTITLE_NAME.match(path)
    if named:
        return named.group(1) + named.group(2) + (named.group(3) or "")
    return path


def hard_close(sock: socket.socket) -> None:
    """Closes `sock` under a thread blocked on it, which then wakes with an
    error: shutdown wakes it on Linux and macOS, only closing the socket
    itself does on Windows. Python's close() waits for the files made of
    the socket, so the handle is taken from it and closed."""
    try:
        sock.shutdown(socket.SHUT_RDWR)
    except OSError:
        pass
    try:
        handle = sock.detach()
        if handle != -1:
            socket.socket(fileno=handle).close()
    except OSError:
        pass
