using Pley.Tv.Core.Player;

namespace Pley.Tv.Core.Ui;

/// <summary>What the music pages and Now Playing say (the Android app's strings_tv_music.xml).</summary>
public static class MusicText
{
    public static string Artist => Strings.T("music.pages.artist");
    public static string Albums => Strings.T("music.pages.albums");
    public static string Play => Strings.T("music.actions.play");
    public static string Pause => Strings.T("music.actions.pause");
    public static string Shuffle => Strings.T("music.actions.shuffle");
    public static string ShuffleOff => Strings.T("music.actions.shuffleOff");
    public static string NowPlaying => Strings.T("music.actions.nowPlaying");
    public static string NoAlbums => Strings.T("music.pages.noAlbums");
    public static string NoTracks => Strings.T("music.pages.noTracks");
    public static string NothingToPlay => Strings.T("music.pages.nothingToPlay");
    public static string ArtistNotFound => Strings.T("music.pages.artistNotFound");
    public static string AlbumNotFound => Strings.T("music.pages.albumNotFound");
    public static string NotFoundBody => Strings.T("music.pages.notFoundBody");
    public static string ArtistFailed => Strings.T("music.pages.artistFailed");
    public static string AlbumFailed => Strings.T("music.pages.albumFailed");
    public static string Retry => Strings.T("music.actions.retry");
    public static string Back => Strings.T("music.actions.back");

    // Now Playing.
    public static string Queue => Strings.T("music.nowPlaying.upNext");
    public static string Next => Strings.T("music.actions.next");
    public static string Previous => Strings.T("music.actions.previous");
    public static string Position => Strings.T("music.nowPlaying.position");
    public static string Loading => Strings.T("music.nowPlaying.loading");
    public static string NothingPlaying => Strings.T("music.nowPlaying.nothingTitle");
    public static string NothingPlayingBody => Strings.T("music.nowPlaying.nothingBody");

    /// <summary>"Disc 2".</summary>
    public static string Disc(int number) => Strings.T("music.pages.disc", ("number", number));

    /// <summary>"12 albums".</summary>
    public static string AlbumCount(long n) => Strings.Count("music.count.albums", n);

    /// <summary>"40 tracks".</summary>
    public static string TrackCount(long n) => Strings.Count("music.count.tracks", n);

    /// <summary>The sleep button's "Sleep timer: 42 min left": the timer's name and what it says.</summary>
    public static string Sleep(string timer, string left) => Strings.T("music.nowPlaying.sleep", ("timer", timer), ("left", left));

    /// <summary>Why music doesn't play, in danger text under the byline.</summary>
    public static string Error(MusicError error) => error switch
    {
        MusicError.Offline => Strings.T("music.errors.offline"),
        MusicError.SignedOut => Strings.T("music.errors.signedOut"),
        MusicError.Stopped => Strings.T("music.errors.stopped"),
        _ => Strings.T("music.errors.unplayable"),
    };
}
