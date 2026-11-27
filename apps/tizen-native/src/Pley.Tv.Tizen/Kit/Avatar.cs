using Pley.Tv.Core.Ui;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Kit;

/// <summary>
/// A profile's picture, or its initial on its color (<see cref="ProfileAvatar"/>; the web's
/// Avatar.tsx): every avatar the app draws. The circle and its initial show while the picture
/// loads and stay when it doesn't come or can't be drawn, so an avatar is never an empty circle.
/// </summary>
sealed class AvatarView : View
{
    public AvatarView(string name, string? colorHex, string? pictureUrl, float sizeDp, Images? images)
    {
        var px = Ui.Dp(sizeDp);
        Size = new Size(px, px);
        CornerRadius = px / 2;
        ClippingMode = ClippingModeType.ClipChildren;
        var look = ProfileAvatar.Of(name, colorHex);
        BackgroundColor = Rgb(look.Color);
        var initial = new TextLabel(look.Initial)
        {
            Size = new Size(px, px),
            PixelSize = px * 0.44f,
            TextColor = Rgb(look.Ink),
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
                var picture = new ImageView { Size = new Size(px, px), DesiredWidth = (int)px, DesiredHeight = (int)px, FittingMode = FittingModeType.ScaleToFill };
                // Over the initial, which goes once the picture is drawn; one NUI can't decode goes instead.
                picture.ResourceReady += (_, _) =>
                {
                    if (Disposed) return;
                    if (picture.LoadingStatus == ImageView.LoadingStatusType.Ready) initial.Hide();
                    else picture.Unparent();
                };
                picture.ResourceUrl = file;
                Add(picture);
            });
        }
    }

    public static Color Rgb(uint rgb) => new Color((rgb >> 16 & 0xff) / 255f, (rgb >> 8 & 0xff) / 255f, (rgb & 0xff) / 255f, 1);
}
