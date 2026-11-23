using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Kit;

/// <summary>
/// Lucide's icons (shared/res/icons, made by scripts/icons.mjs), drawn white and tinted: the
/// icons the Android app and the web TV app draw.
/// </summary>
static class Icons
{
    /// <summary>Where an icon's file is, to change an icon in place.</summary>
    public static string Url(string name) => $"{(global::Tizen.Applications.Application.Current.DirectoryInfo.SharedResource)}icons/{name}.svg";

    public static ImageView Icon(string name, float sizeDp, Color tint)
    {
        var px = (int)Ui.Dp(sizeDp);
        return new ImageView
        {
            ResourceUrl = Url(name),
            Size = new Size(px, px),
            DesiredWidth = px,
            DesiredHeight = px,
            Color = tint,
        };
    }

    /// <summary>
    /// Pley's mark in white for a placeholder whose picture won't come (Placeholder.MarkLayers),
    /// <paramref name="sideDp"/> square at (<paramref name="xDp"/>, <paramref name="yDp"/>). NUI
    /// draws the SVG off the UI thread, at the size it shows; a view that shows it is made once and
    /// shown or hidden after.
    /// </summary>
    public static ImageView PlaceholderMark(float sideDp, float xDp, float yDp)
    {
        var px = (int)Ui.Dp(sideDp);
        return new ImageView
        {
            ResourceUrl = $"{(global::Tizen.Applications.Application.Current.DirectoryInfo.SharedResource)}pley-mark-placeholder.svg",
            Size = new Size(px, px),
            Position = new Position(Ui.Dp(xDp), Ui.Dp(yDp)),
            DesiredWidth = px,
            DesiredHeight = px,
        };
    }
}
