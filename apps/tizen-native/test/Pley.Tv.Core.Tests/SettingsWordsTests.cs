using System;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Net;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>Settings, Who's watching?, Diagnostics, the rail and the shared formats in French and Spanish; English is the other tests'.</summary>
[Collection(LanguageShown.Name)]
public sealed class SettingsWordsTests : IDisposable
{
    static readonly TizenFacts Facts = new TizenFacts { Tizen = 8, Year = 2024, Model = "QE55S95D", OutputMode = "DOLBY_DIGITAL_PLUS" };

    const long Now = 1_750_000_000;

    public void Dispose() => Strings.Reset();

    [Fact]
    public void Settings_read_in_French_and_Spanish()
    {
        Strings.Use("fr");
        Assert.Equal("Paramètres", SettingsText.Title);
        Assert.Equal("Sous-titres et affichage", SettingsText.CategoryLabel(SettingsCategory.Display));
        Assert.Equal("Seulement pour les dialogues étrangers", SettingsText.ModeLabel("forced"));
        Assert.Equal("8 Mbit/s 1080p", SettingsText.QualityLabel("8m"));
        Assert.Equal("Se déconnecter ?", SettingsText.SignOutTitle);
        Assert.Equal("Cette TV sera déconnectée de 192.168.1.20:7539.", SettingsText.SignOutBody("http://192.168.1.20:7539"));
        Assert.Equal("DTS-HD et DTS:X", SettingsText.FormatLabel("dts-hd"));
        Strings.Use("es");
        Assert.Equal("Ajustes", SettingsText.Title);
        Assert.Equal("Muy grande", SettingsText.SizeLabel(Settings.SubtitleSizes[3]));
        Assert.Equal("¿Cerrar sesión?", SettingsText.SignOutTitle);
        Assert.Equal("Cancelar", SettingsText.Cancel);
    }

    [Fact]
    public void Diagnostics_read_in_French_and_Spanish()
    {
        Strings.Use("fr");
        var facts = DiagnosticsText.DeviceFacts(Facts);
        Assert.Contains(new Fact("Système", "Tizen 8.0 (modèles 2024)"), facts);
        Assert.Contains(new Fact("Niveau", "haut"), facts);
        Assert.Equal("Envoyé à l’ampli : ac3, eac3 (sortie audio DOLBY_DIGITAL_PLUS)", DiagnosticsText.Lines(Facts)[6]);
        Strings.Use("es");
        Assert.Equal("Diagnóstico", DiagnosticsText.Title);
        Assert.Equal("Flujos: HLS ts, fmp4", DiagnosticsText.Lines(Facts).Last());
    }

    [Fact]
    public void Whos_watching_and_its_refusals_read_in_French_and_Spanish()
    {
        Strings.Use("fr");
        Assert.Equal("Qui regarde ?", Strings.T("profiles.title"));
        Assert.Equal("Saisissez le code PIN du profil Anna", Strings.T("profiles.pin.title", ("name", "Anna")));
        Assert.Equal("Code PIN erroné.", Errors.AsSentence(ProfilesModel.Message(new OAuthError("invalid_grant", "wrong PIN", "wrong_pin"))));
        Assert.Equal("Impossible de joindre votre serveur : rien ne répond à cette adresse.",
            Errors.AsSentence(Strings.T("profiles.errors.unreachable", ("reason", Errors.Describe(new NetworkException(NetworkFailure.NothingAnswers))))));
        Strings.Use("es");
        Assert.Equal("¿Quién está viendo?", Strings.T("profiles.title"));
        Assert.Equal("No puedes cambiar a este perfil.", Errors.AsSentence(ProfilesModel.Message(new OAuthError("invalid_grant", null, "not_allowed"))));
    }

    [Fact]
    public void The_rail_and_the_exit_read_in_French_and_Spanish()
    {
        Strings.Use("fr");
        Assert.Equal("Accueil", new RailItem("home", RailSlot.Home, new PageKey.Home()).Label);
        Assert.Equal("TV en direct", Rail.LiveTvItem.Label);
        Assert.Equal("Quitter Pley ?", Strings.T("shell.exit.title"));
        Strings.Use("es");
        Assert.Equal("Ajustes", Rail.SettingsItem.Label);
        // A library's name is the server's, in whatever language it has it.
        Assert.Equal("Films", new RailItem("library:3", RailSlot.Library, new PageKey.Library(3), "Films", "movies").Label);
        Assert.Equal("TV en vivo llegará pronto a la aplicación nativa.", Strings.T("shell.comesNext", ("page", Rail.LiveTvItem.Label)));
    }

    [Fact]
    public void Times_and_counts_read_in_French_and_Spanish()
    {
        Strings.Use("fr");
        Assert.Equal("1 h 52 min", Format.Duration(6_720_000));
        Assert.Equal("Il reste 26 min", Format.TimeLeft(1_920_000, 3_480_000));
        Assert.Equal("1 saison", Format.CardSubtitle(new ItemSummary { Kind = "show", ChildCount = 1 }));
        Assert.Equal("S2 · É5", Format.EpisodeLabel(2, 5));
        Assert.Equal("Épisodes spéciaux · Épisode 3", Format.EpisodeLongLabel(0, 3));
        Assert.Equal("il y a 3 heures", Format.Relative(Now - 3 * 3600, Now));
        Assert.Equal("la semaine dernière", Format.Relative(Now - 8 * 86_400, Now));
        Strings.Use("es");
        Assert.Equal("29 s", Format.Duration(29_000));
        Assert.Equal("10 episodios", Format.CardSubtitle(new ItemSummary { Kind = "season", LeafCount = 10 }));
        Assert.Equal("T2 · E5", Format.EpisodeLabel(2, 5));
        Assert.Equal("hace 1 minuto", Format.Relative(Now - 60, Now));
        Assert.Equal("ayer", Format.Relative(Now - 86_400, Now));
        Assert.Equal("1 hora y 12 minutos", Format.SpokenDuration(4_320_000));
    }
}
