using System;
using System.Globalization;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Live;
using Pley.Tv.Core.Requests;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>Live TV, requests and Discover in French and Spanish; English is the other tests'.</summary>
[Collection(LanguageShown.Name)]
public sealed class LiveAndRequestsWordsTests : IDisposable
{
    const long Now = 1_761_825_600, Day = 86_400; // 30 Oct 2025, 12:00 UTC
    readonly RequestText texts = new RequestText(() => Now, TimeZoneInfo.Utc);

    public void Dispose() => Strings.Reset();

    [Fact]
    public void Live_TV_reads_in_French()
    {
        Strings.Use("fr");
        Assert.Equal("TV en direct", LiveText.Title);
        Assert.Equal("Il reste 12 min", LiveText.TimeLeft(FakeLive.Program(1, 0, 720, ""), 0));
        Assert.Equal("Il reste 1 h 5 min", LiveText.TimeLeft(FakeLive.Program(1, 0, 3900, ""), 0));
        Assert.Equal("Sa seule connexion diffuse :", LiveText.ConnectionsWatching(1));
        Assert.Equal("Ses 2 connexions diffusent :", LiveText.ConnectionsWatching(2));
        Assert.Equal("3 personnes", LiveText.Holders(new BusyConnection { People = 3 }));
        Assert.Equal("TF1 est indisponible", LiveText.TroubleTitle(new LiveTrouble(LiveTroubleKind.Unavailable, ""), "TF1"));
        Assert.Equal("Cette chaîne n’est plus regardée.", LiveTrouble.Stopped(null).Message);
        Assert.Equal("S2 É14", new LiveProgram { Season = 2, Episode = 14 }.EpisodeLabel);
        var clock = new LiveClock(CultureInfo.GetCultureInfo("fr-FR"), TimeZoneInfo.Utc);
        Assert.Equal("Aujourd’hui", clock.Day(Now + 3600, Now));
        Assert.Equal("À suivre 12:00 · Le Journal", clock.Next(FakeLive.Program(1, Now, Now + 1, "Le Journal")));
    }

    [Fact]
    public void Live_TV_reads_in_Spanish()
    {
        Strings.Use("es");
        Assert.Equal("TV en vivo", LiveText.Title);
        Assert.Equal("Queda 1 min", LiveText.TimeLeft(FakeLive.Program(1, 0, 10, ""), 0));
        Assert.Equal("Quedan 12 min", LiveText.TimeLeft(FakeLive.Program(1, 0, 720, ""), 0));
        Assert.Equal("3 personas", LiveText.Holders(new BusyConnection { People = 3 }));
        Assert.Equal("No hay canal 9", LiveText.NoChannel("9"));
        Assert.Equal("T2 E14", new LiveProgram { Season = 2, Episode = 14 }.EpisodeLabel);
        // The server's words go in as they are.
        Assert.Equal("No se guardó el favorito: No.", LiveText.FavoriteFailed(new ApiException(500, "internal", "no")));
    }

    [Fact]
    public void Requests_read_in_French()
    {
        Strings.Use("fr");
        Assert.Equal("Saisons 1–2 et épisodes spéciaux", RequestText.Seasons(new[] { 0, 1, 2 }));
        Assert.Equal("Téléchargement 42 % · dans 20 min", texts.Pill(new StagePill.Downloading(0.42f, Now + 20 * 60, DownloadState.Downloading)));
        Assert.Equal("dans 3 heures", texts.Until(Now + 3 * 3_600));
        Assert.Equal($"aujourd’hui à {new DateTime(2025, 10, 30, 21, 0, 0).ToString("t", Strings.Culture)}", texts.DayOf(Now + 9 * 3_600));
        // The day before the month, in French words.
        Assert.Equal($"Sortie : {new DateTime(2025, 11, 4).ToString("d MMM", Strings.Culture)}", texts.Pill(new StagePill.Waiting(false, 1_762_257_600)));
        Assert.Equal(new[] { "Vous pouvez encore demander 2 films et 5 saisons.", "Limites : 3 films et 10 saisons sur 7 jours." },
            texts.Allowance(new Allowance(2, 5, 3, 10, null, null)));
        Assert.Equal("Saison 2 de « Severance » est prête à regarder.",
            RequestText.Notice(new RequestNotice.Ready(2, 47, "Severance", TitleKind.Show, 201, new[] { 2 }, false)));
        Assert.Equal("Demander 1 saison", RequestText.PickerButton(new SeasonPicker(Array.Empty<PickerSeason>(), new[] { 3 }, null)));
        Assert.Equal("1 épisode sur 10 dans la bibliothèque",
            RequestText.SeasonState(new DiscoverSeason { Number = 1, Episodes = 10, InLibrary = 1, State = SeasonState.Partial }));
        Assert.Equal("3 demandes vous attendent, 1 n’a pas pu être envoyée", RequestText.Count(new RequestsEntry(Requests: true, Manage: true, Badge: 3, Failed: 1)));
    }

    [Fact]
    public void Requests_read_in_Spanish()
    {
        Strings.Use("es");
        Assert.Equal("Temporada 3 y especiales", RequestText.Seasons(new[] { 3, 0 }));
        Assert.Equal("mañana", texts.Until(Now + Day));
        Assert.Equal("en 12 días", texts.Until(Now + 12 * Day));
        Assert.Equal($"mañana a las {new DateTime(2025, 10, 31, 9, 0, 0).ToString("t", Strings.Culture)}", texts.DayOf(Now + 21 * 3_600));
        Assert.Equal("Rechazado: “No es para nosotros”. Podrás volver a solicitarlo en 12 días.",
            texts.Declined(new MainAction.Declined("No es para nosotros", Now + 12 * Day)));
        var request = new MediaRequest { Id = 60, Kind = TitleKind.Movie, TmdbId = 1, Title = "Arrival", Status = RequestStatus.Pending };
        Assert.Equal("Solicitaste “Arrival”: un administrador decidirá.", RequestText.Asked(new Asked(request, true), "Arrival"));
        Assert.Equal("1 de tus solicitudes tiene novedades", RequestText.Notice(new RequestNotice.News(10, 1)));
        Assert.Equal("1 episodio", RequestText.SeasonState(new DiscoverSeason { Number = 3, Episodes = 1, State = SeasonState.Missing }));
        Assert.Equal("Serie", RequestText.Kind(TitleKind.Show));
    }
}
