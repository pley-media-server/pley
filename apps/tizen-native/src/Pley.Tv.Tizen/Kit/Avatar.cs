using Pley.Tv.Core.Ui;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Kit;

/// <summary>A profile's picture, or its initial on its colour (the web's Avatar.tsx; the Android app's ProfileAvatar).</summary>
sealed class AvatarView : View
{
    public AvatarView(string name, string? colorHex, string? pictureUrl, float sizeDp, Images? images)
    {
        var px = Ui.Dp(sizeDp);
        Size = new Size(px, px);
        CornerRadius = px / 2;
        ClippingMode = ClippingModeType.ClipChildren;
        var rgb = ProfileColors.Of(name, colorHex);
        BackgroundColor = Rgb(rgb);
        var trimmed = name.Trim();
        var initial = new TextLabel(trimmed.Length > 0 ? char.ToUpperInvariant(trimmed[0]).ToString() : "?")
        {
            Size = new Size(px, px),
            PixelSize = px * 0.44f,
            TextColor = ProfileColors.DarkTextOn(rgb) ? PleyColors.Bg : Color.White,
            FontFamily = PleyType.Roboto,
            FontStyle = Text.Weight(PleyType.Bold),
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
        };
        Add(initial);
        if (pictureUrl != null && images != null)
        {
            images.Load(new[] { pictureUrl }, sizeDp, file =>
            {
                if (file == null || Disposed) return;
                var picture = new ImageView { ResourceUrl = file, Size = new Size(px, px), DesiredWidth = (int)px, DesiredHeight = (int)px, FittingMode = FittingModeType.ScaleToFill };
                Add(picture);
                initial.Hide();
            });
        }
    }

    public static Color Rgb(uint rgb) => new Color((rgb >> 16 & 0xff) / 255f, (rgb >> 8 & 0xff) / 255f, (rgb & 0xff) / 255f, 1);
}
