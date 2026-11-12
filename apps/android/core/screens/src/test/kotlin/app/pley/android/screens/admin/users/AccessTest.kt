package app.pley.android.screens.admin.users

import app.pley.android.model.admin.RatingSystem
import app.pley.android.model.admin.RatingSystems
import app.pley.android.model.admin.SystemRating
import java.time.LocalDate
import java.time.YearMonth
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Age limits and label exceptions, as the web's lib/access.test.ts and lib/labels.test.ts check them. */
class AccessTest {
    private val systems = RatingSystems(
        region = "US",
        systems = listOf(
            RatingSystem(
                "US",
                "United States",
                listOf(
                    SystemRating("G", 0, "film"),
                    SystemRating("TV-Y", 0, "tv"),
                    SystemRating("TV-Y7", 7, "tv"),
                    SystemRating("PG", 10, "film"),
                    SystemRating("TV-PG", 10, "tv"),
                    SystemRating("PG-13", 13, "film"),
                    SystemRating("R", 17, "film"),
                ),
            ),
            RatingSystem(
                "CA",
                "Canada",
                listOf(
                    SystemRating("G", 0, null),
                    SystemRating("C8", 8, "tv"),
                    SystemRating("PG", 8, "tv"),
                    SystemRating("PG", 10, "film"),
                    SystemRating("14A", 14, "film"),
                ),
            ),
        ),
    )

    @Test
    fun aSystemsRatingsAreOfferedByAgeSayingANamesKindWhereItHasTwoAges() {
        assertEquals(
            listOf(AgeLevel(0, "G, TV-Y"), AgeLevel(7, "TV-Y7"), AgeLevel(10, "PG, TV-PG"), AgeLevel(13, "PG-13"), AgeLevel(17, "R")),
            ageLevels(systems, "US"),
        )
        assertEquals(listOf("G", "C8, PG (tv)", "PG (film)", "14A"), ageLevels(systems, "CA").map { it.rating })
        assertEquals(listOf("C8, PG (TV)"), ageLevels(systems, "CA") { name, kind -> "$name (${kind.uppercase()})" }.filter { it.age == 8 }.map { it.rating })
        assertEquals(22, ageLevels(systems, AGE_SYSTEM).size)
        assertEquals(AgeLevel(8, null), ageLevels(systems, AGE_SYSTEM)[8])
    }

    @Test
    fun aLimitShowsInTheRegionsSystemAndOneNoRatingHasAsAnAge() {
        assertEquals("US", shownSystem(systems, null, null))
        assertEquals(AGE_SYSTEM, shownSystem(systems.copy(region = "ZZ"), null, 13))
        assertEquals("CA", shownSystem(systems, "CA", 14))
        assertEquals(AGE_SYSTEM, shownSystem(systems, null, 8))
        assertEquals("US", shownSystem(systems, "XX", 13))
        assertEquals(AGE_SYSTEM, shownSystem(null, "US", 13))
        assertEquals("PG-13", limitRating(systems, null, 13))
        assertNull(limitRating(systems, null, 8))
        assertNull(limitRating(systems, AGE_SYSTEM, 13))
    }

    @Test
    fun aLimitIsKeptAtOrBelowTheOldOneWhenTheSystemChanges() {
        assertEquals(10, ageIn(systems, "CA", 13))
        assertEquals(7, ageIn(systems, "US", 8))
        assertEquals(8, ageIn(systems, AGE_SYSTEM, 8))
        assertNull(ageIn(systems, "US", null))
    }

    @Test
    fun aBirthdayIsFollowedAsTheServerDoes() {
        val november = LocalDate.of(2026, 11, 15)
        assertEquals(10, birthdayLimit("2015-03", LocalDate.of(2026, 2, 28)))
        assertEquals(11, birthdayLimit("2015-03", LocalDate.of(2026, 3, 1)))
        assertEquals(0, birthdayLimit("2026-11", november))
        assertEquals(20, birthdayLimit("2005-12", november))
        assertNull(birthdayLimit("2005-11", november))
        assertNull(birthdayLimit("2015-13", november))
        assertEquals(YearMonth.of(2036, 3), birthdayLimitEnd("2015-03"))
        assertEquals((2026 downTo 2005).toList(), birthYears(null, november))
        assertEquals(2000, birthYears("2000-04", november).last())
    }

    @Test
    fun howALimitIsSet() {
        assertEquals(LimitMode.None, limitMode(systems, AccessChoice()))
        assertEquals(LimitMode.Rating, limitMode(systems, AccessChoice(maxAge = 13)))
        // No US rating is for 8.
        assertEquals(LimitMode.Age, limitMode(systems, AccessChoice(maxAge = 8)))
        assertEquals(LimitMode.Age, limitMode(systems, AccessChoice(maxAge = 13, maxAgeSystem = AGE_SYSTEM)))
        assertEquals(LimitMode.Birthday, limitMode(systems, AccessChoice(maxAge = 9, birthMonth = "2017-01")))
        assertEquals(LimitMode.Birthday, limitMode(systems, AccessChoice(birthMonth = "1990-01")))
    }

    @Test
    fun ratingsArePickedInTheStoredCountryElseTheRegionsElseTheUs() {
        assertEquals("CA", ratingSystem(systems, "CA"))
        assertEquals("US", ratingSystem(systems, AGE_SYSTEM))
        assertEquals("US", ratingSystem(systems.copy(region = "ZZ"), null))
        assertEquals("US", ratingSystem(RatingSystems("PL", systems.systems.reversed()), null))
        assertNull(ratingSystem(null, null))
    }

    @Test
    fun eachWayKeepsTheLimitAsNearAsItCanAndStartsStrict() {
        val today = LocalDate.of(2026, 11, 15)
        val pg13 = AccessChoice(maxAge = 13)
        assertEquals(AccessChoice(maxAge = 13, maxAgeSystem = AGE_SYSTEM), pg13.withMode(LimitMode.Age, systems, birthday = true, today))
        assertEquals(AccessChoice(maxAge = 0, maxAgeSystem = AGE_SYSTEM), AccessChoice().withMode(LimitMode.Age, systems, birthday = true, today))
        // The region's ratings stay "the region's" (null) unless the limit was in another system.
        assertEquals(AccessChoice(maxAge = 0), AccessChoice().withMode(LimitMode.Rating, systems, birthday = false, today))
        assertEquals(
            AccessChoice(maxAge = 7, maxAgeSystem = "US"),
            AccessChoice(maxAge = 8, maxAgeSystem = AGE_SYSTEM).withMode(LimitMode.Rating, systems, birthday = false, today),
        )
        assertEquals(AccessChoice(maxAge = 10, maxAgeSystem = "CA"), AccessChoice(maxAge = 13, maxAgeSystem = "CA").withMode(LimitMode.Rating, systems, birthday = false, today))
        // Born this month, as many years ago as the limit was.
        assertEquals(AccessChoice(maxAge = 13, birthMonth = "2013-11"), pg13.withMode(LimitMode.Birthday, systems, birthday = true, today))
        assertEquals(AccessChoice(maxAge = 10, birthMonth = "2016-11"), AccessChoice().withMode(LimitMode.Birthday, systems, birthday = true, today))
        // A limit set another way stops following a birthday; an invitation has none to stop.
        val following = AccessChoice(maxAge = 13, birthMonth = "2013-11")
        assertEquals(AccessChoice(), following.withMode(LimitMode.None, systems, birthday = true, today))
        assertEquals(AccessChoice(maxAge = 13, maxAgeSystem = AGE_SYSTEM), following.withMode(LimitMode.Age, systems, birthday = true, today))
        assertEquals(AccessChoice(maxAge = 10, maxAgeSystem = "CA"), pg13.withSystem(systems, "CA"))
        assertEquals(AccessChoice(maxAge = 12, maxAgeSystem = AGE_SYSTEM), pg13.withAge(12))
        assertEquals(AccessChoice(maxAge = 11, birthMonth = "2015-03"), AccessChoice().withBirthMonth("2015-03", today))
    }

    @Test
    fun labelsMatchAsTheServerDoes() {
        assertEquals("kids", labelKey("Kids"))
        assertEquals("École", labelKey("École"))
        assertEquals(listOf("Kids"), addLabel(listOf("Kids"), " kids "))
        assertEquals(listOf("Kids", "Cartoons"), addLabel(listOf("Kids"), " Cartoons "))
        assertEquals(listOf("Kids"), addLabel(listOf("Kids"), "  "))
        assertEquals(MAX_LABEL, addLabel(emptyList(), "x".repeat(200)).single().length)
        assertEquals(listOf("Kids"), inBoth(listOf("Kids", "Anime"), listOf("KIDS")))
        assertEquals(listOf("Kids"), AccessChoice(showLabels = listOf("Kids"), hideLabels = listOf("kids")).labelsInBoth)
        assertTrue(sameLabels(listOf("a", "b"), listOf("b", "a")))
        assertTrue(!sameLabels(listOf("a"), listOf("a", "b")))
    }
}
