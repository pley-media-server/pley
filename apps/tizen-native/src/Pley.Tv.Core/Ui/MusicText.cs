using Pley.Tv.Core.Player;

namespace Pley.Tv.Core.Ui;

/// <summary>What the music pages and Now Playing say (the Android app's strings_tv_music.xml).</summary>
public static class MusicText
{
    public const string Artist = "Artist";
    public const string Albums = "Albums";
    public const string Play = "Play";
    public const string Pause = "Pause";
    public const string Shuffle = "Shuffle";
    public const string ShuffleOff = "Stop shuffling";
    public const string NowPlaying = "Now playing";
    public const string NoAlbums = "No albums found.";
    public const string NoTracks = "No tracks found";
    public const string NothingToPlay = "Nothing to play: it has no tracks.";
    public const string ArtistNotFound = "Artist not found";
    public const string AlbumNotFound = "Album not found";
    public const string NotFoundBody = "It may have been removed, or this profile can't see it.";
    public const string ArtistFailed = "Couldn't load this artist";
    public const string AlbumFailed = "Couldn't load this album";
    public const string Retry = "Retry";
    public const string Back = "Back";

    // Now Playing.
    public const string Queue = "Up next";
    public const string Next = "Next";
    public const string Previous = "Previous";
    public const string Position = "Position in the track";
    public const string Loading = "Loading…";
    public const string NothingPlaying = "Nothing is playing";
    public const string NothingPlayingBody = "Play an album or an artist and it shows here.";

    /// <summary>"Disc 2".</summary>
    public static string Disc(int number) => $"Disc {number}";

    /// <summary>Why music doesn't play, in danger text under the byline.</summary>
    public static string Error(MusicError error) => error switch
    {
        MusicError.Offline => "Can't reach the server.",
        MusicError.SignedOut => "Sign in again to play.",
        MusicError.Stopped => "Playback was stopped by the server.",
        _ => "This track can't be played.",
    };
}
