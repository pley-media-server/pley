sub test_format_clock()
    tkEqual(fmtClock(0), "0:00")
    tkEqual(fmtClock(754000), "12:34")
    tkEqual(fmtClock(3723000), "1:02:03")
    tkEqual(fmtClock(59999), "0:59")
    tkEqual(fmtClock(-5), "0:00")
    tkEqual(fmtClock(invalid), "0:00")
end sub

sub test_format_durations()
    tkEqual(fmtDuration(6120000), "1 h 42 min")
    tkEqual(fmtDuration(2520000), "42 min")
    tkEqual(fmtDuration(7200000), "2 h")
    tkEqual(fmtDuration(20000), "1 min")
    tkEqual(fmtDuration(0), "")
    tkEqual(fmtDuration(invalid), "")
    tkEqual(fmtLeft(600000, 3000000), "40 min left")
    tkEqual(fmtLeft(3000000, 3000000), "")
end sub

sub test_format_progress()
    tkEqual(fmtProgress(500, 1000), 0.5)
    tkEqual(fmtProgress(0, 1000), 0)
    tkEqual(fmtProgress(2000, 1000), 1)
    tkEqual(fmtProgress(500, 0), 0)
end sub

sub test_format_episode_codes()
    tkEqual(fmtEpisode(1, 2), "S1 E2")
    tkEqual(fmtEpisode(0, 3), "S0 E3")
    tkEqual(fmtEpisode(invalid, 7), "E7")
    tkEqual(fmtEpisode(1, invalid), "")
end sub

sub test_format_resume_points()
    tkEqual(fmtResume(754000), "Resume from 12:34")
    tkTrue(fmtResumable(600000, 6000000))
    tkFalse(fmtResumable(0, 6000000))
    tkFalse(fmtResumable(500, 6000000), "under a second")
    tkFalse(fmtResumable(5500000, 6000000), "past 90 %, which counts as watched")
    tkTrue(fmtResumable(600000, 0), "a duration the server doesn't know")
end sub

sub test_format_library_counts()
    tkEqual(fmtCount(245, "movies"), "245 movies")
    tkEqual(fmtCount(1, "shows"), "1 show")
    tkEqual(fmtCount(0, "shows"), "0 shows")
    tkEqual(fmtCount(3, "music"), "3 artists")
    tkEqual(fmtCount(3, "other"), "3 titles")
end sub

sub test_format_sizes_as_the_other_apps()
    tkEqual(fmtBytes(512), "512 B")
    tkEqual(fmtBytes(1536), "1.5 KB")
    tkEqual(fmtBytes(157286400), "150 MB")
    tkEqual(fmtBytes(66571993088&), "62.0 GB", "past 32 bits")
    tkEqual(fmtBytes(1000000), "977 KB", "three digits")
    tkEqual(fmtBytes(1048576), "1.0 MB")
    tkEqual(fmtBytes(invalid), "")
end sub

sub test_format_bitrates_as_the_other_apps()
    tkEqual(fmtBitrate(24000000), "24 Mbps")
    tkEqual(fmtBitrate(8200000), "8.2 Mbps")
    tkEqual(fmtBitrate(640000), "640 kbps")
    tkEqual(fmtBitrate(0), "")
    tkEqual(fmtBitrate(invalid), "")
end sub

sub test_format_resolutions_and_dynamic_ranges_by_token()
    tkEqual(fmtTokenLabel(fmtResolutions(), "4k"), "4K")
    tkEqual(fmtTokenLabel(fmtResolutions(), "1080"), "1080p")
    tkEqual(fmtTokenLabel(fmtResolutions(), "sd"), "SD")
    tkEqual(fmtTokenLabel(fmtDynamicRanges(), "hdr"), "Any HDR", "any HDR")
    tkEqual(fmtTokenLabel(fmtDynamicRanges(), "dolby_vision"), "Dolby Vision")
    tkEqual(fmtTokenLabel(fmtDynamicRanges(), "hdr10plus"), "HDR+")
    tkEqual(fmtTokenLabel(fmtDynamicRanges(), "sdr"), "SDR")
    tkEqual(fmtTokenLabel(fmtResolutions(), "4K"), "", "tokens are lower case")
    tkEqual(fmtTokenLabel(fmtResolutions(), "8k"), "")
    tkEqual(fmtTokenLabel(fmtDynamicRanges(), 10), "")
    tkEqual(fmtTokenLabel(fmtDynamicRanges(), invalid), "")
end sub

sub test_format_meta_line()
    tkEqual(fmtMeta({ year: 2016, duration_ms: 6960000, content_rating: "PG-13", rating: 7.9 }), "2016 · 1 h 56 min · PG-13 · ★ 7.9")
    tkEqual(fmtMeta({ year: 2016, rating: 8 }), "2016 · ★ 8")
    tkEqual(fmtMeta({}), "")
    tkEqual(fmtOneDecimal(7.25), "7.3")
end sub

' Sunday 1 November 2026, 12:00 UTC.
sub test_format_times_to_come()
    now = 1793534400
    tkEqual(fmtUntil(now + 30, now), "in a moment")
    tkEqual(fmtUntil(now - 600, now), "in a moment", "due already")
    tkEqual(fmtUntil(now + 20 * 60, now), "in 20 min")
    tkEqual(fmtUntil(now + 3590, now), "in 1 hour")
    tkEqual(fmtUntil(now + 3 * 3600, now), "in 3 hours")
    tkEqual(fmtUntil(now + 23 * 3600 + 2400, now), "tomorrow")
    tkEqual(fmtUntil(now + 3 * 86400, now), "in 3 days")
    tkEqual(fmtUntil(now + 12 * 86400 + 600, now), "in 12 days")
    tkEqual(fmtUntil(invalid, now), "in a moment")
end sub

sub test_format_times_gone()
    now = 1793534400
    tkEqual(fmtAgo(now - 30, now, 0), "now")
    tkEqual(fmtAgo(now - 12 * 60, now, 0), "12 min ago")
    tkEqual(fmtAgo(now - 3600, now, 0), "1 hour ago")
    tkEqual(fmtAgo(now - 3 * 3600, now, 0), "3 hours ago")
    tkEqual(fmtAgo(now - 86400, now, 0), "yesterday")
    tkEqual(fmtAgo(now - 3 * 86400, now, 0), "3 days ago")
    tkEqual(fmtAgo(now - 40 * 86400, now, 0), "Sep 22, 2026")
end sub

sub test_format_days_in_local_time()
    now = 1793534400
    tkEqual(fmtDay(now + 3 * 86400, now, 0), "Nov 4")
    tkEqual(fmtDay(now + 65 * 86400, now, 0), "Jan 5, 2027")
    late = now + 3 * 86400 + 11 * 3600 + 1800
    tkEqual(fmtDay(late, now, -5 * 3600), "Nov 4", "18:30 in New York")
    tkEqual(fmtDay(late, now, 2 * 3600), "Nov 5", "01:30 in Athens")
    tkEqual(fmtTimeOfDay(now + 9 * 3600 + 300, 0), "9:05 PM")
    tkEqual(fmtTimeOfDay(now - 12 * 3600, 0), "12:00 AM")
    tkEqual(fmtTimeOfDay(now, 3600), "1:00 PM")
    tkEqual(fmtDateText("2026-11-04", now, 0), "Nov 4")
    tkEqual(fmtDateText("2027-01-05", now, 0), "Jan 5, 2027")
    tkEqual(fmtDateText("2026-13-01", now, 0), "")
    tkEqual(fmtDateText(invalid, now, 0), "")
    tkTrue(Abs(fmtZoneSeconds()) <= 14 * 3600)
end sub
