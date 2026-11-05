using System;
using System.Globalization;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>Home, item pages, people, collections and search in French and Spanish; English is the other tests'.</summary>
[Collection(LanguageShown.Name)]
public sealed class ItemWordsTests : IDisposable
{
    public void Dispose() => Strings.Reset();

    static ItemPage Page(ItemDetail detail) => ItemPages.Page(new PageInput(detail, "https://pley.example.com"));

    static readonly ItemDetail Episode = new ItemDetail
    {
        Id = 21, LibraryId = 2, Kind = "episode", Title = "Glasshouse", Index = 5, ParentIndex = 2, GrandparentId = 10, GrandparentTitle = "Driftline",
        Studio = "HBO", ReleaseDate = "2024-03-12",
        Crew = new[] { new Credit { Id = 7, Name = "Lena Okafor", Role = "director" }, new Credit { Id = 9, Name = "Ana Ruiz", Role = "director" } },
        Cast = new[] { new Credit { Id = 8, Name = "Idris Vale", Role = "actor", Character = "Tom" } },
    };

    [Fact]
    public void An_item_page_reads_in_French()
    {
        Strings.Use("fr");
        var page = Page(Episode);
        Assert.Equal("Reprendre S1 · E3", ItemText.PrimaryLabel(new PrimaryAction(PrimaryKind.Resume, "S1 · E3")));
        Assert.Equal(new CreditsLine("Réalisation :", "Lena Okafor, Ana Ruiz", "Idris Vale"), ItemText.Credits(page));
        Assert.Equal("Invités et équipe", ItemText.CastRowTitle(page));
        Assert.Equal(new[] { ("Réalisation", "Lena Okafor, Ana Ruiz"), ("Chaîne", "HBO") }, ItemText.Facts(page.Details));
        Assert.Equal("et 1 autre", ItemText.AndMore(1));
        Assert.Equal("et 3 autres", ItemText.AndMore(3));
        Assert.Equal(new[] { "1 étoile", "2 étoiles" }, new[] { ItemText.StarsLabel(1), ItemText.StarsLabel(2) });
        Assert.Equal("Fait partie de Saltwind", ItemText.PartOf("Saltwind"));
        Assert.Equal("Diffusé le 12 mars 2024", ItemText.Aired(ItemFormat.LongDate("2024-03-12")));
        Assert.Equal("1 avr. 2023", ItemFormat.Date("2023-04-01"));
        Assert.Equal(new[] { "Épisodes spéciaux", "Saison 2" }, new[] { ItemFormat.SeasonTitle(null, 0), ItemFormat.SeasonTitle(null, 2) });
        // Quantities take the language's decimal mark and units; numbers that name things keep their point.
        Assert.Equal("62,0 Go", ItemFormat.Bytes(66_571_993_088));
        Assert.Equal("8,2 Mbit/s", ItemFormat.Bitrate(8_200_000));
        Assert.Equal("Main 10 · Niveau 5.1", ItemFormat.ProfileLevelLabel(new MediaStream { Kind = "video", Codec = "hevc", Profile = "Main 10", Level = 153 }));
        Assert.Equal("Rien à lire : cet élément n’a aucun fichier média.", ItemText.Toast(new ItemEvent.NothingToPlay()).Text);
    }

    [Fact]
    public void An_item_page_reads_in_Spanish()
    {
        Strings.Use("es", new[] { "es-ES" });
        Assert.Equal("Reproducir T1 · E3", ItemText.PrimaryLabel(new PrimaryAction(PrimaryKind.Play, "T1 · E3")));
        Assert.Equal("Emitido el 12 de marzo de 2024", ItemText.Aired(ItemFormat.LongDate("2024-03-12")));
        Assert.Equal("Automática (Inglés)", ItemText.AutomaticAudio(new TrackChoices(
            Array.Empty<TrackChoice>(), Array.Empty<TrackChoice>(), Array.Empty<TrackChoice>(), null, true, ItemFormat.LanguageName("eng"), null, null)));
        Assert.Equal(new[] { "Quitar de Seguir viendo", "Añadir a Mi lista" },
            new[] { ItemActions.Label(ItemAction.RemoveFromContinueWatching), ItemActions.Label(ItemAction.AddToMyList) });
        Assert.Equal("8,2 Mbps", ItemFormat.Bitrate(8_200_000));
        // A title naming the language in English, as files' titles mostly do, isn't repeated.
        Assert.Equal("Inglés (AAC Estéreo)", ItemFormat.StreamLabel(new MediaStream { Kind = "audio", Codec = "aac", Language = "eng", Title = "English", Channels = 2 }));
        Assert.Equal("Francés – SDH (SRT, Forzados)", ItemFormat.StreamLabel(new MediaStream { Kind = "subtitle", Codec = "subrip", Language = "fre", Title = "SDH", Forced = true }));
        var audio = MediaFacts.Audio(new MediaStream { Kind = "audio", Codec = "eac3", Channels = 2, ChannelLayout = "stereo", SampleRate = 44100, Language = "pt-BR", Default = true });
        Assert.Equal(new[] { ("Códec", "EAC3"), ("Canales", "Estéreo"), ("Muestreo", "44,1 kHz"), ("Idioma", "Portugués (BR)"), ("Atributos", "Por defecto") },
            audio.Select(f => (f.Label, f.Value)));
        // Mexico writes a decimal point.
        Strings.Use("es", new[] { "es-MX" });
        Assert.Equal("8.2 Mbps", ItemFormat.Bitrate(8_200_000));
    }

    [Fact]
    public void People_collections_home_and_search_read_in_the_language_shown()
    {
        Strings.Use("fr");
        Assert.Equal("dans le rôle de Mark (1 épisode)", PersonRules.RoleText(new[] { new PersonRole { Role = "actor", Character = "Mark", Episodes = 1 } }));
        Assert.Equal("Réalisation, Équipe technique", PersonRules.RoleText(new[] { new PersonRole { Role = "director" }, new PersonRole { Role = "gaffer" } }));
        Assert.Equal("Scénario", PersonText.RoleTitle("writer"));
        Assert.Equal("Par date de sortie", CollectionRules.SortNote("release"));
        Assert.Equal("Aucun résultat pour « nord »", SearchText.NoneTitle("nord"));
        Assert.Equal(new[] { "Personnes", "TV en direct" }, new[] { SearchText.GroupName(SearchGroupKind.People), SearchText.LiveTv });
        Assert.Equal("Analyse de vos bibliothèques…", Strings.T("home.states.scanning"));

        Strings.Use("es");
        var million = 1_000_000.ToString("N0", Strings.Culture);
        Assert.Equal($"Aparición especial ({million} episodios)", PersonRules.RoleText(new[] { new PersonRole { Role = "actor", Episodes = 1_000_000 } }));
        Assert.Equal("3 películas en tus bibliotecas", PersonText.InLibraries(Strings.Count("item.counts.movies", 3)));
        Assert.Equal("Esta colección aún está vacía.", CollectionText.Empty);
        Assert.Equal("TMDB", CollectionText.Tmdb);
        Assert.Equal("No hay resultados para «norte»", SearchText.NoneTitle("norte"));
    }
}

/// <summary>Language names (languages.json): each code ItemFormat knows has its name in every language.</summary>
public class LanguageNamesTests
{
    [Fact]
    public void Every_code_named_has_a_name_in_English_and_each_name_its_own_code()
    {
        var english = Strings.Of(Languages.English);
        var letters = Enumerable.Range('a', 26).Select(c => ((char)c).ToString()).ToList();
        var codes = letters.SelectMany(a => letters.Select(b => a + b)).ToList();
        codes.AddRange(codes.SelectMany(two => letters.Select(c => two + c)).ToList());
        var named = codes.Select(c => (Code: c, Key: ItemFormat.LanguageKey(c))).Where(c => c.Key != null).ToList();
        Assert.True(named.Count > 180, "the codes weren't found");
        Assert.All(named, c => Assert.True(english.ContainsKey(c.Key!), $"{c.Code}: {c.Key} isn't in languages.json"));
        foreach (var key in english.Keys.Where(k => k.StartsWith("languages.", StringComparison.Ordinal)))
            Assert.Equal(key, ItemFormat.LanguageKey(key.Substring("languages.".Length)));
    }

    [Fact]
    public void A_language_is_named_in_the_language_asked_for()
    {
        Assert.Equal("Portuguese (BR)", ItemFormat.LanguageName("pt_BR"));
        Assert.Equal("Allemand", Strings.In("fr").T(ItemFormat.LanguageKey("deu")!));
        Assert.Equal("Alemán", Strings.In("es").T(ItemFormat.LanguageKey("de")!));
        Assert.Equal("XYZ", ItemFormat.LanguageName("xyz"));
    }
}
