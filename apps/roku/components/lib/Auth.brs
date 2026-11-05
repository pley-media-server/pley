' Signing in with a code shown on the TV (RFC 8628) and keeping the tokens
' it gives (docs/API.md, "Apps (OAuth 2.1)"). These are the decisions only:
' the API task asks the server and saves, one request at a time for tokens.

function authDeviceGrant() as string
    return "urn:ietf:params:oauth:grant-type:device_code"
end function

' Seconds before an access token's end at which it is refreshed: playback
' carries the token in its HTTP agent, so it is renewed well ahead.
function authMarginSeconds() as integer
    return 300
end function

function authTooOld() as string
    return "This server is too old for this app: update it."
end function

function authNoCodes() as string
    return "This server can't sign in with a code yet: update it."
end function

' The server's metadata (RFC 8414), kept only when it is this server's own:
' its issuer is the address the app used, and every endpoint is on it, so
' an answer can't send the app's codes or tokens anywhere else.
' { ok, issuer, tokenEndpoint, revocationEndpoint, deviceEndpoint, scopes
' (scopes_supported) } or { ok: false, message }.
function authParseMetadata(text as dynamic, server as string) as object
    data = invalid
    data = httpObject(httpJson(text))
    if data = invalid then return { ok: false, message: authTooOld() }
    issuer = httpString(data.issuer)
    if issuer = "" then return { ok: false, message: authTooOld() }
    if issuer <> server
        return { ok: false, message: "This server calls itself " + httpSentence(Left(issuer, 100)) + ": open it by that address, or check that the proxy in front of it passes Host." }
    end if
    scopes = []
    for each name in httpArray(data.scopes_supported)
        if GetInterface(name, "ifString") <> invalid then scopes.Push(name)
    end for
    result = { ok: true, issuer: issuer, tokenEndpoint: "", revocationEndpoint: "", deviceEndpoint: "", scopes: scopes }
    names = { tokenEndpoint: "token_endpoint", revocationEndpoint: "revocation_endpoint", deviceEndpoint: "device_authorization_endpoint" }
    for each key in names.Keys()
        url = httpString(data[names[key]])
        if url = "" then return { ok: false, message: authNoCodes() }
        if urlOrigin(url) <> issuer or Left(url, Len(issuer) + 1) <> issuer + "/"
            return { ok: false, message: "This server sends apps to another address to sign in: Pley doesn't." }
        end if
        result[key] = url
    end for
    grants = httpArray(data.grant_types_supported)
    deviceGrant = false
    for each grant in grants
        if grant = authDeviceGrant() then deviceGrant = true
    end for
    if not deviceGrant then return { ok: false, message: authNoCodes() }
    return result
end function

' What the app asks for at sign-in: its own scopes (httpScope), then
' asking for movies and shows (requests) and approving them (approvals)
' where the server's metadata offers them: a server that doesn't know a
' scope refuses the whole sign-in.
function authScopeFor(supported as object) as string
    offered = {}
    for each name in supported
        offered[name] = true
    end for
    scope = httpScope()
    for each name in ["requests", "approvals"]
        if offered.DoesExist(name) then scope = scope + " " + name
    end for
    return scope
end function

' A token answer (RFC 6749 §5.1) as kept: { ok, access, refresh, expiresAt,
' scope }, expiresAt in seconds since the epoch. Without a scope the server
' issued what was asked; without expires_in, pley's hour.
function authParseTokens(text as dynamic, askedScope as string, now as integer) as object
    bad = { ok: false, message: "The server's answer can't be read: try again." }
    data = invalid
    data = httpObject(httpJson(text))
    if data = invalid then return bad
    access = httpString(data.access_token)
    if access = "" then return bad
    if LCase(httpString(data.token_type)) <> "bearer" then return bad
    expiresIn = 3600
    if data.DoesExist("expires_in") then expiresIn = httpNumber(data.expires_in, -1)
    if expiresIn <= 0 then return bad
    scope = httpString(data.scope)
    if scope = "" then scope = askedScope
    return { ok: true, access: access, refresh: httpString(data.refresh_token), expiresAt: now + Int(expiresIn), scope: scope, proof: authParseProof(data) }
end function

' --- Telling servers apart (docs/API.md) ---

' How long a server's proof counts for the access token, unless a request
' gets no answer or the network changes meanwhile; and when the API task
' asks for the next one, so the token the players hold never lapses.
function authProofTtlMs() as integer
    return 1800000
end function

function authProofRenewMs() as integer
    return 1500000
end function

function authAnotherDevice() as string
    return "Another device answers there."
end function

' The server's id and the sign-in's proof key in a token answer (or kept
' tokens): { serverId, proofId, proofKey } when they are written as they
' should be, else invalid (a server before them).
function authParseProof(data as dynamic) as dynamic
    data = httpObject(data)
    if data = invalid then return invalid
    serverId = httpString(data.server_id)
    proofId = httpString(data.proof_id)
    proofKey = httpString(data.proof_key)
    if serverId = "" or not authProofIdOk(proofId) or authKeyBytes(proofKey) = invalid then return invalid
    return { serverId: serverId, proofId: proofId, proofKey: proofKey }
end function

' A grant's id: 1 to 19 digits, the first not 0.
function authProofIdOk(id as string) as boolean
    if Len(id) < 1 or Len(id) > 19 then return false
    for i = 1 to Len(id)
        c = Mid(id, i, 1)
        if c < "0" or c > "9" then return false
        if i = 1 and c = "0" then return false
    end for
    return true
end function

' The 32 bytes of a proof key written in base64url without padding; invalid
' for anything else.
function authKeyBytes(key as string) as dynamic
    if Len(key) Mod 4 = 1 then return invalid
    standard = ""
    for i = 1 to Len(key)
        c = Mid(key, i, 1)
        if c = "-"
            c = "+"
        else if c = "_"
            c = "/"
        else if not ((c >= "A" and c <= "Z") or (c >= "a" and c <= "z") or (c >= "0" and c <= "9"))
            return invalid
        end if
        standard = standard + c
    end for
    while Len(standard) Mod 4 <> 0
        standard = standard + "="
    end while
    bytes = CreateObject("roByteArray")
    bytes.FromBase64String(standard)
    if bytes.Count() <> 32 then return invalid
    return bytes
end function

' bytes (a roByteArray) in base64url without padding.
function authBase64Url(bytes as object) as string
    text = bytes.ToBase64String()
    out = ""
    for i = 1 to Len(text)
        c = Mid(text, i, 1)
        if c = "+"
            out = out + "-"
        else if c = "/"
            out = out + "_"
        else if c <> "="
            out = out + c
        end if
    end for
    return out
end function

' The proof of serverId, asked at address, for challenge under key (a
' roByteArray): HMAC-SHA256 of "pley-server-proof", the server's id, the
' address and the challenge, each on a line of its own, in base64url.
function authProof(key as object, serverId as string, address as string, challenge as string) as string
    message = CreateObject("roByteArray")
    message.FromAsciiString("pley-server-proof" + Chr(10) + serverId + Chr(10) + address + Chr(10) + challenge)
    hmac = CreateObject("roHMAC")
    if hmac.Setup("sha256", key) <> 0 then return ""
    return authBase64Url(hmac.Process(message))
end function

' What the server at address answered a proof's challenge, for the keys
' kept: "proved"; "unreachable" (nothing answered, or the server or a proxy
' before it failed: try again later); "another" (another device answers at
' the address: it gets no token). The proof covers the address asked, so
' an answer from elsewhere (a redirect followed to the real server's other
' address) proves nothing.
function authProofVerdict(status as integer, text as dynamic, keys as object, address as string, challenge as string) as string
    if status <= 0 or status >= 500 then return "unreachable"
    if status <> 200 then return "another"
    data = httpObject(httpJson(text))
    if data = invalid then return "another"
    given = data.proof
    if httpString(data.id) <> keys.serverId or GetInterface(given, "ifString") = invalid then return "another"
    key = authKeyBytes(keys.proofKey)
    if key = invalid then return "another"
    if given <> authProof(key, keys.serverId, address, challenge) then return "another"
    return "proved"
end function

' Whether a proof made at provedAt (the task's clock, ms; invalid for none)
' on the network counted then still lets the access token go now.
function authRecentlyProved(proved as dynamic, network as integer, now as integer) as boolean
    if proved = invalid then return false
    if proved.network <> network then return false
    age = now - proved.at
    return age >= 0 and age < authProofTtlMs()
end function

' Whether the access token may go to the server, for the keys kept (invalid:
' a server without proofs, nothing to check): it proved itself lately, and
' no request to it went unanswered since (doubted: the Roku may be on
' another network).
function authTokenMayGo(keys as dynamic, proved as dynamic, network as integer, doubted as boolean, now as integer) as boolean
    if keys = invalid then return true
    return not doubted and authRecentlyProved(proved, network, now)
end function

' Whether the API task asks for the next proof now: none is held, a request
' went unanswered since, the network changed, or the one held is about to
' lapse. One that failed is asked again at retryAt, not before.
function authProofDue(proved as dynamic, network as integer, doubted as boolean, now as integer, retryAt as integer) as boolean
    if now < retryAt then return false
    if proved = invalid or doubted then return true
    return proved.network <> network or now - proved.at >= authProofRenewMs()
end function

' The scope names in answer that asked doesn't hold. pley never widens a
' grant, so an answer with more is ended at once and never used.
function authExtraScopes(answer as string, asked as string) as object
    allowed = {}
    for each name in asked.Split(" ")
        if name <> "" then allowed[name] = true
    end for
    extra = []
    for each name in answer.Split(" ")
        if name <> "" and not allowed.DoesExist(name) then extra.Push(name)
    end for
    return extra
end function

' Whether the access token still has the margin to be sent.
function authTokenUsable(tokens as dynamic, now as integer) as boolean
    if tokens = invalid then return false
    if tokens.access = "" then return false
    return tokens.expiresAt - now > authMarginSeconds()
end function

' Before a request that needs the token: "send" it, "refresh" first (none
' under way), "wait" for the refresh under way, or "signed_out".
function authBeforeRequest(tokens as dynamic, now as integer, refreshing as boolean) as string
    if tokens = invalid then return "signed_out"
    if authTokenUsable(tokens, now) then return "send"
    if refreshing then return "wait"
    if tokens.refresh = ""
        ' Nothing to refresh with: the token works to its last second.
        if tokens.expiresAt > now then return "send"
        return "signed_out"
    end if
    return "refresh"
end function

' A 401 answer to a request that sent the token sent:
' "answer": the refusal is the answer (not the token's fault, or tried
' again already); "ended": the token a refresh gave moments ago is refused
' too, so the server no longer accepts this sign-in; "resend": a newer
' token is there already; "wait": a refresh is under way; "refresh": get a
' new token, then send again. lastIssued is { token, at } of the last
' refresh, or invalid.
function authAfterRefusal(sent as string, invalidToken as boolean, retried as boolean, tokens as dynamic, lastIssued as dynamic, now as integer, refreshing as boolean) as string
    if not invalidToken or sent = "" or tokens = invalid then return "answer"
    if lastIssued <> invalid
        if lastIssued.token = sent and tokens.access = sent and now - lastIssued.at <= 60 then return "ended"
    end if
    if retried then return "answer"
    if tokens.access <> sent and authTokenUsable(tokens, now) then return "resend"
    if refreshing then return "wait"
    if tokens.refresh = "" then return "ended"
    return "refresh"
end function

' What the answer to a refresh means: "ok"; "ended" (the server ended the
' sign-in: a spent, unknown or expired refresh token, a disabled account);
' "failed" (the network or the server: try again later, the tokens stay).
function authRefreshOutcome(status as integer, text as dynamic) as string
    if status >= 200 and status < 300 then return "ok"
    if status = 400 or status = 401
        data = invalid
        data = httpObject(httpJson(text))
        if data <> invalid
            error = httpString(data.error)
            if error = "invalid_grant" or error = "invalid_client" or error = "unauthorized_client" then return "ended"
        end if
    end if
    return "failed"
end function

' --- Switching profiles ---

function authSwitchGrant() as string
    return "urn:pley:params:oauth:grant-type:switch-profile"
end function

' Whether a scope ("library play playlists profiles") allows name.
function authAllows(scope as string, name as string) as boolean
    for each part in scope.Split(" ")
        if part = name then return true
    end for
    return false
end function

' What the token endpoint's answer to a profile switch (or to a PIN
' checked for the profile signed in) means (docs/API.md, the switch-profile
' grant): { kind, reason, message }. kind: "tokens"; "through" (no PIN to
' check: the profile has none, or the server can't); "refused" (reason:
' wrong_pin, throttled, pin_locked, browser, not_allowed, ticket_spent
' (a cast's ticket), unauthorized_client; nothing spent); "ended" (the refresh token doesn't
' work: the sign-in is over); "failed" (the network or the server).
function authSwitchOutcome(status as integer, text as dynamic) as object
    if status >= 200 and status < 300 then return { kind: "tokens", reason: "", message: "" }
    error = ""
    reason = ""
    description = ""
    data = invalid
    data = httpObject(httpJson(text))
    if data <> invalid
        error = httpString(data.error)
        reason = httpString(data.reason)
        description = httpSentence(data.error_description)
    end if
    if status <= 0 or status = 502 or status = 504 then return { kind: "failed", reason: "network", message: "The server can't be reached." }
    if error = "invalid_request" and reason = "" then return { kind: "through", reason: "", message: "" }
    if error = "unauthorized_client" then return { kind: "refused", reason: error, message: "This sign-in can't switch profiles." }
    if error = "invalid_grant"
        if reason = "" then return { kind: "ended", reason: "", message: "" }
        messages = {
            wrong_pin: "Wrong PIN.",
            throttled: "Too many wrong PINs: try again in a few minutes.",
            pin_locked: "This profile's PIN is locked after too many wrong tries. Its owner or an administrator can reset it.",
            browser: "This profile needs its password: sign in to it with a code, choosing it in the browser.",
            not_allowed: "You can't switch to this profile.",
            ticket_spent: "This cast's switch expired: cast again."
        }
        message = messages[reason]
        if reason = "throttled" and description <> "" then message = description
        if message = invalid then message = description
        return { kind: "refused", reason: reason, message: message }
    end if
    if description = "" then description = "The server refused: try again."
    return { kind: "failed", reason: error, message: description }
end function

' How long a cast's switch waits for what the profile before it sent (the
' playback it ended: its last progress, its stopped report and its
' session's end) to go out with that profile's tokens.
function authSwitchHoldMs() as integer
    return 5000
end function

' What a switch held for the profile's requests does now (jobs: the API
' task's in flight, waiting for a refresh or a proof, or to go again
' later; switches: the profile switches made so far; heldAt and now in
' ms): "wait" while a request with the token, made for the profile signed
' in, hasn't been answered; "go" once none is left; "drop" once
' authSwitchHoldMs has passed with some left, which are dropped before
' the switch goes.
function authSwitchHeld(jobs as object, switches as integer, heldAt as integer, now as integer) as string
    remaining = 0
    for each job in jobs
        if httpTrue(job.auth) and httpNumber(job.gen, -1) = switches then remaining = remaining + 1
    end for
    if remaining = 0 then return "go"
    if now - heldAt >= authSwitchHoldMs() then return "drop"
    return "wait"
end function

' Whether a request may go with the tokens in use (switches: the profile
' switches made so far). One with the token made for the profile before a
' switch (gen: the switches made when it was asked), waiting for a refresh
' or a proof, or refused while the switch was under way, never goes with
' the new profile's tokens: it is dropped.
function authJobGoes(job as object, switches as integer) as boolean
    if not httpTrue(job.auth) then return true
    return httpNumber(job.gen, -1) = switches
end function

' Whether the tokens a refresh of old gave are kept: only while the
' registry (stored, its tokens for old's server) still holds old's refresh
' token. Signing out, forgetting the server or signing in again with a code
' while the refresh was under way replaced or removed it: its tokens are
' ended instead, and whatever is stored stays.
function authRefreshKept(stored as dynamic, old as object) as boolean
    if stored = invalid then return false
    return stored.refresh <> "" and stored.refresh = old.refresh
end function

' Whether a revocation is sent again (once, a few seconds later): the
' server was busy (503), so the token may still work.
function authRevokeAgain(status as integer, retried as boolean) as boolean
    return status = 503 and not retried
end function

' The answer of the device authorization endpoint (RFC 8628 §3.2):
' { ok, deviceCode, userCode, verificationUri, verificationUriComplete,
' expiresAt, interval } (interval in seconds), or { ok: false, message }.
function authParseDeviceCode(text as dynamic, now as integer) as object
    bad = { ok: false, message: "The server's answer can't be read: try again." }
    data = invalid
    data = httpObject(httpJson(text))
    if data = invalid then return bad
    deviceCode = httpString(data.device_code)
    userCode = authShownCode(httpString(data.user_code))
    verificationUri = authLink(data.verification_uri)
    if deviceCode = "" or userCode = "" or verificationUri = "" then return bad
    expiresIn = httpNumber(data.expires_in, -1)
    if expiresIn <= 0 then return bad
    interval = Int(httpNumber(data.interval, 5))
    if interval <= 0 then interval = 5
    return {
        ok: true,
        deviceCode: deviceCode,
        userCode: userCode,
        verificationUri: verificationUri,
        verificationUriComplete: authLink(data.verification_uri_complete),
        expiresAt: now + Int(expiresIn),
        interval: interval
    }
end function

' A user code as people read it: 8 letters as "BCDF-GHJK", anything else as
' sent.
function authShownCode(code as string) as string
    letters = authCodeLetters(code)
    if Len(letters) = 8 then return Left(letters, 4) + "-" + Mid(letters, 5)
    return httpSentence(code)
end function

' A user code's letters and digits, in capitals: "BCDFGHJK".
function authCodeLetters(code as string) as string
    letters = ""
    upper = UCase(code)
    for i = 1 to Len(upper)
        c = Mid(upper, i, 1)
        if (c >= "A" and c <= "Z") or (c >= "0" and c <= "9") then letters = letters + c
    end for
    return letters
end function

' The server's picture of where to type the code, as a QR code
' (GET /api/oauth/device/qr): the same address as
' verification_uri_complete. Anyone may fetch it. "" without a code.
function authQrAddress(server as string, userCode as string) as string
    letters = authCodeLetters(userCode)
    if letters = "" then return ""
    return server + urlWithQuery("/api/oauth/device/qr", { code: letters })
end function

' value when it is an http(s) address to show as it is, else "".
function authLink(value as dynamic) as string
    text = httpString(value)
    if text = "" or httpSentence(text) <> text then return ""
    if urlOrigin(text) = invalid then return ""
    return text
end function

' The answer to one poll of the token endpoint with the device code:
' { kind, message }, kind one of "tokens", "pending", "slow_down",
' "denied", "expired", "invalid", "network" (keep trying, slower) or
' "failed".
function authPollAnswer(status as integer, text as dynamic) as object
    if status <= 0 or status >= 500 then return { kind: "network", message: "" }
    if status >= 200 and status < 300 then return { kind: "tokens", message: "" }
    data = invalid
    data = httpObject(httpJson(text))
    error = ""
    description = ""
    if data <> invalid
        error = httpString(data.error)
        description = httpSentence(data.error_description)
    end if
    if error = "authorization_pending" then return { kind: "pending", message: "" }
    if error = "slow_down" then return { kind: "slow_down", message: "" }
    if error = "access_denied" then return { kind: "denied", message: "" }
    if error = "expired_token" then return { kind: "expired", message: "" }
    if error = "invalid_grant" then return { kind: "invalid", message: "This code doesn't work any more: get a new one." }
    if description = "" then description = httpStatusSentence(status)
    return { kind: "failed", message: description }
end function

' Milliseconds to wait before the next poll: the interval, or after network
' failures in a row 2, 4, 8, 16, then 30 seconds, never less than the
' interval.
function authPollWait(intervalMs as integer, failures as integer) as integer
    if failures <= 0 then return intervalMs
    backoff = [2000, 4000, 8000, 16000, 30000]
    index = failures - 1
    if index >= backoff.Count() then index = backoff.Count() - 1
    if backoff[index] > intervalMs then return backoff[index]
    return intervalMs
end function

' What the device authorization endpoint's refusals mean for people.
function authDeviceRefusal(status as integer, text as dynamic) as string
    data = invalid
    data = httpObject(httpJson(text))
    error = ""
    if data <> invalid then error = httpString(data.error)
    if error = "invalid_client" then return "This server doesn't know Pley for Roku yet: update it."
    if error = "unauthorized_client" then return "This server doesn't let Pley for Roku sign in with a code."
    if error = "invalid_scope" then return "This server doesn't offer what Pley for Roku asks for: update it."
    if status = 429 then return "Too many sign-ins are waiting on this server: try again in a few minutes."
    if status = 404 then return authNoCodes()
    if status <= 0 then return "The server can't be reached."
    if data <> invalid
        description = httpSentence(data.error_description)
        if description <> "" then return description
    end if
    return httpStatusSentence(status)
end function

' What the sign-in screen says of two-step sign-in, from the answer to
' GET /api/server: a server that requires it of every account with a
' password says so, and the phone that approves the code sets it up for
' whoever hasn't. "" for any other answer, as from servers before it,
' which don't say.
function authTwoStepNote(answer as object) as string
    data = invalid
    if httpTrue(answer.ok) then data = httpObject(answer.data)
    if data = invalid or not httpTrue(data.two_step_required) then return ""
    return "This server requires two-step sign-in: your phone sets it up if you haven't yet."
end function
