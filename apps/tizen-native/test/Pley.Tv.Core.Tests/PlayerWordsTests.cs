using System;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>The player and the videos on a drive in French and Spanish; English is the other tests'.</summary>
[Collection(LanguageShown.Name)]
public sealed class PlayerWordsTests : IDisposable
{
    public void Dispose() => Strings.Reset();

    [Fact]
    public void The_players_controls_and_panels_read_in_French_and_Spanish()
    {
        Strings.Use("fr");
        Assert.Equal("Sous-titres", PlayerText.Subtitles);
        Assert.Equal("Minuteur de veille", PlayerPanels.Build(PlayerPanelKind.Sleep, new PlayerState(), 0, new Settings()).Title);
        Assert.Equal("Sous-titres : Anglais (SRT)", PlayerText.SubtitlesToast("Anglais (SRT)"));
        Assert.Equal("À suivre · S1 · É2", SessionText.UpNext("S1 · É2"));
        Assert.Equal("Lecture depuis un autre appareil", Casts.Banner(" "));
        Assert.Equal("8 Mbit/s 720p", SessionText.Quality(Qualities.ById("8m"), 720));
        Strings.Use("es");
        Assert.Equal("Subtítulos", PlayerText.Subtitles);
        Assert.Equal("Automática · 8 Mbps 1080p", SessionText.Automatic(SessionText.Quality(Qualities.ById("8m"))));
        Assert.Equal("Reproduciendo desde Salón", Casts.Banner("Salón"));
    }

    [Fact]
    public void Counts_and_numbers_are_the_languages_own()
    {
        Strings.Use("fr");
        Assert.Equal("Il reste 42 min", SleepText.Summary(new SleepState(new SleepChoice.Minutes(45), MinutesLeft: 42)));
        Assert.Equal("1 heure 2 minutes sur 0 seconde", PlayerText.Position(3_720_000, 0));
        Assert.Equal("+0,5 s", SubtitleSync.Label(500));
        Assert.Equal("1,25×", PlaybackSpeeds.Label(1.25));
        Assert.Equal("−1,5 s", AudioDelays.Label(-1_500));
        Strings.Use("es");
        Assert.Equal("Queda 1 min", SleepText.Summary(new SleepState(new SleepChoice.Minutes(5), MinutesLeft: 1)));
        Assert.Equal("Quedan 3 min", SleepText.Summary(new SleepState(new SleepChoice.Minutes(5), MinutesLeft: 3)));
        Assert.Equal("2 horas 1 segundo", PlayerText.Spoken(7_201_000));
        Assert.Equal("+30 s", PlayerText.Badge(30_000));
    }

    [Fact]
    public void Chapters_name_themselves_in_English_or_in_the_language_shown()
    {
        Strings.Use("fr");
        Assert.Equal("Chapitre 2", PlayerText.ChapterTitle(new ChapterMark("00:06:19.212", 0, 1, null), 1));
        Assert.Equal("Chapitre 3 · Le casse", PlayerText.ChapterLine(new ChapterMark("Le casse", 0, 1, null), 2));
        Assert.Equal("Chapitre 3", PlayerText.ChapterLine(new ChapterMark("Chapitre 3", 0, 1, null), 2));
        Assert.Equal("Chapter 3", PlayerText.ChapterLine(new ChapterMark("Chapter 3", 0, 1, null), 2));
    }

    [Fact]
    public void Playback_errors_read_in_French_and_Spanish()
    {
        Strings.Use("fr");
        Assert.Equal("La lecture a échoué (ERROR_CODE_DECODING_FAILED).", SessionText.Failure(Failure.Engine, "ERROR_CODE_DECODING_FAILED: boom"));
        Assert.Equal("Pas de réponse à temps. Vérifiez que le serveur est allumé et que cet appareil est sur son réseau.", SessionText.Unreachable("pas de réponse à temps"));
        Assert.Equal("Impossible de lire cet élément", PlayerPhase.Error("x", canRetry: false).Title);
        Strings.Use("es");
        Assert.Equal("El administrador del servidor detuvo la reproducción.\nNos vemos mañana", SessionText.Stopped(" Nos vemos mañana ", byAdmin: true));
        Assert.Equal("Este servidor es demasiado antiguo para reproducir en esta TV: actualízalo.", Strings.T("player.errors.serverTooOld"));
    }

    [Fact]
    public void The_drive_page_reads_in_French_and_Spanish()
    {
        Strings.Use("fr");
        Assert.Equal("12 Go libres sur 64 Go", Strings.T("storage.space", ("free", "12 Go"), ("total", "64 Go")));
        Assert.Equal("MKV · Reprise à 42:10", Strings.T("storage.resumesAt", ("kind", "MKV"), ("time", PlayerText.Time(2_530_000))));
        Strings.Use("es");
        Assert.Equal("No hay ningún dispositivo USB", Strings.T("storage.noDrive.title"));
        Assert.StartsWith("Esta TV no reproduce este archivo:", SessionText.LocalStuck, StringComparison.Ordinal);
    }
}
