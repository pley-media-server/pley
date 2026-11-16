package app.pley.android.ui.phone.admin.users

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import app.pley.android.model.admin.RatingSystem
import app.pley.android.model.admin.RatingSystems
import app.pley.android.model.admin.SystemRating
import app.pley.android.screens.admin.users.AccessChoice
import app.pley.android.ui.phone.theme.PhoneTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * An account's age limit while the rating systems are read: one that may
 * be a rating waits for them, instead of showing (and offering to change)
 * a plain age that isn't what the account has.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w412dp-h915dp-xxhdpi")
class AccessGroupUiTest {
    @get:Rule
    val compose = createComposeRule()

    private val us = RatingSystems(
        region = "US",
        systems = listOf(RatingSystem("US", "United States", listOf(SystemRating("G", 0, "film"), SystemRating("PG-13", 13, "film")))),
    )

    @Test
    fun aLimitByRatingWaitsForTheRatingSystems() {
        var sources by mutableStateOf(AccessSources(systemsLoading = true))
        val sent = mutableListOf<AccessChoice>()
        compose.setContent {
            PhoneTheme {
                AccessGroup(
                    access = AccessChoice(maxAge = 13),
                    libraries = emptyList(),
                    sources = sources,
                    onLibraries = { _, _ -> },
                    onAgeLimit = { sent += it },
                    onLabels = { _, _ -> },
                    pickLibrary = "",
                    birthday = true,
                )
            }
        }
        compose.onNodeWithTag(ACCESS_AGE_LIMIT).assertIsNotEnabled()
        compose.onNodeWithText("By age").assertDoesNotExist()
        compose.onNodeWithText("Titles suitable for age 13, from any country's ratings.").assertDoesNotExist()

        sources = AccessSources(systems = us)
        compose.onNodeWithTag(ACCESS_AGE_LIMIT).assertIsEnabled()
        compose.onNodeWithText("By rating").assertExists()
        compose.onNodeWithText("Titles suitable for age 13: PG-13", substring = true).assertExists()
        assertEquals(emptyList<AccessChoice>(), sent)
    }
}
