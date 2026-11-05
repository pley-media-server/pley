using System;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>Libraries, the watch history, playlists and music in French and Spanish; English is the other tests'.</summary>
[Collection(LanguageShown.Name)]
public sealed class LibraryWordsTests : IDisposable
{
    public void Dispose() => Strings.Reset();

    [Fact]
    public void Libraries_history_playlists_and_music_read_in_French()
    {
        Strings.Use("fr");
        var group = Strings.Culture.NumberFormat.NumberGroupSeparator;
        Assert.Equal("Trier par", LibraryText.SortBy);
        Assert.Equal("Dernière lecture", LibraryText.SortLabel(LibrarySort.LastPlayed));
        Assert.Equal("Toutes", LibraryText.NoDynamicRangeChosen);
        Assert.Equal($"1{group}500 films", LibraryText.Count("movies", 1_500));
        Assert.Equal($"23 sur 1{group}500 films", LibraryText.CountOf("movies", 23, 1_500));
        Assert.Equal("1 sur 1 artiste", LibraryText.CountOf("music", 1, 1));
        Assert.Equal($"1{group}000{group}000 de films", LibraryText.Count("movies", 1_000_000));
        // French counts 0 as one.
        Assert.Equal("0 lecture", HistoryText.Count(0));
        Assert.Equal("40 lectures", HistoryText.Count(40));
        Assert.Equal("Supprimer la playlist ?", PlaylistText.DeleteTitle);
        Assert.Equal("Retiré de la playlist : « Half Loop »", PlaylistText.Removed("Half Loop"));
        Assert.Equal("19 éléments ajoutés à Road trip", PlaylistText.Added(new AddEvent.Added("Road trip", 19)));
        Assert.Equal(new[] { "Genre : Comedy", "Non vu", "Les plus récents d’abord" }, SmartFilter.Describe("genre=Comedy&unwatched=1&sort=year%3Adesc"));
        Assert.Equal("Disque 2", MusicText.Disc(2));
        Assert.Equal("Impossible de joindre le serveur.", MusicText.Error(MusicError.Offline));
    }

    [Fact]
    public void Libraries_history_playlists_and_music_read_in_Spanish()
    {
        Strings.Use("es");
        Assert.Equal("Ordenar bibliotecas", LibraryText.SortLibraries);
        Assert.Equal("1 serie", LibraryText.Count("shows", 1));
        Assert.Equal("3 de 120 artistas", LibraryText.CountOf("music", 3, 120));
        Assert.Equal("0 reproducciones", HistoryText.Count(0));
        Assert.Equal("¿Eliminar la lista de reproducción?", PlaylistText.DeleteTitle);
        Assert.Equal("Se quitó “Half Loop”", PlaylistText.Removed("Half Loop"));
        Assert.Equal("Añadido a Road trip", PlaylistText.Added(new AddEvent.Added("Road trip", 1)));
        Assert.Equal(new[] { "Después de 2019", "Añadidos recientemente primero", "20 como máximo" },
            SmartFilter.Describe("year%3E%3E=2019&sort=movie.addedAt:desc&includeGuids=1&limit=20"));
        Assert.Equal("1 pista", MusicText.TrackCount(1));
        Assert.Equal("Desactivar aleatorio", MusicText.ShuffleOff);
    }
}
