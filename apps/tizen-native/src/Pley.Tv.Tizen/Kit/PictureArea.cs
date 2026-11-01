using System;

namespace Pley.Tv.Tizen.Kit;

/// <summary>
/// Where a film's picture goes on the screen, letterboxed: the TV's player is told to put it
/// there (its region), and the black bars go round the same rectangle, so the two never part
/// whatever the film's shape. Left to its own letterboxing, the TV placed a 2.39:1 film a pixel or
/// two from where the bars ended, and its menu showed through.
/// </summary>
static class PictureArea
{
    /// <summary>The picture's rectangle, in whole pixels of a screen <paramref name="screenWidth"/> × <paramref name="screenHeight"/>.</summary>
    public static (int X, int Y, int Width, int Height) Of(int videoWidth, int videoHeight, int screenWidth, int screenHeight)
    {
        if (videoWidth <= 0 || videoHeight <= 0) return (0, 0, screenWidth, screenHeight);
        var scale = Math.Min(screenWidth / (double)videoWidth, screenHeight / (double)videoHeight);
        var w = (int)Math.Round(videoWidth * scale);
        var h = (int)Math.Round(videoHeight * scale);
        return ((screenWidth - w) / 2, (screenHeight - h) / 2, w, h);
    }

    /// <summary>The screen's size in pixels, as the app draws it.</summary>
    public static (int Width, int Height) Screen => ((int)Math.Round(Ui.Dp(PleyTv.CanvasWidth)), (int)Math.Round(Ui.Dp(PleyTv.CanvasHeight)));
}
