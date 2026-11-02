' The tokens of each server, in the registry (roRegistrySection "tokens",
' one key per server). A refresh token works once, so a new pair is written
' and flushed before the new access token is used. Only the API task
' writes here. section is a roRegistrySection, or a stand-in with the same
' methods in tests.

function tokensKey(issuer as string) as string
    return "server:" + issuer
end function

' The tokens kept for issuer: { issuer, tokenEndpoint, revocationEndpoint,
' access, refresh, expiresAt, scope, and proof (authParseProof) from a
' server with them }, or invalid.
function tokensLoad(section as object, issuer as string) as dynamic
    key = tokensKey(issuer)
    if not section.Exists(key) then return invalid
    data = httpObject(httpJson(section.Read(key)))
    if data = invalid then return invalid
    tokens = {
        issuer: issuer,
        tokenEndpoint: httpString(data.token_endpoint),
        revocationEndpoint: httpString(data.revocation_endpoint),
        access: httpString(data.access),
        refresh: httpString(data.refresh),
        expiresAt: httpNumber(data.expires_at, 0),
        scope: httpString(data.scope)
    }
    if tokens.access = "" or tokens.tokenEndpoint = "" or tokens.revocationEndpoint = "" then return invalid
    proof = authParseProof(data)
    if proof <> invalid then tokens.proof = proof
    return tokens
end function

' Writes tokens and flushes the registry; false when either failed.
function tokensSave(section as object, tokens as object) as boolean
    kept = {
        token_endpoint: tokens.tokenEndpoint,
        revocation_endpoint: tokens.revocationEndpoint,
        access: tokens.access,
        refresh: tokens.refresh,
        expires_at: tokens.expiresAt,
        scope: tokens.scope
    }
    if tokens.proof <> invalid
        kept.server_id = tokens.proof.serverId
        kept.proof_id = tokens.proof.proofId
        kept.proof_key = tokens.proof.proofKey
    end if
    text = FormatJson(kept)
    if not section.Write(tokensKey(tokens.issuer), text) then return false
    return section.Flush()
end function

function tokensWipe(section as object, issuer as string) as boolean
    section.Delete(tokensKey(issuer))
    return section.Flush()
end function

' Tokens as a sign-in or refresh gave them (authParseTokens), kept for the
' endpoints they came from. A refresh or a profile switch is the same
' sign-in as before (tokens before it), whose key never changes: an answer
' brings one only to a sign-in without.
function tokensFrom(answer as object, issuer as string, tokenEndpoint as string, revocationEndpoint as string, before = invalid as dynamic) as object
    tokens = {
        issuer: issuer,
        tokenEndpoint: tokenEndpoint,
        revocationEndpoint: revocationEndpoint,
        access: answer.access,
        refresh: answer.refresh,
        expiresAt: answer.expiresAt,
        scope: answer.scope
    }
    proof = answer.proof
    if before <> invalid and before.proof <> invalid then proof = before.proof
    if proof <> invalid then tokens.proof = proof
    return tokens
end function
