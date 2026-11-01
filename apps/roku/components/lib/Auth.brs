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
    return { ok: true, access: access, refresh: httpString(data.refresh_token), expiresAt: now + Int(expiresIn), scope: scope }
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
' wrong_pin, throttled, pin_locked, browser, not_allowed,
' unauthorized_client; nothing spent); "ended" (the refresh token doesn't
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
            not_allowed: "You can't switch to this profile."
        }
        message = messages[reason]
        if reason = "throttled" and description <> "" then message = description
        if message = invalid then message = description
        return { kind: "refused", reason: reason, message: message }
    end if
    if description = "" then description = "The server refused: try again."
    return { kind: "failed", reason: error, message: description }
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
