' Answers as docs/API.md shows them.

function authTestMetadata() as string
    return "{""issuer"":""http://pley.local:7539"",""authorization_endpoint"":""http://pley.local:7539/oauth/authorize"",""token_endpoint"":""http://pley.local:7539/api/oauth/token"",""revocation_endpoint"":""http://pley.local:7539/api/oauth/revoke"",""device_authorization_endpoint"":""http://pley.local:7539/api/oauth/device"",""response_types_supported"":[""code""],""grant_types_supported"":[""authorization_code"",""refresh_token"",""urn:pley:params:oauth:grant-type:switch-profile"",""urn:ietf:params:oauth:grant-type:device_code""],""token_endpoint_auth_methods_supported"":[""none""],""code_challenge_methods_supported"":[""S256""],""scopes_supported"":[""library"",""play"",""playlists"",""profiles""]}"
end function

function authTestTokens() as string
    return "{""access_token"":""pley_at_abc"",""token_type"":""Bearer"",""expires_in"":3600,""scope"":""library play"",""refresh_token"":""pley_rt_def""}"
end function

function authTestDeviceCode() as string
    return "{""device_code"":""dc-256-bits"",""user_code"":""BCDF-GHJK"",""verification_uri"":""http://pley.local:7539/link"",""verification_uri_complete"":""http://pley.local:7539/link?code=BCDFGHJK"",""expires_in"":600,""interval"":5}"
end function

function authTestTokensAt(expiresAt as integer, refresh as string) as object
    return { issuer: "http://nas:7539", tokenEndpoint: "http://nas:7539/api/oauth/token", revocationEndpoint: "http://nas:7539/api/oauth/revoke", access: "pley_at_1", refresh: refresh, expiresAt: expiresAt, scope: "library play" }
end function

sub test_auth_reads_the_servers_own_metadata()
    meta = authParseMetadata(authTestMetadata(), "http://pley.local:7539")
    tkTrue(meta.ok)
    tkEqual(meta.issuer, "http://pley.local:7539")
    tkEqual(meta.tokenEndpoint, "http://pley.local:7539/api/oauth/token")
    tkEqual(meta.revocationEndpoint, "http://pley.local:7539/api/oauth/revoke")
    tkEqual(meta.deviceEndpoint, "http://pley.local:7539/api/oauth/device")
    tkEqual(meta.scopes, ["library", "play", "playlists", "profiles"])
end sub

' An older server doesn't know requests or approvals, and would refuse the
' whole sign-in for them.
sub test_auth_asks_for_requests_where_the_server_has_them()
    tkEqual(authScopeFor(["library", "play", "playlists", "profiles"]), "library play playlists profiles")
    tkEqual(authScopeFor(["library", "play", "playlists", "profiles", "requests", "approvals", "admin"]), "library play playlists profiles requests approvals")
    tkEqual(authScopeFor(["approvals", "requests"]), "library play playlists profiles requests approvals", "in the app's order")
    tkEqual(authScopeFor(["requests"]), "library play playlists profiles requests")
    tkEqual(authScopeFor([]), "library play playlists profiles")
    newer = authTestMetadata().Replace("""profiles""]", """profiles"",""requests"",""approvals""]")
    tkEqual(authScopeFor(authParseMetadata(newer, "http://pley.local:7539").scopes), "library play playlists profiles requests approvals")
    odd = authTestMetadata().Replace("""scopes_supported"":[""library"",""play"",""playlists"",""profiles""]", """scopes_supported"":[1,""requests"",null]")
    tkEqual(authParseMetadata(odd, "http://pley.local:7539").scopes, ["requests"])
end sub

sub test_auth_refuses_metadata_of_another_address()
    meta = authParseMetadata(authTestMetadata(), "http://192.168.1.20:7539")
    tkFalse(meta.ok)
    tkTrue(Instr(1, meta.message, "calls itself http://pley.local:7539") > 0, meta.message)

    elsewhere = authTestMetadata().Replace("""token_endpoint"":""http://pley.local:7539/", """token_endpoint"":""http://evil.example.com/")
    tkEqual(authParseMetadata(elsewhere, "http://pley.local:7539").message, "This server sends apps to another address to sign in: Pley doesn't.")
end sub

sub test_auth_tells_old_servers_apart()
    tkEqual(authParseMetadata("<!doctype html>", "http://nas:7539").message, authTooOld())
    tkEqual(authParseMetadata("", "http://nas:7539").message, authTooOld())
    noDevice = authTestMetadata().Replace("""device_authorization_endpoint"":""http://pley.local:7539/api/oauth/device"",", "")
    tkEqual(authParseMetadata(noDevice, "http://pley.local:7539").message, authNoCodes())
    noGrant = authTestMetadata().Replace(",""urn:ietf:params:oauth:grant-type:device_code""", "")
    tkEqual(authParseMetadata(noGrant, "http://pley.local:7539").message, authNoCodes())
end sub

sub test_auth_reads_token_answers()
    tokens = authParseTokens(authTestTokens(), "library play", 1000)
    tkTrue(tokens.ok)
    tkEqual(tokens.access, "pley_at_abc")
    tkEqual(tokens.refresh, "pley_rt_def")
    tkEqual(tokens.expiresAt, 4600)
    tkEqual(tokens.scope, "library play")

    plain = authParseTokens("{""access_token"":""a"",""token_type"":""bearer""}", "library play", 1000)
    tkTrue(plain.ok)
    tkEqual(plain.expiresAt, 4600, "an hour without expires_in")
    tkEqual(plain.scope, "library play", "what was asked without scope")
    tkEqual(plain.refresh, "")

    tkFalse(authParseTokens("{""access_token"":""a"",""token_type"":""mac""}", "", 0).ok)
    tkFalse(authParseTokens("{""access_token"":""a"",""token_type"":""Bearer"",""expires_in"":0}", "", 0).ok)
    tkFalse(authParseTokens("{""token_type"":""Bearer""}", "", 0).ok)
    tkFalse(authParseTokens("nope", "", 0).ok)
end sub

sub test_auth_spots_answers_with_more_than_asked()
    tkEqual(authExtraScopes("library play playlists", "library play"), ["playlists"])
    tkEqual(authExtraScopes("library", "library play"), [])
    tkEqual(authExtraScopes("library  play", "library play"), [])
end sub

sub test_auth_decides_before_a_request()
    now = 10000
    tkEqual(authBeforeRequest(authTestTokensAt(now + 3600, "r"), now, false), "send")
    tkEqual(authBeforeRequest(authTestTokensAt(now + 200, "r"), now, false), "refresh", "within the margin")
    tkEqual(authBeforeRequest(authTestTokensAt(now + 200, "r"), now, true), "wait")
    tkEqual(authBeforeRequest(invalid, now, false), "signed_out")
    tkEqual(authBeforeRequest(authTestTokensAt(now + 200, ""), now, false), "send", "nothing to refresh with")
    tkEqual(authBeforeRequest(authTestTokensAt(now - 1, ""), now, false), "signed_out")
end sub

sub test_auth_decides_after_a_refusal()
    now = 10000
    tokens = authTestTokensAt(now + 3600, "r")
    tkEqual(authAfterRefusal("pley_at_1", false, false, tokens, invalid, now, false), "answer", "not a token problem")
    tkEqual(authAfterRefusal("pley_at_1", true, false, tokens, { token: "pley_at_1", at: now - 5 }, now, false), "ended", "refused moments after a refresh gave it")
    tkEqual(authAfterRefusal("pley_at_1", true, false, tokens, { token: "pley_at_1", at: now - 120 }, now, false), "refresh", "given long ago")
    tkEqual(authAfterRefusal("pley_at_1", true, true, tokens, invalid, now, false), "answer", "once")
    tkEqual(authAfterRefusal("pley_at_0", true, false, tokens, invalid, now, false), "resend", "a newer token is there")
    tkEqual(authAfterRefusal("pley_at_1", true, false, tokens, invalid, now, true), "wait")
    tkEqual(authAfterRefusal("pley_at_1", true, false, authTestTokensAt(now + 3600, ""), invalid, now, false), "ended", "nothing to refresh with")
    tkEqual(authAfterRefusal("pley_at_1", true, false, invalid, invalid, now, false), "answer", "signed out meanwhile")
end sub

sub test_auth_reads_refresh_outcomes()
    tkEqual(authRefreshOutcome(200, authTestTokens()), "ok")
    tkEqual(authRefreshOutcome(400, "{""error"":""invalid_grant"",""error_description"":""refresh token used again: the app's access is ended""}"), "ended")
    tkEqual(authRefreshOutcome(400, "{""error"":""invalid_request""}"), "failed")
    tkEqual(authRefreshOutcome(503, "{""error"":""temporarily_unavailable""}"), "failed")
    tkEqual(authRefreshOutcome(-28, ""), "failed")
end sub

sub test_auth_reads_device_codes()
    code = authParseDeviceCode(authTestDeviceCode(), 1000)
    tkTrue(code.ok)
    tkEqual(code.deviceCode, "dc-256-bits")
    tkEqual(code.userCode, "BCDF-GHJK")
    tkEqual(code.verificationUri, "http://pley.local:7539/link")
    tkEqual(code.verificationUriComplete, "http://pley.local:7539/link?code=BCDFGHJK")
    tkEqual(code.expiresAt, 1600)
    tkEqual(code.interval, 5)

    bare = authParseDeviceCode("{""device_code"":""d"",""user_code"":""bcdfghjk"",""verification_uri"":""https://pley.example.com/link"",""expires_in"":600}", 0)
    tkTrue(bare.ok)
    tkEqual(bare.userCode, "BCDF-GHJK")
    tkEqual(bare.interval, 5, "RFC 8628's default")
    tkEqual(bare.verificationUriComplete, "")

    tkFalse(authParseDeviceCode("{""device_code"":""d"",""user_code"":""BCDFGHJK"",""verification_uri"":""javascript:alert(1)"",""expires_in"":600}", 0).ok)
    tkFalse(authParseDeviceCode("{""device_code"":""d"",""user_code"":""BCDFGHJK"",""verification_uri"":""http://nas/link"",""expires_in"":0}", 0).ok)
    tkFalse(authParseDeviceCode("{}", 0).ok)
end sub

sub test_auth_shows_user_codes_in_two_halves()
    tkEqual(authShownCode("BCDFGHJK"), "BCDF-GHJK")
    tkEqual(authShownCode("bcdf ghjk"), "BCDF-GHJK")
    tkEqual(authShownCode("ABC"), "ABC")
end sub

sub test_auth_keeps_a_refresh_only_while_its_sign_in_lasts()
    old = { issuer: "http://nas:7539", access: "a1", refresh: "r1" }
    tkTrue(authRefreshKept({ issuer: "http://nas:7539", access: "a1", refresh: "r1" }, old))
    tkFalse(authRefreshKept(invalid, old), "signed out or the server forgotten meanwhile")
    tkFalse(authRefreshKept({ issuer: "http://nas:7539", access: "a9", refresh: "r9" }, old), "signed in again with a code meanwhile")
end sub

sub test_auth_revokes_again_once_when_the_server_is_busy()
    tkTrue(authRevokeAgain(503, false))
    tkFalse(authRevokeAgain(503, true), "once")
    tkFalse(authRevokeAgain(200, false))
    tkFalse(authRevokeAgain(-28, false))
end sub

sub test_auth_asks_the_server_for_the_codes_qr_code()
    tkEqual(authQrAddress("http://192.168.1.20:7539", "BCDF-GHJK"), "http://192.168.1.20:7539/api/oauth/device/qr?code=BCDFGHJK")
    tkEqual(authQrAddress("https://pley.example.com", "bcdf ghjk"), "https://pley.example.com/api/oauth/device/qr?code=BCDFGHJK")
    tkEqual(authQrAddress("http://nas:7539", "-"), "", "no code")
end sub

sub test_auth_reads_poll_answers()
    tkEqual(authPollAnswer(400, "{""error"":""authorization_pending""}").kind, "pending")
    tkEqual(authPollAnswer(400, "{""error"":""slow_down""}").kind, "slow_down")
    tkEqual(authPollAnswer(400, "{""error"":""access_denied""}").kind, "denied")
    tkEqual(authPollAnswer(400, "{""error"":""expired_token""}").kind, "expired")
    invalidCode = authPollAnswer(400, "{""error"":""invalid_grant""}")
    tkEqual(invalidCode.kind, "invalid")
    tkEqual(invalidCode.message, "This code doesn't work any more: get a new one.")
    tkEqual(authPollAnswer(200, authTestTokens()).kind, "tokens")
    tkEqual(authPollAnswer(-7, "").kind, "network")
    tkEqual(authPollAnswer(503, "").kind, "network")
    odd = authPollAnswer(400, "{""error"":""unsupported_grant_type"",""error_description"":""that grant isn't offered""}")
    tkEqual(odd.kind, "failed")
    tkEqual(odd.message, "that grant isn't offered")
end sub

sub test_auth_polls_slower_after_network_failures()
    tkEqual(authPollWait(5000, 0), 5000)
    tkEqual(authPollWait(5000, 1), 5000, "never sooner than the interval")
    tkEqual(authPollWait(5000, 3), 8000)
    tkEqual(authPollWait(5000, 4), 16000)
    tkEqual(authPollWait(5000, 5), 30000)
    tkEqual(authPollWait(5000, 12), 30000)
    tkEqual(authPollWait(10000, 3), 10000)
end sub

sub test_auth_explains_device_refusals()
    tkEqual(authDeviceRefusal(400, "{""error"":""invalid_client""}"), "This server doesn't know Pley for Roku yet: update it.")
    tkEqual(authDeviceRefusal(400, "{""error"":""unauthorized_client""}"), "This server doesn't let Pley for Roku sign in with a code.")
    tkEqual(authDeviceRefusal(429, "{""error"":""temporarily_unavailable""}"), "Too many sign-ins are waiting on this server: try again in a few minutes.")
    tkEqual(authDeviceRefusal(-7, ""), "The server can't be reached.")
end sub

sub test_auth_says_when_the_server_requires_two_step_sign_in()
    note = "This server requires two-step sign-in: your phone sets it up if you haven't yet."
    tkEqual(authTwoStepNote(httpAnswer(200, "{""name"":""Den"",""version"":""1.0.0"",""commit"":""abc1234"",""setup_required"":false,""setup_code_required"":false,""two_step_required"":true}")), note)
    tkEqual(authTwoStepNote(httpAnswer(200, "{""name"":""Den"",""version"":""1.0.0"",""two_step_required"":false}")), "", "not required")
    tkEqual(authTwoStepNote(httpAnswer(200, "{""name"":""Den"",""version"":""0.9.0""}")), "", "a server from before it")
    tkEqual(authTwoStepNote(httpAnswer(200, "{""two_step_required"":""true""}")), "", "not a boolean")
    tkEqual(authTwoStepNote(httpAnswer(200, "{""two_step_required"":1}")), "")
    tkEqual(authTwoStepNote(httpAnswer(200, "{""two_step_required"":null}")), "")
    tkEqual(authTwoStepNote(httpAnswer(200, "[true]")), "")
    tkEqual(authTwoStepNote(httpAnswer(200, "<!doctype html>")), "", "not JSON")
    tkEqual(authTwoStepNote(httpAnswer(200, "")), "")
    tkEqual(authTwoStepNote(httpAnswer(503, "{""code"":""unavailable"",""two_step_required"":true}")), "", "a refusal")
    tkEqual(authTwoStepNote(httpAnswer(-28, "")), "", "no answer in time")
end sub

' --- Telling servers apart ---

function authTestKey() as string
    return "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE"
end function

function authTestProofKeys() as object
    return { serverId: "0123456789abcdef0123456789abcdef01234567", proofId: "17", proofKey: authTestKey() }
end function

sub test_auth_keeps_the_servers_proof_key_from_token_answers()
    keyed = authTestTokens().Replace("}", ",""server_id"":""0123456789abcdef0123456789abcdef01234567"",""proof_id"":""17"",""proof_key"":""" + authTestKey() + """}")
    tkEqual(authParseTokens(keyed, "library play", 1000).proof, authTestProofKeys())
    tkInvalid(authParseTokens(authTestTokens(), "library play", 1000).proof, "a server before them")
    base = { server_id: "0123456789abcdef0123456789abcdef01234567", proof_id: "17", proof_key: authTestKey() }
    tkEqual(authParseProof(base), authTestProofKeys())
    for each bad in [{ proof_id: "017" }, { proof_id: "0" }, { proof_id: 17 }, { proof_id: "12345678901234567890" }, { proof_id: "17" + Chr(10) }, { proof_key: authTestKey() + "=" }, { proof_key: "AQEBAQEBAQEBAQEBAQEBAQ" }, { proof_key: authTestKey().Replace("A", "+") }, { server_id: "" }]
        data = {}
        data.Append(base)
        data.Append(bad)
        tkInvalid(authParseProof(data), FormatJson(bad))
    end for
end sub

' The server's known answers (apps/server/src/identity.rs, and the shared
' fixture packages/tv/test/fixtures/auth.json).
sub test_auth_a_proof_is_the_servers_hmac()
    key = authKeyBytes(authTestKey())
    id = "0123456789abcdef0123456789abcdef01234567"
    tkEqual(authProof(key, id, "http://192.168.1.20:7539", "AAECAwQFBgcICQoLDA0ODw"), "YwkvaCQfaIQCZkoOP5yOE3yR4xRKU5T5wYWz4b85SCM")
    tkEqual(authProof(key, id, "https://pley.example.com", "AAECAwQFBgcICQoLDA0OEA"), "zHpBDBWP9YET2oKU1zMoY50w77pLxVJDXPKyJZVOHuA")
    tkEqual(authProof(key, id, "http://[fd00::5]:7539", "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8"), "Hy76L1n79Vkst4fC8d1JqvXMZ7rY3MhMKArRSR57AQ4")
end sub

sub test_auth_only_the_server_proves_itself_at_its_address()
    keys = authTestProofKeys()
    at = "http://192.168.1.20:7539"
    challenge = "AAECAwQFBgcICQoLDA0ODw"
    right = "YwkvaCQfaIQCZkoOP5yOE3yR4xRKU5T5wYWz4b85SCM"
    answer = "{""id"":""0123456789abcdef0123456789abcdef01234567"",""name"":""Den"",""proof"":""" + right + """}"
    tkEqual(authProofVerdict(200, answer, keys, at, challenge), "proved")
    ' The same answer at another address (a redirect followed elsewhere).
    tkEqual(authProofVerdict(200, answer, keys, "https://pley.example.com", challenge), "another")
    tkEqual(authProofVerdict(200, answer, keys, at, "AAECAwQFBgcICQoLDA0OEA"), "another", "another challenge")
    tkEqual(authProofVerdict(200, answer.Replace("0123456789abcdef0123456789abcdef01234567", "another"), keys, at, challenge), "another")
    tkEqual(authProofVerdict(200, "{""id"":""0123456789abcdef0123456789abcdef01234567"",""proof"":null}", keys, at, challenge), "another")
    tkEqual(authProofVerdict(200, "<html>a router</html>", keys, at, challenge), "another")
    tkEqual(authProofVerdict(403, "{""message"":""a proof for a plain-http address is made for the home network only""}", keys, at, challenge), "another")
    tkEqual(authProofVerdict(404, "", keys, at, challenge), "another")
    ' The server, or a proxy before it, out of reach for now.
    tkEqual(authProofVerdict(502, "Bad Gateway", keys, at, challenge), "unreachable")
    tkEqual(authProofVerdict(-28, "", keys, at, challenge), "unreachable")
end sub

sub test_auth_a_proof_counts_half_an_hour_on_its_network()
    proved = { at: 1000, network: 2 }
    tkTrue(authRecentlyProved(proved, 2, 1000))
    tkTrue(authRecentlyProved(proved, 2, 1000 + authProofTtlMs() - 1))
    tkFalse(authRecentlyProved(proved, 2, 1000 + authProofTtlMs()))
    tkFalse(authRecentlyProved(proved, 3, 1000), "another network")
    tkFalse(authRecentlyProved(proved, 2, 999), "a clock gone back")
    tkFalse(authRecentlyProved(invalid, 2, 1000))
    tkTrue(authProofRenewMs() < authProofTtlMs(), "renewed before it lapses")
end sub

sub test_auth_no_token_goes_after_a_request_went_unanswered()
    keys = { serverId: "abc", proofId: "17", proofKey: "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE" }
    proved = { at: 1000, network: 2 }
    tkTrue(authTokenMayGo(keys, proved, 2, false, 1000))
    tkFalse(authTokenMayGo(keys, proved, 2, true, 1000), "doubted: the players' token waits for a proof")
    tkFalse(authTokenMayGo(keys, proved, 3, false, 1000), "another network")
    tkTrue(authTokenMayGo(invalid, invalid, 2, true, 1000), "a server without proofs: nothing to check")
end sub

sub test_auth_the_next_proof_is_asked_when_due_and_not_before_a_retry()
    proved = { at: 1000, network: 2 }
    tkFalse(authProofDue(proved, 2, false, 1000 + authProofRenewMs() - 1, 0))
    tkTrue(authProofDue(proved, 2, false, 1000 + authProofRenewMs(), 0), "before it lapses")
    tkTrue(authProofDue(proved, 3, false, 1000, 0), "another network")
    tkTrue(authProofDue(proved, 2, true, 1000, 0), "a request went unanswered")
    tkTrue(authProofDue(invalid, 2, false, 1000, 0), "none held")
    ' A proof refused or unanswered isn't asked again at once, so another
    ' device at the address isn't asked over and over.
    tkFalse(authProofDue(invalid, 2, false, 1000, 31000))
    tkTrue(authProofDue(invalid, 2, false, 31000, 31000))
end sub
