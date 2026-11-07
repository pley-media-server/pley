package app.pley.android.model.admin

import app.pley.android.model.ItemKind
import app.pley.android.model.JobState
import app.pley.android.model.PlaybackMethod
import app.pley.android.model.PleyJson
import app.pley.android.model.ScanPhase
import app.pley.android.model.StreamAction
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The dashboard's answers, as docs/API.md and apps/server/src/api/activity.rs and admin.rs write them. */
class DashboardTest {
    @Test
    fun activityReadsEveryonesSessionsAndTheLibrariesWork() {
        val json = """
            {
              "scans": [{ "library_id": 3, "library_name": "Films", "phase": "probing", "processed": 40, "total": 120, "started_at": 1700000000 }],
              "sessions": [{
                "session_id": "a1b2", "kind": "item", "user_id": 2, "username": "bea", "device": "Living room TV",
                "device_id": "d1", "device_kind": "tv",
                "item": { "id": 9, "library_id": 3, "kind": "episode", "title": "Pilot", "grandparent_id": 7, "grandparent_title": "A show",
                          "parent_index": 1, "index": 1, "duration_ms": 3000000 },
                "method": "transcode", "position_ms": 120000, "state": "paused", "started_at": 1, "updated_at": 2,
                "transcode": {
                  "job_id": "j1", "state": "running", "video": "transcode", "audio": "copy", "video_in": "HEVC 3840×2160",
                  "video_out": "H.264 1920×1080", "audio_in": "EAC3 5.1", "audio_out": null, "encoder": "h264_nvenc",
                  "decoder": "cuda", "pipeline": "gpu", "tonemap": "libplacebo", "subtitle_burn": false, "speed": 2.5, "fps": 60.0,
                  "transcoded_ms": 90000, "segments_ready": 12, "segments_total": 600, "last_error": null, "rendition": 1, "variants": 2
                }
              }],
              "analysis": { "item_id": 5, "title": "A show — Season 1", "done": 3, "total": 10 },
              "seasons_pending": 4, "previews_pending": 12, "previews_running": 2,
              "library_work": { "mode": "auto", "waiting": "hours", "resumes_at": 1700003600, "learned": true }
            }
        """.trimIndent()
        val activity = PleyJson.decodeFromString(DashboardActivity.serializer(), json)
        assertEquals(ScanPhase.Probing, activity.scans.single().phase)
        assertEquals("Films", activity.scans.single().libraryName)
        val session = activity.sessions.single()
        assertEquals("a1b2", session.sessionId)
        assertEquals("bea", session.username)
        assertEquals("Living room TV", session.device)
        assertEquals(ItemKind.Episode, session.item.kind)
        assertEquals(PlaybackMethod.Transcode, session.method)
        assertEquals("paused", session.state)
        val tc = session.transcode!!
        assertEquals(JobState.Running, tc.state)
        assertEquals(StreamAction.Copy, tc.audio)
        assertEquals("gpu", tc.pipeline)
        assertEquals(1, tc.rendition)
        assertEquals(2, tc.variants)
        assertEquals("A show — Season 1", activity.analysis?.title)
        assertEquals(4, activity.seasonsPending)
        assertEquals(2, activity.previewsRunning)
        assertEquals("hours", activity.libraryWork?.waiting)
        assertEquals(1700003600L, activity.libraryWork?.resumesAt)
    }

    @Test
    fun aServerBeforeTheLibrariesWorkLeavesItOut() {
        val activity = PleyJson.decodeFromString(DashboardActivity.serializer(), """{ "scans": [], "sessions": [], "analysis": null }""")
        assertEquals(0, activity.seasonsPending)
        assertNull(activity.libraryWork)
        assertTrue(activity.sessions.isEmpty())
    }

    @Test
    fun systemReadsTheServerItsTranscoderAndWhatTheLibrariesHold() {
        val json = """
            {
              "name": "Home", "version": "0.1.6", "commit": "abc1234", "os": "linux", "arch": "x86_64", "uptime_secs": 90061,
              "data_dir": "/data", "transcode_dir": { "in_use": "/t", "next": null, "default": "/t", "problem": null, "fixed": false },
              "secret_key_ok": true,
              "transcoder": {
                "available": true, "managed": true, "url": "http://127.0.0.1:7540", "version": "0.1.6", "commit": null,
                "license": "AGPL-3.0-only", "ffmpeg_version": "7.1", "encoders": ["libx264", "h264_nvenc"], "active_encoder": "h264_nvenc",
                "hwaccels": ["cuda"], "accels": ["nvenc"], "hardware_decoding": true, "tonemapping": ["libplacebo"], "dolby_vision": true,
                "chromaprint": true, "throttling": true, "max_jobs": 4, "active_jobs": 1, "error": null
              },
              "counts": { "libraries": 3, "movies": 1200, "shows": 80, "episodes": 4000, "artists": 10, "albums": 30, "tracks": 400, "files": 5600, "users": 4 }
            }
        """.trimIndent()
        val system = PleyJson.decodeFromString(DashboardSystem.serializer(), json)
        assertEquals("Home", system.name)
        assertEquals(90061L, system.uptimeSecs)
        assertTrue(system.transcoder.managed)
        assertEquals(listOf("libx264", "h264_nvenc"), system.transcoder.encoders)
        assertEquals("h264_nvenc", system.transcoder.activeEncoder)
        assertTrue(system.transcoder.dolbyVision)
        assertEquals(4000, system.counts.episodes)
        assertEquals(4, system.counts.users)
    }

    @Test
    fun theTranscodersStatusReadsItsCountersAndJobs() {
        val json = """
            {
              "available": true, "version": "0.1.6", "uptime_secs": 10, "max_jobs": 4, "running_processes": 1, "hw_processes": 1,
              "max_hw_jobs": 3, "work_dir_bytes": 1048576,
              "counters": { "jobs_created": 7, "jobs_failed": 1, "processes_started": 9, "process_failures": 2, "segments_produced": 300,
                            "segments_served": 280, "bytes_served": 999, "probes": 4, "probe_failures": 0, "probe_ms_total": 400,
                            "thumbnails": 5, "subtitle_extractions": 6, "keyframe_scans": 0 },
              "jobs": [{
                "id": "j1", "label": "A film", "owner": "bea", "state": "idle", "created_at_ms": 1, "last_access_ms": 2,
                "info": { "job_id": "j1", "state": "idle", "video": "copy", "audio": "transcode", "video_in": "HEVC", "video_out": null,
                          "audio_in": "TrueHD 7.1", "audio_out": "AAC 2.0", "encoder": null, "decoder": null, "tonemap": null,
                          "subtitle_burn": true, "speed": null, "fps": null, "transcoded_ms": 0, "segments_ready": 0, "segments_total": 0,
                          "last_error": null },
                "restarts": 2, "served_bytes": 4096, "warnings": ["slow disk"]
              }]
            }
        """.trimIndent()
        val status = PleyJson.decodeFromString(DashboardTranscoderStatus.serializer(), json)
        assertEquals(3, status.maxHwJobs)
        assertEquals(400L, status.counters.probeMsTotal)
        val job = status.jobs.single()
        assertEquals(JobState.Idle, job.state)
        assertEquals(StreamAction.Copy, job.info.video)
        assertTrue(job.info.subtitleBurn)
        assertEquals(listOf("slow disk"), job.warnings)
        assertEquals(0, job.info.variants)
    }

    @Test
    fun resourcesStorageAndTheDatabasesHealthRead() {
        val resources = PleyJson.decodeFromString(
            DashboardResources.serializer(),
            """{ "available": true, "interval_ms": 2000, "cpus": 8, "memory_total": 17179869184,
                 "samples": [{ "at": 1700000000000, "cpu": 12.5, "pley_cpu": 3.25, "memory": 8589934592, "pley_memory": 536870912 }] }""",
        )
        assertEquals(8, resources.cpus)
        assertEquals(3.25, resources.samples.single().pleyCpu, 0.0)
        assertEquals(536870912L, resources.samples.single().pleyMemory)
        assertFalse(PleyJson.decodeFromString(DashboardResources.serializer(), """{ "available": false, "interval_ms": 2000, "cpus": 0, "memory_total": 0, "samples": [] }""").available)

        val usage = PleyJson.decodeFromString(
            StorageUsage.serializer(),
            """{ "blobs": 10, "blob_bytes": 2048, "media_folders": 3, "media_bytes": 4096, "cache_bytes": 512 }""",
        )
        assertEquals(StorageUsage(10, 2048, 3, 4096, 512), usage)
        val report = PleyJson.decodeFromString(
            StorageCleanReport.serializer(),
            """{ "blobs_removed": 2, "media_folders_removed": 1, "subtitles_removed": 0, "bytes_freed": 3000, "cache_files_removed": 4, "cache_bytes_freed": 10 }""",
        )
        assertEquals(StorageCleanReport(2, 1, 3000), report)

        val damaged = PleyJson.decodeFromString(
            DashboardSystem.serializer(),
            """{ "name": "Home", "database": { "ok": false, "problem": "page 4 is damaged", "checked_at": 1700000000 } }""",
        )
        assertEquals(false, damaged.database.ok)
        assertEquals("page 4 is damaged", damaged.database.problem)
        // Not checked yet.
        assertNull(PleyJson.decodeFromString(DashboardSystem.serializer(), """{ "database": { "ok": null, "checked_at": null } }""").database.ok)
    }
}
