import Foundation
import PleyModel
import PleyTesting
import Testing

private func decode<T: Decodable>(_ type: T.Type, fixture path: String) throws -> T {
    try PleyJSON.decoder.decode(T.self, from: Fixtures.data(path))
}

private func decode<T: Decodable>(_ type: T.Type, json: String) throws -> T {
    try PleyJSON.decoder.decode(T.self, from: Data(json.utf8))
}

@Suite("The server's answers decode as the Android app reads them")
struct FixtureTests {
    @Test func homeHubs() throws {
        let home = try decode(HomeHubs.self, fixture: "hubs/home.json")
        #expect(home.hubs.map(\.key) == ["continue_watching", "recently_added:1", "recently_added:2", "recently_added:3"])
        let continueWatching = home.hubs[0]
        #expect(continueWatching.libraryId == nil)
        #expect(continueWatching.title == "Continue Watching")
        let episode = continueWatching.items[0]
        #expect(episode.kind == .episode)
        #expect(episode.grandparentTitle == "Severance")
        #expect(episode.parentIndex == 1)
        #expect(episode.index == 2)
        #expect(episode.posterUrl == "/api/items/202/images/poster?v=1761200400")
        #expect(episode.userState.positionMs == 1_185_000)
        #expect(episode.userState.lastPlayedAt == 1_761_390_000)
        #expect(continueWatching.items[1].userState.rating == 9.0)
        #expect(home.hubs[1].libraryId == 1)
        let album = home.hubs[3].items[0]
        #expect(album.kind == .album)
        #expect(album.leafCount == 12)
        #expect(album.durationMs == nil)
    }

    @Test func householdProfiles() throws {
        let profiles = try decode([HouseholdProfile].self, fixture: "profiles/household.json")
        #expect(profiles.map(\.name) == ["Alex", "Sam", "Kids", "Robin", "Guest"])
        #expect(profiles.map(\.needs) == [.none, .pin, .none, .password, .locked])
        #expect(profiles[0].current)
        #expect(profiles[0].avatarUrl == "/api/users/2/avatar?v=1761100000")
        #expect(profiles[1].avatarUrl == nil)
        #expect(profiles[3].avatarColor == nil)
    }

    @Test func serverInfo() throws {
        let info = try decode(ServerInfo.self, fixture: "server/info.json")
        #expect(info == ServerInfo(name: "Living Room", version: "0.42.0"))
        #expect(info.commit == nil)
    }

    @Test func appUser() throws {
        let me = try decode(User.self, fixture: "me/app_token.json")
        #expect(me.id == 2)
        #expect(me.name == "Alex")
        #expect(me.hasPin)
        #expect(!me.isAdmin)
        #expect(me.libraryIds.isEmpty)
        #expect(me.maxAge == nil)
        #expect(me.subtitleMode == "foreign")
        #expect(me.scopes == ["library", "play", "playlists", "profiles"])
    }

    @Test func errors() throws {
        let notFound = try decode(APIErrorBody.self, fixture: "errors/not_found.json")
        #expect(notFound == APIErrorBody(code: "not_found", message: "item 42 not found"))
        let scope = try decode(APIErrorBody.self, fixture: "errors/insufficient_scope.json")
        #expect(scope.code == "insufficient_scope")
        #expect(scope.message?.hasPrefix("this app wasn't allowed") == true)
    }

    @Test func sessionStoppedKeepsItsCode() throws {
        let body = try decode(APIErrorBody.self, json: #"{"code":"session_stopped","error":"session_stopped","message":null}"#)
        #expect(body == APIErrorBody(code: "session_stopped", message: nil))
    }

    @Test func unknownFieldsAndKindsAreRead() throws {
        let item = try decode(ItemSummary.self, fixture: "items/unknown_fields.json")
        #expect(item.kind == ItemKind(rawValue: "clip"))
        #expect(!ItemKind.known.contains(item.kind))
        #expect(item.title == "Gag Reel")
        #expect(item.extraType == "bloopers")
        #expect(item.media?.dynamicRange == "hdr_vivid")
        #expect(item.media?.width == 1920)
        #expect(item.userState.playCount == 0)
    }

    @Test func nullsReadAsDefaults() throws {
        let item = try decode(ItemSummary.self, fixture: "items/explicit_nulls.json")
        #expect(item.title == "Sicario")
        #expect(item.kind == .movie)
        #expect(item.sortTitle == "")
        #expect(item.addedAt == 0)
        #expect(item.year == nil)
        #expect(item.posterUrl == nil)
        #expect(item.media == nil)
        #expect(item.userState == UserState())
    }

    @Test func missingFieldsReadAsDefaults() throws {
        let item = try decode(ItemSummary.self, json: #"{"id":1,"library_id":2,"kind":"movie","title":"Heat"}"#)
        #expect(item.userState == UserState())
        #expect(item.sortTitle == "")
        let hub = try decode(Hub.self, json: #"{"key":"my_list","title":"My List"}"#)
        #expect(hub.items.isEmpty)
        #expect(hub.libraryId == nil)
    }

    @Test func unknownNeedsDecode() throws {
        let profile = try decode(HouseholdProfile.self, json: #"{"id":9,"name":"New","current":false,"needs":"face"}"#)
        #expect(profile.needs == ProfileNeeds(rawValue: "face"))
        let missing = try decode(HouseholdProfile.self, json: #"{"id":9,"name":"New"}"#)
        #expect(missing.needs == .locked)
    }

    @Test func wireValuesEncodeAsTheirNames() throws {
        let data = try PleyJSON.encoder.encode([ItemKind.episode, ItemKind(rawValue: "clip")])
        #expect(String(decoding: data, as: UTF8.self) == #"["episode","clip"]"#)
    }
}

@Suite("docs/API.md's examples decode")
struct APIExampleTests {
    @Test func tokenAnswer() throws {
        let tokens = try decode(TokenResponse.self, json: """
            {
              "access_token": "pley_at_example",
              "token_type": "Bearer",
              "expires_in": 3600,
              "scope": "library play",
              "refresh_token": "pley_rt_example"
            }
            """)
        #expect(tokens.accessToken == "pley_at_example")
        #expect(tokens.refreshToken == "pley_rt_example")
        #expect(tokens.expiresIn == 3600)
        #expect(tokens.scope == "library play")
        for text in [tokens.description, String(reflecting: tokens), "\(tokens)"] {
            #expect(!text.contains("pley_at_example"))
            #expect(!text.contains("pley_rt_example"))
        }
    }

    @Test func metadata() throws {
        let metadata = try decode(AuthorizationServerMetadata.self, json: """
            {
              "issuer": "http://pley.local:7539",
              "authorization_endpoint": "http://pley.local:7539/oauth/authorize",
              "token_endpoint": "http://pley.local:7539/api/oauth/token",
              "revocation_endpoint": "http://pley.local:7539/api/oauth/revoke",
              "device_authorization_endpoint": "http://pley.local:7539/api/oauth/device",
              "response_types_supported": ["code"],
              "response_modes_supported": ["query"],
              "grant_types_supported": ["authorization_code", "refresh_token",
                                        "urn:pley:params:oauth:grant-type:switch-profile",
                                        "urn:ietf:params:oauth:grant-type:device_code"],
              "token_endpoint_auth_methods_supported": ["none"],
              "revocation_endpoint_auth_methods_supported": ["none"],
              "code_challenge_methods_supported": ["S256"],
              "scopes_supported": ["library", "play", "playlists", "profiles"],
              "authorization_response_iss_parameter_supported": true,
              "client_id_metadata_document_supported": false,
              "service_documentation": "https://github.com/pley-media-server/pley/blob/master/docs/API.md"
            }
            """)
        #expect(metadata.issuer == "http://pley.local:7539")
        #expect(metadata.authorizationEndpoint == "http://pley.local:7539/oauth/authorize")
        #expect(metadata.tokenEndpoint == "http://pley.local:7539/api/oauth/token")
        #expect(metadata.revocationEndpoint == "http://pley.local:7539/api/oauth/revoke")
        #expect(metadata.deviceAuthorizationEndpoint == "http://pley.local:7539/api/oauth/device")
        #expect(metadata.codeChallengeMethodsSupported == ["S256"])
        #expect(metadata.grantTypesSupported?.contains("urn:ietf:params:oauth:grant-type:device_code") == true)
        #expect(metadata.authorizationResponseIssParameterSupported == true)
    }

    @Test func deviceAnswer() throws {
        let device = try decode(DeviceAuthorization.self, json: """
            {
              "device_code": "secret-device-code",
              "user_code": "BCDF-GHJK",
              "verification_uri": "http://pley.local:7539/link",
              "verification_uri_complete": "http://pley.local:7539/link?code=BCDFGHJK",
              "expires_in": 600,
              "interval": 5
            }
            """)
        #expect(device.userCode == "BCDF-GHJK")
        #expect(device.verificationUri == "http://pley.local:7539/link")
        #expect(device.verificationUriComplete == "http://pley.local:7539/link?code=BCDFGHJK")
        #expect(device.expiresIn == 600)
        #expect(device.interval == 5)
        #expect(!device.description.contains("secret-device-code"))
    }

    @Test func oauthError() throws {
        let error = try decode(OAuthErrorBody.self, json: #"{"error":"invalid_grant","error_description":"wrong PIN","reason":"wrong_pin"}"#)
        #expect(error == OAuthErrorBody(error: "invalid_grant", errorDescription: "wrong PIN", reason: "wrong_pin"))
    }
}
