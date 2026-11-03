function tokensTestPair(access as string, refresh as string) as object
    return { issuer: "http://nas:7539", tokenEndpoint: "http://nas:7539/api/oauth/token", revocationEndpoint: "http://nas:7539/api/oauth/revoke", access: access, refresh: refresh, expiresAt: 1729603600, scope: "library play" }
end function

sub test_tokens_are_saved_flushed_and_read_back()
    section = tkRegistrySection()
    tkTrue(tokensSave(section, tokensTestPair("pley_at_1", "pley_rt_1")))
    tkEqual(section.flushes, 1, "flushed before use")
    tkEqual(tokensLoad(section, "http://nas:7539"), tokensTestPair("pley_at_1", "pley_rt_1"))
    tkInvalid(tokensLoad(section, "http://other:7539"), "another server's")
end sub

sub test_tokens_of_each_server_stay_apart()
    section = tkRegistrySection()
    other = tokensTestPair("pley_at_9", "pley_rt_9")
    other.issuer = "https://pley.example.com"
    tokensSave(section, tokensTestPair("pley_at_1", "pley_rt_1"))
    tokensSave(section, other)
    tkEqual(tokensLoad(section, "https://pley.example.com").access, "pley_at_9")
    tkTrue(tokensWipe(section, "http://nas:7539"))
    tkInvalid(tokensLoad(section, "http://nas:7539"))
    tkEqual(tokensLoad(section, "https://pley.example.com").refresh, "pley_rt_9")
end sub

sub test_tokens_a_new_pair_replaces_the_old()
    section = tkRegistrySection()
    tokensSave(section, tokensTestPair("pley_at_1", "pley_rt_1"))
    tokensSave(section, tokensTestPair("pley_at_2", "pley_rt_2"))
    tkEqual(tokensLoad(section, "http://nas:7539").refresh, "pley_rt_2")
    tkEqual(section.GetKeyList().Count(), 1)
end sub

sub test_tokens_unreadable_entries_are_none()
    section = tkRegistrySection()
    section.Write(tokensKey("http://nas:7539"), "not json")
    tkInvalid(tokensLoad(section, "http://nas:7539"))
    section.Write(tokensKey("http://nas:7539"), "{""access"":""""}")
    tkInvalid(tokensLoad(section, "http://nas:7539"))
end sub

sub test_tokens_from_an_answer_keep_their_endpoints()
    answer = authParseTokens(authTestTokens(), "library play", 100)
    tokens = tokensFrom(answer, "http://pley.local:7539", "http://pley.local:7539/api/oauth/token", "http://pley.local:7539/api/oauth/revoke")
    tkEqual(tokens.issuer, "http://pley.local:7539")
    tkEqual(tokens.access, "pley_at_abc")
    tkEqual(tokens.refresh, "pley_rt_def")
    tkEqual(tokens.expiresAt, 3700)
    tkEqual(tokens.revocationEndpoint, "http://pley.local:7539/api/oauth/revoke")
end sub

sub test_tokens_keep_their_servers_proof_key()
    section = tkRegistrySection()
    keys = { serverId: "0123456789abcdef0123456789abcdef01234567", proofId: "17", proofKey: "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE" }
    pair = tokensTestPair("pley_at_1", "pley_rt_1")
    pair.proof = keys
    tokensSave(section, pair)
    tkEqual(tokensLoad(section, "http://nas:7539").proof, keys)
    ' A refresh keeps the sign-in's key whatever its answer brings; one from
    ' before keys gets its key.
    other = { ok: true, access: "pley_at_2", refresh: "pley_rt_2", expiresAt: 1, scope: "library", proof: { serverId: "x", proofId: "18", proofKey: "CQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQk" } }
    tkEqual(tokensFrom(other, "http://nas:7539", "t", "r", pair).proof, keys)
    tkEqual(tokensFrom(other, "http://nas:7539", "t", "r", tokensTestPair("pley_at_1", "pley_rt_1")).proof, other.proof)
    tkInvalid(tokensFrom({ ok: true, access: "a", refresh: "r", expiresAt: 1, scope: "library" }, "http://nas:7539", "t", "r").proof)
end sub
