using System;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Player;

/// <summary>
/// Where one seek preview is: its sheet's address and the part of the sheet it takes, in pixels.
/// The screen shows the sheet (a JPEG, fetched with the token like any picture) cropped to it.
/// </summary>
/// <param name="SheetWidth">The sheet's whole size, for a crop given as a share of it.</param>
public sealed record TrickplayFrame(int Index, int Sheet, string SheetUrl, int Left, int Top, int Width, int Height, int SheetWidth, int SheetHeight);

/// <summary>The seek previews' arithmetic (docs/API.md, GET /api/files/{id}/trickplay; the Android app's TrickplayTiles).</summary>
public static class Trickplay
{
    /// <summary>While the server makes them it is asked again this often…</summary>
    public const long AgainMs = 20_000;

    /// <summary>…for this long at most.</summary>
    public const long GiveUpMs = 10 * 60_000;

    /// <summary>Previews are looked up for every 2 s of the seek's target at most.</summary>
    public const long StepMs = 2_000;

    /// <summary>The sheet thumbnail <paramref name="k"/> is on.</summary>
    public static int SheetOf(TrickplayInfo info, int k) => k / Math.Max(1, info.Columns * info.Rows);

    /// <summary>Its column and row on that sheet.</summary>
    public static (int Column, int Row) CellOf(TrickplayInfo info, int k) => (k % Math.Max(1, info.Columns), k / Math.Max(1, info.Columns) % Math.Max(1, info.Rows));

    /// <summary>The thumbnail for <paramref name="positionMs"/>: k × interval, clamped to the last one.</summary>
    public static int ThumbnailAt(TrickplayInfo info, long positionMs) =>
        info.IntervalMs <= 0 || info.Count <= 0 ? 0 : (int)Math.Max(0, Math.Min(info.Count - 1, positionMs / info.IntervalMs));

    /// <summary>
    /// The preview for <paramref name="positionMs"/> of file <paramref name="fileId"/>, its sheet's
    /// address made by <paramref name="sheetUrl"/> (file, sheet, version: the API's
    /// <see cref="Api.Api.TrickplaySheetUrl"/>); null without previews.
    /// </summary>
    public static TrickplayFrame? At(TrickplayInfo? info, long fileId, long positionMs, Func<long, int, string, string> sheetUrl)
    {
        if (info == null || info.Count <= 0 || info.Width <= 0 || info.Height <= 0) return null;
        var k = ThumbnailAt(info, positionMs);
        var sheet = SheetOf(info, k);
        var (column, row) = CellOf(info, k);
        return new TrickplayFrame(k, sheet, sheetUrl(fileId, sheet, info.Id), column * info.Width, row * info.Height, info.Width, info.Height,
            info.Width * info.Columns, info.Height * info.Rows);
    }
}
