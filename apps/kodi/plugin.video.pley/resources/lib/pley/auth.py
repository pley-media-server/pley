"""Signing in with a code shown on screen (RFC 8628), and the tokens that
come of it.

The plugin signs in and writes the tokens to a file. From then on only the
service reads and refreshes them, one refresh at a time: each refresh token
works once, and two refreshes with the same one end the sign-in. Plugin
calls reach the server through the service's proxy, which adds the token.

No token goes to the server's address before the server there proves it is
the one signed in to (docs/API.md, "Telling servers apart"): an address can
lead to another device later, on another network or after the router hands
it out again.
"""

from __future__ import annotations

import base64
import binascii
import hashlib
import hmac
import json
import os
import re
import threading
import time

from .api import ApiError, Client

CLIENT_ID = "https://pley-media-server.github.io/pley/oauth/kodi.json"
DEVICE_GRANT = "urn:ietf:params:oauth:grant-type:device_code"
SWITCH_GRANT = "urn:pley:params:oauth:grant-type:switch-profile"
# What the add-on asks to do: browse, play, make and change playlists (Add
# to playlist, a library's Save as…), and switch to the household's other
# profiles ("Who's watching?").
SCOPE = "library play playlists profiles"
# And where the server offers them: ask for movies and shows the libraries
# don't have and follow the profile's requests, and approve and decline what
# people ask for (an administrator's account). A server refuses a sign-in
# that asks for a scope it doesn't know, so these go only where listed.
OFFERED = ("requests", "approvals")
# Access tokens are refreshed this long before they expire.
REFRESH_MARGIN = 300
# A server's proof counts this long for an access token, unless a request to
# the server gets no answer or Kodi wakes from sleep meanwhile.
PROOF_TTL = 30 * 60
PROOF_TIMEOUT = 10
PROOF_ID = re.compile(r"[1-9][0-9]{0,18}")
PROOF_KEY = re.compile(r"[A-Za-z0-9_-]*")
KEY_BYTES = 32
CHALLENGE_BYTES = 16

# Counts the times Kodi may have changed networks: a proof counts on the
# network it was made on.
_network = 0


class SignedOut(Exception):
    """No tokens, or the server ended the sign-in."""


class NotYourServer(ApiError):
    """What answers at the server's address can't prove it is the server
    signed in to: it gets no token, and the add-on treats it as the server
    being out of reach."""

    def __init__(self):
        super().__init__(0, "another_device", "another device answers there")


def network_may_have_changed() -> None:
    """Kodi woke from sleep, maybe on another network: the server proves
    itself again before its next token."""
    global _network
    _network += 1


def b64url(data: bytes) -> str:
    """base64url without padding (RFC 4648 §5)."""
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode("ascii")


def key_of(text) -> bytes | None:
    """The 32 bytes of a proof key written in base64url without padding;
    None for anything else."""
    if not isinstance(text, str) or not PROOF_KEY.fullmatch(text) or len(text) % 4 == 1:
        return None
    try:
        key = base64.urlsafe_b64decode(text + "=" * (-len(text) % 4))
    except (ValueError, binascii.Error):
        return None
    return key if len(key) == KEY_BYTES else None


def proof_from(answer) -> dict:
    """The server's id and the sign-in's proof key in a token answer, when
    it has them and they are written as they should be; {} from a server
    before them."""
    if not isinstance(answer, dict):
        return {}
    server_id, proof_id, proof_key = answer.get("server_id"), answer.get("proof_id"), answer.get("proof_key")
    if not (isinstance(server_id, str) and server_id and isinstance(proof_id, str) and PROOF_ID.fullmatch(proof_id)):
        return {}
    if key_of(proof_key) is None:
        return {}
    return {"server_id": server_id, "proof_id": proof_id, "proof_key": proof_key}


def proof_of(record: dict | None) -> dict:
    """What a kept sign-in has to tell its server by ({} for none)."""
    return proof_from((record or {}).get("proof"))


def proof(key: bytes, server_id: str, address: str, challenge: str) -> str:
    """The proof of `server_id`, asked at `address`, for `challenge` under
    `key`, in base64url without padding."""
    text = "pley-server-proof\n%s\n%s\n%s" % (server_id, address, challenge)
    return b64url(hmac.new(key, text.encode("utf-8"), hashlib.sha256).digest())


def prove(server: str, keys: dict, user_agent: str, random=os.urandom) -> None:
    """Asks the server at `server` (its issuer, as kept) to prove it holds
    `keys`' key, for that address. Raises NotYourServer when what answers
    can't, ApiError (status 0) when nothing answers or the server, or a
    proxy before it, fails for now. http.client follows no redirect: a proof
    counts only from the address itself."""
    challenge = b64url(random(CHALLENGE_BYTES))
    query = {"challenge": challenge, "proof_id": keys["proof_id"]}
    response = Client(server, user_agent, timeout=PROOF_TIMEOUT).request("GET", "/api/server", query=query)
    if response.status >= 500:
        raise ApiError(0, "unreachable", "the server couldn't answer (%d)" % response.status)
    try:
        answer = response.json() if response.status == 200 else None
    except ValueError:
        answer = None
    if not isinstance(answer, dict) or answer.get("id") != keys["server_id"] or not isinstance(answer.get("proof"), str):
        raise NotYourServer()
    expected = proof(key_of(keys["proof_key"]), keys["server_id"], server, challenge)
    if not hmac.compare_digest(answer["proof"].encode("utf-8"), expected.encode("utf-8")):
        raise NotYourServer()


def proves(record: dict, user_agent: str, random=os.urandom) -> bool:
    """Whether the server at `record`'s address proves it holds the
    sign-in's key now (a sign-in without one has nothing to check)."""
    keys = proof_of(record)
    if not keys:
        return True
    try:
        prove(record["server"], keys, user_agent, random)
    except ApiError:
        return False
    return True


def metadata(server: str, user_agent: str) -> dict:
    """The server's OAuth metadata (RFC 8414), checked to be its own: the
    issuer is the address asked (§3.3), and the endpoints are on it."""
    meta = Client(server, user_agent).get("/.well-known/oauth-authorization-server")
    if not isinstance(meta, dict):
        raise ApiError(0, "issuer", "{} isn't a Pley server".format(server))
    if meta.get("issuer") != server:
        raise ApiError(0, "issuer", "the server says it is {}, not {}".format(meta.get("issuer"), server))
    for name in ("token_endpoint", "device_authorization_endpoint", "revocation_endpoint"):
        if not str(meta.get(name, "")).startswith(server + "/"):
            raise ApiError(0, "issuer", "the server's {} isn't on {}".format(name, server))
    return meta


def path_of(server: str, endpoint: str) -> str:
    return endpoint[len(server) :]


def scope_for(meta: dict) -> str:
    """What to ask `meta`'s server for: SCOPE, and those of OFFERED that its
    metadata lists (scopes_supported)."""
    supported = meta.get("scopes_supported") or []
    return " ".join([SCOPE] + [name for name in OFFERED if name in supported])


class DeviceSignIn:
    """One sign-in with a code: `start`, then `poll` every `interval`."""

    def __init__(self, server: str, user_agent: str):
        self.server = server
        self.user_agent = user_agent
        self.meta = metadata(server, user_agent)
        self.scope = scope_for(self.meta)
        self.client = Client(server, user_agent)
        self.interval = 5

    def start(self) -> dict:
        path = path_of(self.server, self.meta["device_authorization_endpoint"])
        started = self.client.json("POST", path, form={"client_id": CLIENT_ID, "scope": self.scope})
        self.interval = max(int(started.get("interval") or 5), 1)
        return started

    def poll(self, device_code: str):
        """("allowed", tokens), or (what to do, None): "pending",
        "slow_down" (the interval is now 5 seconds longer), "denied",
        "expired"."""
        path = path_of(self.server, self.meta["token_endpoint"])
        form = {"grant_type": DEVICE_GRANT, "device_code": device_code, "client_id": CLIENT_ID}
        response = self.client.request("POST", path, form=form)
        if response.status == 200:
            return "allowed", response.json()
        error = (response.json() or {}).get("error") if response.body.startswith(b"{") else None
        if error == "authorization_pending":
            return "pending", None
        if error == "slow_down":
            self.interval += 5
            return "slow_down", None
        if error == "access_denied":
            return "denied", None
        if error in ("expired_token", "invalid_grant"):
            return "expired", None
        raise ApiError(response.status, error or "http_%d" % response.status, "the server refused the code")

    def record(self, tokens: dict, server_name: str, server_id: str, now: float | None = None) -> dict:
        """What the token file keeps of a sign-in, named anew each time
        (Tokens.identity), with the key its server proves itself with."""
        record = {
            "server": self.server,
            "server_name": server_name,
            "server_id": server_id,
            "token_endpoint": self.meta["token_endpoint"],
            "revocation_endpoint": self.meta["revocation_endpoint"],
            "access_token": tokens["access_token"],
            "expires_at": (now if now is not None else time.time()) + int(tokens.get("expires_in") or 3600),
            "refresh_token": tokens["refresh_token"],
            "scope": tokens.get("scope", self.scope),
            "sign_in": binascii.hexlify(os.urandom(8)).decode("ascii"),
        }
        keys = proof_from(tokens)
        if keys:
            record["proof"] = keys
        return record


def write_private(path: str, text: str) -> None:
    """Writes `path` whole, readable by this user only, and on the disk
    before it replaces what was there: a refresh token written and lost to a
    power cut is spent all the same. The plugin and the service each write
    to a name of their own first."""
    temporary = "%s.%s.new" % (path, binascii.hexlify(os.urandom(6)).decode("ascii"))
    fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            f.write(text)
            f.flush()
            os.fsync(f.fileno())
        os.replace(temporary, path)
    except BaseException:
        try:
            os.remove(temporary)
        except OSError:
            pass
        raise


class Store:
    """The token file, written whole and atomically."""

    def __init__(self, path: str):
        self.path = path

    def stamp(self):
        try:
            stat = os.stat(self.path)
        except OSError:
            return None
        return stat.st_mtime_ns, stat.st_size

    def load(self) -> dict | None:
        try:
            with open(self.path, encoding="utf-8") as f:
                data = json.load(f)
        except (OSError, ValueError):
            return None
        return data if isinstance(data, dict) and data.get("refresh_token") else None

    def save(self, data: dict) -> None:
        write_private(self.path, json.dumps(data))

    def clear(self) -> None:
        try:
            os.remove(self.path)
        except OSError:
            pass


class Tokens:
    """The service's hold on the sign-in: a current access token, refreshed
    one at a time, going only to a server that proved itself. A file the
    plugin wrote since is read again."""

    def __init__(self, store: Store, user_agent: str, clock=time.time, random=os.urandom):
        self.store = store
        self.user_agent = user_agent
        self.clock = clock
        self.random = random
        self.lock = threading.Lock()
        self.data = None
        self.stamp = None
        # When, and on which network, the server last proved itself.
        self.proved_at = None

    def current(self) -> dict | None:
        stamp = self.store.stamp()
        if stamp != self.stamp:
            was = self.data
            self.data = self.store.load() if stamp else None
            self.stamp = stamp
            if (was or {}).get("sign_in") != (self.data or {}).get("sign_in"):
                # Another sign-in, another key: its server proves itself anew.
                self.proved_at = None
        return self.data

    def _proved(self, data: dict, now: bool = False) -> None:
        """Returns once the server at the sign-in's address has proved it is
        the one signed in to, within the half hour and on this network, or
        now (for a refresh token, a PIN, a sign-out): no token leaves
        before. A sign-in from a server without proofs has nothing to check.
        With the lock held."""
        keys = proof_of(data)
        if not keys:
            return
        if now:
            self.proved_at = None
        if self._recently_proved():
            return
        network = _network
        prove(data["server"], keys, self.user_agent, self.random)
        self.proved_at = (self.clock(), network)

    def _recently_proved(self) -> bool:
        if self.proved_at is None:
            return False
        at, network = self.proved_at
        # A clock set back makes an age below zero: proved again.
        return network == _network and 0 <= self.clock() - at < PROOF_TTL

    def doubt(self) -> None:
        """A request to the server got no answer: the next token waits for a
        new proof (Kodi may be on another network)."""
        self.proved_at = None

    def server(self) -> str | None:
        data = self.current()
        return data["server"] if data else None

    def identity(self):
        """Who the sign-in acts as: the server, the profile switched to
        (None: the one picked as it signed in), and which sign-in it is,
        since whoever approves a code picks the account and the file doesn't
        say whom. The event stream is opened again, and requests' news
        starts afresh, when it changes."""
        data = self.current()
        return (data["server"], data.get("profile"), data.get("sign_in")) if data else None

    def access(self) -> str:
        """An access token good for a while yet."""
        with self.lock:
            data = self.current()
            if not data:
                raise SignedOut()
            if data["expires_at"] - self.clock() > REFRESH_MARGIN:
                self._proved(data)
                return data["access_token"]
            # The refresh's own proof does for the new token.
            return self._refresh(data)["access_token"]

    def refreshed(self, stale: str) -> str:
        """A new access token after `stale` was refused, unless it was
        refreshed meanwhile."""
        with self.lock:
            data = self.current()
            if not data:
                raise SignedOut()
            if data["access_token"] != stale:
                # Refreshed meanwhile, maybe on another network since.
                self._proved(data)
                return data["access_token"]
            return self._refresh(data)["access_token"]

    def _refresh(self, data: dict) -> dict:
        # The refresh token goes only to a server that proves itself now.
        self._proved(data, now=True)
        client = Client(data["server"], self.user_agent)
        form = {"grant_type": "refresh_token", "refresh_token": data["refresh_token"], "client_id": CLIENT_ID}
        response = client.request("POST", path_of(data["server"], data["token_endpoint"]), form=form)
        if response.status == 200:
            return self._take(data, response)
        if response.status in (400, 401) and response.body.startswith(b"{"):
            if (response.json() or {}).get("error") in ("invalid_grant", "invalid_client", "unauthorized_client"):
                self.forget()
                raise SignedOut()
        raise ApiError(response.status, "refresh", "the server didn't renew the sign-in (%d)" % response.status)

    def _take(self, data: dict, response, **changed) -> dict:
        """The new tokens of a refresh or a switch, saved in place of
        `data`'s (with `changed`)."""
        try:
            tokens = response.json()
            renewed = dict(data)
            renewed.update(
                access_token=str(tokens["access_token"]),
                refresh_token=str(tokens["refresh_token"]),
                expires_at=self.clock() + int(tokens.get("expires_in") or 3600),
                scope=tokens.get("scope", data.get("scope")),
                **changed
            )
            # A sign-in keeps the key it was given whatever a later answer
            # brings; one from before keys gets its key now.
            if not proof_of(data) and proof_from(tokens):
                renewed["proof"] = proof_from(tokens)
        except (ValueError, KeyError, TypeError, AttributeError):
            raise ApiError(response.status, "refresh", "the server's answer to a refresh wasn't tokens")
        on_disk = self.store.load()
        if not on_disk or on_disk.get("refresh_token") != data["refresh_token"]:
            # Signed out, or in again, while this refresh was asked (the
            # plugin): what it got belongs to no sign-in any more.
            revoke(renewed, self.user_agent)
            self.data, self.stamp = None, None
            current = self.current()
            if not current:
                raise SignedOut()
            return current
        # Saved before it is used: the old refresh token is spent.
        self.store.save(renewed)
        self.data, self.stamp = renewed, self.store.stamp()
        return renewed

    def switch_profile(self, profile_id: int, pin=None) -> None:
        """Acts as another profile of the household from now on (the
        switch-profile grant), with its PIN when it has one; to the profile
        it acts as already, with its PIN, checks that PIN. A refusal spends
        nothing and raises ApiError, its code the server's reason
        ("wrong_pin", "throttled", "pin_locked", "browser", "not_allowed")
        or else its OAuth error ("invalid_request" without a reason: the
        server can't check the PIN of the profile in use)."""
        with self.lock:
            data = self.current()
            if not data:
                raise SignedOut()
            form = {"grant_type": SWITCH_GRANT, "refresh_token": data["refresh_token"], "client_id": CLIENT_ID, "profile": str(int(profile_id))}
            if pin:
                form["pin"] = pin
            # The refresh token and the PIN go only to a server that proves itself now.
            self._proved(data, now=True)
            client = Client(data["server"], self.user_agent)
            response = client.request("POST", path_of(data["server"], data["token_endpoint"]), form=form)
            if response.status == 200:
                self._take(data, response, profile=int(profile_id))
                return
            try:
                body = response.json() if response.body.startswith(b"{") else {}
            except ValueError:
                body = {}
            error, reason = body.get("error"), body.get("reason")
            if error == "invalid_grant" and not reason:
                # The refresh token doesn't work, as for a refresh.
                self.forget()
                raise SignedOut()
            message = body.get("error_description") or "the server refused (%d)" % response.status
            raise ApiError(response.status, reason or error or "http_%d" % response.status, message)

    def forget(self) -> None:
        self.store.clear()
        self.data, self.stamp = None, None
        self.proved_at = None

    def sign_out(self) -> None:
        """Ends the sign-in on the server (best effort), then here. A server
        that can't prove itself gets no token: the person can sign Kodi out
        in Account → Apps."""
        with self.lock:
            data = self.current()
            if data and proves(data, self.user_agent, self.random):
                revoke(data, self.user_agent)
            self.forget()


def revoke(record: dict, user_agent: str) -> bool:
    """Ends the grant of a sign-in on its server (RFC 7009): whether the
    server said so. Once more after a 503, which says to try again."""
    form = {"token": record["refresh_token"], "client_id": CLIENT_ID}
    path = path_of(record["server"], record["revocation_endpoint"])
    for attempt in range(2):
        try:
            response = Client(record["server"], user_agent, timeout=5).request("POST", path, form=form)
        except ApiError:
            return False
        if response.status != 503 or attempt:
            return response.status == 200
        time.sleep(2)
    return False
