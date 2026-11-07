package app.pley.android.model.admin

import app.pley.android.model.PleyJson
import app.pley.android.model.RequestAccess
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Users' and Invitations' answers as the server writes them (apps/server/src/accounts.rs, api/accounts.rs, api/oauth.rs). */
class UsersAndInvitationsTest {
    @Test
    fun anAccountReadsAsTheServerWritesIt() {
        val json = """
            [{"id":1,"username":"ann","name":"Ann","display_name":"Ann","is_admin":true,"managed":false,"home":true,
              "has_pin":false,"avatar_url":"/api/users/1/avatar?v=3","avatar_color":"#38bdf8","all_libraries":true,
              "library_ids":[],"max_age":null,"disabled":false,"created_at":1700000000,"audio_language":null,
              "subtitle_language":null,"subtitle_mode":"foreign","language":null,"has_password":true,"two_step":true,
              "request_access":"auto"},
             {"id":4,"username":"kid","name":"kid","display_name":null,"is_admin":false,"managed":true,"home":true,
              "has_pin":true,"avatar_url":null,"avatar_color":null,"all_libraries":false,"library_ids":[2,5],
              "max_age":7,"disabled":true,"created_at":1700000100,"audio_language":null,"subtitle_language":null,
              "subtitle_mode":"off","language":"fr","has_password":false,"two_step":false,"request_access":"off"}]
        """
        val users = PleyJson.decodeFromString(ListSerializer(UserAccount.serializer()), json)
        val ann = users[0]
        assertTrue(ann.isAdmin)
        assertTrue(ann.twoStep)
        assertEquals("/api/users/1/avatar?v=3", ann.avatarUrl)
        assertEquals(RequestAccess.Auto, ann.requestAccess)
        val kid = users[1]
        assertTrue(kid.managed)
        assertTrue(kid.hasPin)
        assertTrue(kid.disabled)
        assertFalse(kid.allLibraries)
        assertEquals(listOf(2L, 5L), kid.libraryIds)
        assertEquals(7, kid.maxAge)
        assertNull(kid.displayName)
        assertFalse(kid.hasPassword)
        assertEquals(RequestAccess.Off, kid.requestAccess)
    }

    @Test
    fun anAccountFromAServerBeforeRequestsHasNoRequestAccess() {
        val user = PleyJson.decodeFromString(UserAccount.serializer(), """{"id":2,"username":"bea","name":"Bea"}""")
        assertNull(user.requestAccess)
    }

    @Test
    fun aNewAccountLeavesOutWhatIsNotGiven() {
        val body = PleyJson.encodeToJsonElement(NewUserAccount.serializer(), NewUserAccount(username = "kid", managed = true)).jsonObject
        assertEquals(setOf("username", "is_admin", "managed", "home", "all_libraries", "library_ids"), body.keys)
    }

    @Test
    fun devicesAndAppsReadAsTheServerWritesThem() {
        val devices = PleyJson.decodeFromString(
            ListSerializer(UserDevice.serializer()),
            """[{"id":"ab12","name":"Firefox on Windows","user_agent":"Mozilla/5.0","ip":"10.0.0.2","created_at":1,"last_seen_at":2,"current":false}]""",
        )
        assertEquals(UserDevice("ab12", "Firefox on Windows", "Mozilla/5.0", "10.0.0.2", 2), devices.single())

        val apps = PleyJson.decodeFromString(
            ListSerializer(UserApp.serializer()),
            """
            [{"id":7,"client_id":"https://pley.app/android","name":"Pley for Android","first_party":true,"host":"pley.app",
              "scopes":["library","play","server"],"profile":{"id":4,"name":"kid"},"approved_by":{"id":1,"name":"Ann"},
              "device":"Android","ip":null,"created_at":1,"last_used_at":3},
             {"id":8,"client_id":"https://tools.example/app.json","name":"Tool","first_party":false,"host":"tools.example",
              "scopes":["library"],"profile":{"id":1,"name":"Ann"},"approved_by":{"id":1,"name":"Ann"},
              "device":null,"ip":"10.0.0.3","created_at":1,"last_used_at":3}]
            """,
        )
        assertEquals("Pley for Android", apps[0].label)
        assertEquals(listOf("library", "play", "server"), apps[0].scopes)
        assertEquals(UserAppPerson(4, "kid"), apps[0].profile)
        // Only Pley's own apps go by their name; others by host, as their name can't be checked.
        assertEquals("tools.example", apps[1].label)
    }

    @Test
    fun anInvitationReadsAsTheServerWritesIt() {
        val invites = PleyJson.decodeFromString(
            ListSerializer(Invitation.serializer()),
            """
            [{"code":"Xy3k9QpL2m","created_at":1700000000,"expires_at":1700604800,"uses_left":1,"all_libraries":false,
              "library_ids":[3],"max_age":13,"note":"for Sam"},
             {"code":"Ab","created_at":1700000000,"expires_at":null,"uses_left":0,"all_libraries":true,"library_ids":[],
              "max_age":null,"note":null}]
            """,
        )
        assertEquals(Invitation("Xy3k9QpL2m", 1700000000, 1700604800, 1, false, listOf(3), 13, "for Sam"), invites[0])
        assertNull(invites[1].expiresAt)
        assertNull(invites[1].note)
    }

    @Test
    fun aNewInvitationLeavesOutANoteAndAgeLimitNotGiven() {
        val body = PleyJson.encodeToJsonElement(NewInvitation.serializer(), NewInvitation(expiresInDays = 0, uses = 3)).jsonObject
        assertEquals(setOf("expires_in_days", "uses", "all_libraries", "library_ids"), body.keys)
        assertEquals("0", body["expires_in_days"].toString())
    }
}
