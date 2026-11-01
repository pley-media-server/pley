using System.Globalization;
using System.Linq;
using System.Text;

namespace Pley.Tv.Core.Net;

/// <summary>
/// How the app's requests introduce themselves: <c>pleyTizen/0.1.0 (SMART-TV; Tizen 9.0; QE55Q80D)</c>.
/// The server names the device after it in Account → Devices and Apps: "SMART-TV" with "Tizen"
/// is how it knows a Samsung TV (as from the web app's engine), and the Tizen version is what
/// an app's row shows (apps/server/src/accounts.rs, device_name; api/oauth.rs, grant_device).
/// The model tells two TVs of a household apart, as the Android app's device name does.
/// </summary>
public static class UserAgents
{
    /// <summary>Long enough for a TV's model and most names people give.</summary>
    const int MaxName = 40;

    public static string Of(string version, string? tizenVersion, string? model)
    {
        var name = model == null ? "" : Cleaned(model);
        var tizen = tizenVersion == null ? "" : Cleaned(tizenVersion);
        return $"pleyTizen/{Cleaned(version)} (SMART-TV; Tizen {(tizen.Length > 0 ? tizen : "?")}{(name.Length > 0 ? "; " + name : "")})";
    }

    /// <summary>
    /// <paramref name="name"/> as a header carries it: printable ASCII (accents dropped, typographic
    /// quotes and dashes made plain, the rest left out), without the brackets and semicolons the
    /// user agent is made of. The Android app's <c>UserAgent.cleaned</c>.
    /// </summary>
    internal static string Cleaned(string name)
    {
        var b = new StringBuilder();
        foreach (var c in name.Normalize(NormalizationForm.FormD))
        {
            if (CharUnicodeInfo.GetUnicodeCategory(c) == UnicodeCategory.NonSpacingMark) continue;
            b.Append(c switch
            {
                '‘' or '’' or '‛' or '′' => '\'',
                '“' or '”' or '″' => '"',
                '–' or '—' or '‐' or '‑' => '-',
                >= ' ' and <= '~' => "();\\".IndexOf(c) >= 0 ? ' ' : c,
                _ => ' ',
            });
        }
        var words = string.Join(" ", b.ToString().Split(' ').Where(w => w.Length > 0));
        return (words.Length > MaxName ? words.Substring(0, MaxName) : words).Trim();
    }
}
