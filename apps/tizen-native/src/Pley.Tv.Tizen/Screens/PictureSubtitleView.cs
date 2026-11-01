using System;
using System.Collections.Generic;
using Pley.Tv.Core.Subtitles.Pictures;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// Picture subtitles (PGS, VobSub, DVB) over the video, as the web app's PictureSubtitles draws
/// them: each bitmap of the frame where its plane puts it, the plane (a Blu-ray's 1920×1080, a
/// DVD's 720×576) stretched over the whole screen. Each bitmap's RGBA bytes become a texture shown
/// by an image; those of the frame before are let go as the next one comes. Unlike text, pictures
/// stay where they are while the controls show, under them.
/// </summary>
sealed class PictureSubtitleView : View
{
    readonly List<(ImageView Image, ImageUrl Url, PixelData Pixels)> shown = new List<(ImageView, ImageUrl, PixelData)>();
    PictureFrame? frame;

    public PictureSubtitleView()
    {
        Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(PleyTv.CanvasHeight));
    }

    /// <summary>The frame showing now; null clears the screen.</summary>
    public void Show(PictureFrame? next)
    {
        if (Disposed || next == frame) return;
        frame = next;
        Clear();
        if (next == null || next.PlaneWidth <= 0 || next.PlaneHeight <= 0) return;
        var sx = SizeWidth / next.PlaneWidth;
        var sy = SizeHeight / next.PlaneHeight;
        foreach (var b in next.Bitmaps)
        {
            if (b.Width <= 0 || b.Height <= 0) continue;
            // PixelData copies the bytes: the decoder keeps its own for the frames around the playhead.
            var pixels = new PixelData(b.Rgba, (uint)b.Rgba.Length, (uint)b.Width, (uint)b.Height, PixelFormat.RGBA8888);
            var url = pixels.GenerateUrl();
            var image = new ImageView
            {
                ResourceUrl = url.ToString(),
                Position = new Position((float)Math.Round(b.X * sx), (float)Math.Round(b.Y * sy)),
                Size = new Size((float)Math.Round(b.Width * sx), (float)Math.Round(b.Height * sy)),
            };
            Add(image);
            shown.Add((image, url, pixels));
        }
    }

    void Clear()
    {
        foreach (var (image, url, pixels) in shown)
        {
            Remove(image);
            image.Dispose();
            url.Dispose();
            pixels.Dispose();
        }
        shown.Clear();
    }

    protected override void Dispose(DisposeTypes type)
    {
        if (Disposed) return;
        Clear();
        base.Dispose(type);
    }
}
