sub test_sync_steps_of_a_tenth_within_ten_seconds()
    tkEqual(syncClamp(0), 0)
    tkEqual(syncClamp(250), 300, "rounded to a step")
    tkEqual(syncClamp(-249), -200)
    tkEqual(syncClamp(12345), 10000, "at most 10 s later")
    tkEqual(syncClamp(-99999), -10000, "at most 10 s earlier")
    tkEqual(syncClamp("soon"), 0)
end sub

sub test_sync_labels_as_android_tv()
    tkEqual(syncLabel(300), "+0.3 s")
    tkEqual(syncLabel(-1200), "-1.2 s")
    tkEqual(syncLabel(0), "0.0 s")
    tkEqual(syncLabel(10000), "+10.0 s")
    i18nUse("fr")
    tkEqual(syncLabel(300), "+0,3 s", "a decimal comma")
    i18nUse("es")
    tkEqual(syncLabel(-1200), "-1,2 s")
end sub

sub test_sync_moves_tracks_by_address()
    track = "http://nas:7539/api/files/7/subtitles/3.vtt"
    tkEqual(syncTrackUrl(track, 300), track + "?offset_ms=300")
    tkEqual(syncTrackUrl(track + "?offset_ms=300", -500), track + "?offset_ms=-500", "moved again from scratch")
    tkEqual(syncTrackUrl(track + "?offset_ms=300", 0), track, "back as it was")
    tkEqual(syncTrackUrl("", 300), "")
    tkEqual(syncTrackUrl("http://nas:7539/s.vtt?lang=en", 100), "http://nas:7539/s.vtt?lang=en&offset_ms=100")
    tkEqual(syncBaseUrl("http://nas:7539/s.vtt?lang=en&offset_ms=100"), "http://nas:7539/s.vtt?lang=en")
    tracks = [{ Language: "eng", TrackName: track, Description: "English" }]
    moved = syncTracks(tracks, 300)
    tkEqual(moved[0], { Language: "eng", TrackName: track + "?offset_ms=300", Description: "English" })
    tkEqual(tracks[0].TrackName, track, "the tracks given stay as they were")
end sub

sub test_sync_remembers_each_file()
    section = tkRegistrySection()
    feature = syncKey("http://nas:7539", 7)
    tkEqual(feature, "http://nas:7539|7")
    tkEqual(syncOffset(section, feature), 0, "nothing remembered")
    tkTrue(syncRemember(section, feature, 300))
    tkEqual(section.flushes, 1)
    tkEqual(syncOffset(section, feature), 300)
    tkEqual(syncOffset(section, syncKey("http://nas:7539", 8)), 0, "another file's")
    tkEqual(syncOffset(section, syncKey("http://other:7539", 7)), 0, "another server's")
    syncRemember(section, feature, -200)
    tkEqual(syncOffset(section, feature), -200)
    syncRemember(section, feature, 0)
    tkEqual(syncOffset(section, feature), 0, "Reset forgets it")
    tkEqual(section.Read("offsets"), "[]")
end sub

sub test_sync_keeps_the_latest_files()
    section = tkRegistrySection()
    for i = 1 to syncKeep() + 5
        syncRemember(section, syncKey("http://nas:7539", i), 100)
    end for
    tkEqual(syncOffset(section, syncKey("http://nas:7539", 1)), 0, "the oldest went")
    tkEqual(syncOffset(section, syncKey("http://nas:7539", syncKeep() + 5)), 100)
    tkEqual(syncPairs(section).Count(), syncKeep())
    ' Moved again, a file is the latest.
    syncRemember(section, syncKey("http://nas:7539", 6), 200)
    syncRemember(section, syncKey("http://nas:7539", 1000), 100)
    tkEqual(syncOffset(section, syncKey("http://nas:7539", 6)), 200)
    ' Something else in the registry is no offset.
    section.Write("offsets", "not json")
    tkEqual(syncOffset(section, syncKey("http://nas:7539", 6)), 0)
end sub
