package app.pley.android.model.admin

import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
import app.pley.android.model.PleyJson
import app.pley.android.model.ScanPhase
import kotlinx.serialization.builtins.ListSerializer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The JSON of managing the libraries, as libraries.rs and admin.rs write and read it. */
class LibrariesAdminTest {
    @Test
    fun librariesCarryTheirFoldersForThisApp() {
        val json = """[{"id":3,"name":"Films","kind":"movies","paths":["/mnt/a","/mnt/b"],"item_count":1204,
            "created_at":1700000000,"scanned_at":null,"scanning":true}]"""
        val library = PleyJson.decodeFromString(ListSerializer(Library.serializer()), json).single()
        assertEquals(listOf("/mnt/a", "/mnt/b"), library.paths)
        assertEquals(1204, library.itemCount)
        assertNull(library.scannedAt)
        assertTrue(library.scanning)
    }

    @Test
    fun theActivitysScansAreReadAndTheRestLeftToTheDashboard() {
        val json = """{"scans":[{"library_id":3,"library_name":"Films","phase":"probing","processed":12,"total":40,"started_at":1},
            {"library_id":4,"library_name":"Music","phase":"done","processed":0,"total":0,"started_at":1}],
            "sessions":[{"id":"x"}],"previews_pending":2}"""
        val scans = PleyJson.decodeFromString(LibraryScans.serializer(), json).scans
        assertEquals(LibraryScan(3, ScanPhase.Probing, 12, 40), scans[0])
        assertTrue(scans[0].running)
        assertFalse(scans[1].running)
        // A phase a newer server adds still runs.
        val newer = PleyJson.decodeFromString(LibraryScans.serializer(), """{"scans":[{"library_id":1,"phase":"thinking"}]}""")
        assertTrue(newer.scans.single().running)
    }

    @Test
    fun aNewLibraryAndAChangeAreSentAsTheServerReadsThem() {
        assertEquals(
            """{"name":"Films","kind":"movies","paths":["/mnt/a"]}""",
            PleyJson.encodeToString(NewLibrary.serializer(), NewLibrary("Films", LibraryKind.Movies, listOf("/mnt/a"))),
        )
        assertEquals("""{"name":"Films"}""", PleyJson.encodeToString(LibraryChange.serializer(), LibraryChange(name = "Films")))
        assertEquals("""{"force":true}""", PleyJson.encodeToString(LibraryScanRequest.serializer(), LibraryScanRequest(true)))
    }

    @Test
    fun foldersAreAskedForAndRead() {
        assertEquals("{}", PleyJson.encodeToString(FolderQuery.serializer(), FolderQuery(null)))
        assertEquals("""{"path":"D:\\Media"}""", PleyJson.encodeToString(FolderQuery.serializer(), FolderQuery("D:\\Media")))
        val roots = PleyJson.decodeFromString(
            FolderListing.serializer(),
            """{"path":null,"parent":null,"entries":[{"name":"/","path":"/"},{"name":"Home","path":"/home/ann"}]}""",
        )
        assertNull(roots.path)
        assertEquals(FolderEntry("Home", "/home/ann"), roots.entries[1])
        val folder = PleyJson.decodeFromString(FolderListing.serializer(), """{"path":"/mnt","parent":"/","entries":[]}""")
        assertEquals("/", folder.parent)
        assertTrue(folder.entries.isEmpty())
    }
}
