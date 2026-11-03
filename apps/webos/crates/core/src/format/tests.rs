//! packages/tv/test/format.test.ts, ported, and the rest of the labels.

use super::*;

fn video(f: impl FnOnce(&mut MediaStream)) -> MediaStream {
    let mut s = MediaStream {
        index: 0,
        kind: StreamKind::Video,
        codec: "hevc".into(),
        width: Some(3840),
        height: Some(2160),
        default: true,
        ..MediaStream::default()
    };
    f(&mut s);
    s
}

fn p81() -> MediaStream {
    video(|s| {
        s.dynamic_range = Some(DynamicRange::DolbyVision);
        s.dv_profile = Some(8);
        s.dv_bl_compat = Some(1);
        s.dv_level = Some(6);
    })
}

fn p7() -> MediaStream {
    video(|s| {
        s.dynamic_range = Some(DynamicRange::DolbyVision);
        s.dv_profile = Some(7);
        s.dv_bl_compat = Some(6);
        s.dv_level = Some(6);
    })
}

fn with(mut s: MediaStream, f: impl FnOnce(&mut MediaStream)) -> MediaStream {
    f(&mut s);
    s
}

#[test]
fn names_each_dolby_vision_profile() {
    assert_eq!(stream_range(&p81()).as_deref(), Some("Dolby Vision P8.1"));
    assert_eq!(stream_range(&with(p81(), |s| s.dv_bl_compat = Some(4))).as_deref(), Some("Dolby Vision P8.4"));
    let p5 = with(p81(), |s| {
        s.dv_profile = Some(5);
        s.dv_bl_compat = Some(0);
    });
    assert_eq!(stream_range(&p5).as_deref(), Some("Dolby Vision P5"));
    // Profile 7's base layer (Blu-ray, compatibility 6) isn't named: never "P7.6".
    assert_eq!(stream_range(&p7()).as_deref(), Some("Dolby Vision P7"));
    assert_eq!(stream_range(&with(p7(), |s| s.dv_el = Some("fel".into()))).as_deref(), Some("Dolby Vision P7 FEL"));
    assert_eq!(stream_range(&with(p7(), |s| s.dv_el = Some("mel".into()))).as_deref(), Some("Dolby Vision P7 MEL"));
}

#[test]
fn names_hdr10_plus_alone_and_with_dolby_vision() {
    assert_eq!(stream_range(&with(p81(), |s| s.hdr10plus = Some(true))).as_deref(), Some("Dolby Vision P8.1 + HDR10+"));
    let range = |r: DynamicRange, plus: Option<bool>| {
        stream_range(&video(|s| {
            s.dynamic_range = Some(r);
            s.hdr10plus = plus;
        }))
    };
    assert_eq!(range(DynamicRange::Hdr10Plus, Some(true)).as_deref(), Some("HDR10+"));
    assert_eq!(range(DynamicRange::Hdr10, Some(false)).as_deref(), Some("HDR10"));
    assert_eq!(range(DynamicRange::Hlg, None).as_deref(), Some("HLG"));
    assert_eq!(range(DynamicRange::Sdr, None), None);
    assert_eq!(dynamic_range_label(None, None, None, Some(true), None, None).as_deref(), Some("HDR"));
}

#[test]
fn badges_a_hybrid_twice() {
    assert_eq!(dynamic_range_badges(&with(p81(), |s| s.hdr10plus = Some(true))), ["DV P8.1", "HDR10+"]);
    assert_eq!(dynamic_range_badges(&with(p7(), |s| s.dv_el = Some("fel".into()))), ["DV P7 FEL"]);
    let file = MediaFile {
        id: 1,
        path: "a.mkv".into(),
        size: 1,
        width: Some(3840),
        height: Some(2160),
        streams: vec![with(p81(), |s| s.hdr10plus = Some(true))],
        ..MediaFile::default()
    };
    assert_eq!(quality_badges(&file), ["4K", "DV P8.1", "HDR10+"]);
}

#[test]
fn gives_the_info_panel_the_profile_level_base_layer_and_enhancement_layer() {
    assert_eq!(
        dolby_vision_details(&with(p7(), |s| s.dv_el = Some("fel".into()))).as_deref(),
        Some("Profile 7 · Level 6 · Blu-ray HDR10-compatible base layer · full enhancement layer")
    );
    assert_eq!(
        dolby_vision_details(&with(p81(), |s| s.dv_bl_compat = Some(6))).as_deref(),
        Some("Profile 8 · Level 6 · Blu-ray HDR10-compatible base layer")
    );
    let block = stream_block(&with(p81(), |s| s.hdr10plus = Some(true)), "Video");
    assert_eq!(block.value("HDR"), Some("Dolby Vision P8.1 + HDR10+"));
}

fn subtitle(title: Option<&str>, f: impl FnOnce(&mut MediaStream)) -> String {
    let mut s = MediaStream {
        index: 6,
        kind: StreamKind::Subtitle,
        codec: "subrip".into(),
        language: Some("eng".into()),
        title: title.map(str::to_string),
        ..MediaStream::default()
    };
    f(&mut s);
    stream_label(&s)
}

#[test]
fn tells_apart_subtitles_whose_titles_name_their_language() {
    // Forced by its title alone, as the server counts it: never two "English (SRT)".
    assert_eq!(subtitle(Some("English"), |_| {}), "English (SRT)");
    assert_eq!(subtitle(Some("English (Forced)"), |_| {}), "English (SRT, Forced)");
    assert_eq!(subtitle(Some("Forced"), |_| {}), "English (SRT, Forced)");
    assert_eq!(subtitle(Some("Signs & Songs"), |_| {}), "English – Signs & Songs (SRT, Forced)");
    assert_eq!(subtitle(Some("English SDH"), |_| {}), "English – SDH (SRT)");
    assert_eq!(subtitle(Some("Designs"), |_| {}), "English – Designs (SRT)");
    let french = subtitle(None, |s| {
        s.language = Some("fre".into());
        s.forced = true;
        s.external = true;
    });
    assert_eq!(french, "French (SRT, Forced, External)");
    let signs = MediaStream { kind: StreamKind::Subtitle, title: Some("Signs/Songs".into()), ..MediaStream::default() };
    assert!(is_forced_subtitle(&signs));
}

fn audio(index: i64, codec: &str, channels: i64, default: bool, language: &str) -> MediaStream {
    MediaStream {
        index,
        kind: StreamKind::Audio,
        codec: codec.into(),
        channels: Some(channels),
        language: Some(language.into()),
        default,
        ..MediaStream::default()
    }
}

fn file(picked: Option<bool>, selected: i64) -> MediaFile {
    MediaFile {
        id: 1,
        path: "a.mkv".into(),
        size: 1,
        streams: vec![
            audio(1, "truehd", 6, true, "eng"),
            audio(2, "aac", 2, false, "eng"),
            audio(3, "ac3", 6, false, "fre"),
        ],
        selected_audio: Some(selected),
        audio_picked: picked,
        ..MediaFile::default()
    }
}

#[test]
fn without_a_pick_audio_is_automatic_with_the_language() {
    assert_eq!(automatic_audio_label(&file(Some(false), 2)), "Automatic (English)");
    assert_eq!(automatic_audio_label(&file(Some(false), 3)), "Automatic (French)");
    let und = MediaFile { streams: vec![audio(1, "aac", 2, true, "und")], ..file(Some(false), 1) };
    assert_eq!(automatic_audio_label(&und), "Automatic");
    assert_eq!(starting_audio(&file(Some(false), 2)).map(|s| s.index), Some(1));
}

#[test]
fn with_a_pick_or_from_a_server_that_doesnt_say_audio_is_the_track() {
    assert_eq!(starting_audio(&file(Some(true), 2)).map(|s| s.index), Some(2));
    assert_eq!(starting_audio(&file(None, 3)).map(|s| s.index), Some(3));
}

#[test]
fn tracks_are_labelled_as_the_pickers_list_them() {
    assert_eq!(stream_label(&audio(1, "eac3", 6, true, "eng")), "English (EAC3 5.1)");
    let commentary = MediaStream { title: Some("Commentary".into()), ..audio(2, "aac", 2, false, "eng") };
    assert_eq!(stream_label(&commentary), "English – Commentary (AAC Stereo)");
    let atmos =
        MediaStream { profile: Some("Dolby TrueHD + Dolby Atmos".into()), ..audio(3, "truehd", 8, false, "eng") };
    assert_eq!(audio_format_label(&atmos), "TrueHD Atmos 7.1");
    let v = video(|s| {
        s.dynamic_range = Some(DynamicRange::DolbyVision);
        s.dv_profile = Some(8);
        s.dv_bl_compat = Some(1);
        s.bit_depth = Some(10);
    });
    assert_eq!(stream_label(&v), "4K HEVC Dolby Vision P8.1 10-bit");
}

#[test]
fn codecs_channels_and_sizes() {
    assert_eq!(codec_label("hdmv_pgs_subtitle"), "PGS");
    assert_eq!(codec_label("h264"), "H.264");
    assert_eq!(codec_label("weird"), "WEIRD");
    assert_eq!(channels_label(Some(6), None), "5.1");
    assert_eq!(channels_label(Some(5), None), "5ch");
    assert_eq!(channels_label(None, Some("stereo")), "stereo");
    assert_eq!(resolution_label(Some(1920), Some(800)), Some("1080p"));
    assert_eq!(resolution_label(Some(720), Some(576)), Some("576p"));
    assert_eq!(resolution_label(None, None), None);
    assert_eq!(resolution_badge(Some(1280), Some(536)), Some("720p"));
    assert_eq!(resolution_badge(Some(640), Some(360)), Some("SD"));
}

#[test]
fn languages() {
    assert_eq!(language_name(Some("eng")).as_deref(), Some("English"));
    assert_eq!(language_name(Some("pt-BR")).as_deref(), Some("Portuguese (BR)"));
    assert_eq!(language_name(Some("xyz")).as_deref(), Some("XYZ"));
    assert_eq!(language_name(Some("klingon")).as_deref(), Some("klingon"));
    assert_eq!(language_name(Some(" ")), None);
    assert_eq!(two_letter_code("eng"), "en");
    assert_eq!(two_letter_code("chi"), "zh");
    assert_eq!(two_letter_code("FR"), "fr");
    assert_eq!(two_letter_code("yue"), "yue");
}

#[test]
fn items() {
    assert_eq!(episode_label(Some(1), Some(3)), "S1 · E3");
    assert_eq!(episode_label(None, Some(3)), "E3");
    assert_eq!(season_title(None, Some(0)), "Specials");
    assert_eq!(season_title(Some("Book One"), Some(1)), "Book One");
    assert_eq!(season_title(None, Some(2)), "Season 2");
    assert_eq!(pluralize(1, "movie", None), "1 movie");
    assert_eq!(pluralize(12_345, "movie", None), "12,345 movies");
    let show = ItemSummary { kind: ItemKind::Show, child_count: Some(3), year: Some(2008), ..ItemSummary::default() };
    assert_eq!(card_subtitle(&show), "3 seasons");
    let album = ItemSummary {
        kind: ItemKind::Album,
        parent_title: Some("Daft Punk".into()),
        year: Some(2001),
        ..ItemSummary::default()
    };
    assert_eq!(card_subtitle(&album), "Daft Punk · 2001");
    let episode =
        ItemSummary { kind: ItemKind::Episode, parent_index: Some(2), index: Some(5), ..ItemSummary::default() };
    assert_eq!(card_subtitle(&episode), "S2 · E5");
    let mut movie = ItemSummary { kind: ItemKind::Movie, duration_ms: Some(6_000_000), ..ItemSummary::default() };
    movie.user_state.position_ms = 1_500_000;
    assert!(is_in_progress(&movie));
    assert_eq!(progress_fraction(1_500_000, Some(6_000_000)), 0.25);
    let season = ItemSummary {
        kind: ItemKind::Season,
        leaf_count: Some(10),
        viewed_leaf_count: Some(4),
        ..ItemSummary::default()
    };
    assert_eq!(unwatched_count(&season), 6);
}

#[test]
fn times() {
    assert_eq!(format_duration(Some(6_720_000)), "1h 52m");
    assert_eq!(format_duration(Some(2_700_000)), "45m");
    assert_eq!(format_duration(Some(29_000)), "29s");
    assert_eq!(format_duration(Some(7_200_000)), "2h");
    assert_eq!(format_runtime(Some(3_480_000)), "58 min");
    assert_eq!(format_runtime(Some(6_720_000)), "1h 52m");
    assert_eq!(format_time_left(1_000_000, Some(2_560_000)), "26 min left");
    assert_eq!(format_time_left(2_560_000, Some(2_560_000)), "");
    assert_eq!(format_clock(Some(3_723_000)), "1:02:03");
    assert_eq!(format_clock(Some(123_000)), "2:03");
    assert_eq!(format_clock(None), "0:00");
    assert_eq!(format_time_of_day(21, 5, true), "21:05");
    assert_eq!(format_time_of_day(21, 5, false), "9:05 PM");
    assert_eq!(format_time_of_day(0, 30, false), "12:30 AM");
}

#[test]
fn sizes_and_rates() {
    assert_eq!(format_bytes(Some(1536)), "1.5 KB");
    assert_eq!(format_bytes(Some(150 * 1024 * 1024)), "150 MB");
    assert_eq!(format_bytes(Some(512)), "512 B");
    assert_eq!(format_bytes(Some(1280)), "1.3 KB", "halves round up, as JavaScript's toFixed");
    assert_eq!(format_bitrate(Some(8_200_000)), "8.2 Mbps");
    assert_eq!(format_bitrate(Some(12_000_000)), "12 Mbps");
    assert_eq!(format_bitrate(Some(320_000)), "320 kbps");
    assert_eq!(format_bitrate(None), "");
}

#[test]
fn versions_and_media_facts() {
    let f = MediaFile {
        id: 1,
        path: "/media/Movies/Blade Runner {edition-Final Cut}.mkv".into(),
        size: 62_491_000_000,
        container: Some("mkv".into()),
        width: Some(3840),
        height: Some(2160),
        video_codec: Some("hevc".into()),
        edition: Some("Final Cut".into()),
        duration_ms: Some(7_020_000),
        bitrate: Some(71_200_000),
        streams: vec![
            with(p81(), |s| {
                s.level = Some(153);
                s.profile = Some("Main 10".into());
                s.frame_rate = Some(23.976);
                s.bit_depth = Some(10);
                s.pix_fmt = Some("yuv420p10le".into());
                s.color_transfer = Some("smpte2084".into());
            }),
            audio(1, "truehd", 8, true, "eng"),
            audio(2, "ac3", 6, false, "fre"),
        ],
        ..MediaFile::default()
    };
    assert_eq!(version_label(&f, 0), "Final Cut · 4K · HEVC · MKV · 58.2 GB");
    assert_eq!(version_label(&MediaFile::default(), 1), "Version 2");
    let blocks = media_facts(&f);
    let titles: Vec<&str> = blocks.iter().map(|b| b.title.as_str()).collect();
    assert_eq!(titles, ["File", "Video", "Audio 1", "Audio 2"]);
    assert_eq!(blocks[0].value("Name"), Some("Blade Runner {edition-Final Cut}.mkv"));
    assert_eq!(blocks[0].value("Overall bitrate"), Some("71 Mbps"));
    assert_eq!(blocks[1].value("Profile"), Some("Main 10 · Level 5.1"));
    assert_eq!(blocks[1].value("Frame rate"), Some("23.976 fps"));
    assert_eq!(blocks[1].value("Bit depth"), Some("10-bit · 4:2:0"));
    assert_eq!(blocks[1].value("Transfer"), Some("PQ (SMPTE ST 2084)"));
    assert_eq!(blocks[2].value("Channels"), Some("7.1"));
    assert_eq!(blocks[2].value("Flags"), Some("Default"));
    assert_eq!(level_label("h264", Some(51)).as_deref(), Some("5.1"));
    assert_eq!(level_label("av1", Some(13)).as_deref(), Some("5.1"));
    assert_eq!(level_label("hevc", Some(120)).as_deref(), Some("4"));
    assert_eq!(sample_rate_label(Some(44_100)).as_deref(), Some("44.1 kHz"));
    assert_eq!(sample_rate_label(Some(48_000)).as_deref(), Some("48 kHz"));
    assert_eq!(frame_rate_label(Some(25.0), true).as_deref(), Some("25 fps, interlaced"));
    assert_eq!(file_name("C:\\media\\a.mkv"), "a.mkv");
}
