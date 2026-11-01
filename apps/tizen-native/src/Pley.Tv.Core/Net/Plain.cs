using System.Globalization;
using System.Linq;
using System.Text;

namespace Pley.Tv.Core.Net;

/// <summary>Text from a server or a sign-in page, made safe to show (the Android app's <c>plain</c>).</summary>
public static class Plain
{
    /// <summary>
    /// <paramref name="text"/> without control or format characters (which could reorder or hide
    /// what shows), trimmed, and at most <paramref name="max"/> characters long.
    /// </summary>
    public static string Of(string? text, int max = 200)
    {
        if (string.IsNullOrEmpty(text)) return "";
        var b = new StringBuilder(text!.Length);
        foreach (var c in text)
        {
            if (char.IsControl(c) || CharUnicodeInfo.GetUnicodeCategory(c) == UnicodeCategory.Format) continue;
            b.Append(c);
        }
        var clean = b.ToString().Trim();
        return clean.Length > max ? clean.Substring(0, max) : clean;
    }

    /// <summary>The first line of <paramref name="text"/> that isn't blank, made safe as <see cref="Of"/> does.</summary>
    public static string FirstLine(string? text, int max) =>
        Of((text ?? "").Split(new[] { "\r\n", "\r", "\n" }, System.StringSplitOptions.None).FirstOrDefault(l => l.Trim().Length > 0), max);
}
