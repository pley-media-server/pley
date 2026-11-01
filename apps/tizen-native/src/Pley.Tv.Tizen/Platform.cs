using System.Globalization;
using System.Reflection;
using Pley.Tv.Core;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Player;
using Tizen.Applications;

namespace Pley.Tv.Tizen;

/// <summary>What the app keeps between launches, in the app's own preferences.</summary>
sealed class PreferenceStore : IKeyValue
{
    public string? Get(string key) => Preference.Contains(key) ? Preference.Get<string>(key) : null;

    public void Set(string key, string value)
    {
        // A write can hold the UI thread for 400 ms while the TV's storage is busy, and most of the
        // app's are of the value already there (the server, the household, at each launch).
        if (Get(key) == value) return;
        Preference.Set(key, value);
    }

    public void Remove(string key)
    {
        if (Preference.Contains(key)) Preference.Remove(key);
    }
}

/// <summary>The TV as the shared state sees it: the app's name, version and client, and the TV's model.</summary>
static class Platform
{
    /// <summary>The Samsung apps' sign-in client: the native app signs in as the web one does.</summary>
    const string ClientId = "https://pley-media-server.github.io/pley/oauth/tizen.json";

    public static AppPlatform Of() => new AppPlatform("tizen", ClientId, new PreferenceStore())
    {
        Version = Version(),
        OsVersion = Info("http://tizen.org/feature/platform.version"),
        Model = Info("http://tizen.org/system/model_name"),
        TokenVault = new KeyManagerVault(),
    };

    /// <summary>The version people see: "0.0.1", without the commit.</summary>
    public static string Version()
    {
        var info = typeof(Platform).Assembly.GetCustomAttribute<AssemblyInformationalVersionAttribute>()?.InformationalVersion ?? "?";
        var plus = info.IndexOf('+');
        return plus < 0 ? info : info.Substring(0, plus);
    }

    /// <summary>
    /// What this TV is, for the server's choice of how to send it, Diagnostics and Settings: its
    /// Tizen version (which tells the model year) and model, as the system says them. The rest is
    /// TizenFacts' own: a 4K HDR panel, and nothing known of the sound output, so nothing passed
    /// through but what Settings turns on.
    /// </summary>
    public static TizenFacts Facts()
    {
        var version = Info("http://tizen.org/feature/platform.version");
        var tizen = double.TryParse(version, NumberStyles.Float, CultureInfo.InvariantCulture, out var v) ? v : 9;
        return new TizenFacts { Tizen = tizen, Year = TizenFacts.YearOf(tizen), Model = Info("http://tizen.org/system/model_name") ?? "Samsung TV" };
    }

    static string? Info(string key) => global::Tizen.System.Information.TryGetValue(key, out string value) ? value : null;
}
