using System.Collections.Generic;

namespace Pley.Tv.Core.Local;

/// <summary>
/// A menu button: its number, where it is on the picture (720×480 or 720×576), which button
/// Up, Down, Left and Right select, its colour scheme, whether selecting it acts at once, and the
/// command it runs when pressed.
/// </summary>
public sealed record DvdButton(int Number, int X1, int Y1, int X2, int Y2, int Up, int Down, int Left, int Right, int Scheme, bool AutoAction, byte[] Command);

/// <summary>
/// A highlight colour scheme: for each of the subpicture's four pixel codes (background, pattern,
/// emphasis 1, emphasis 2), the palette entry and contrast (0-15) a button takes when selected,
/// and when pressed.
/// </summary>
public sealed record DvdColourScheme(int[] SelectedColors, int[] SelectedContrast, int[] ActionColors, int[] ActionContrast);

/// <summary>A VOBU's highlight information: its buttons, its colour schemes, and the buttons to select or press first.</summary>
public sealed record DvdHighlight(IReadOnlyList<DvdButton> Buttons, IReadOnlyList<DvdColourScheme> Schemes, int ForcedSelect, int ForcedAction);

/// <summary>
/// A menu's buttons, read from the PCI packet of a VOBU's navigation pack (its first sector:
/// private stream 2, substream 0): the highlight information's general part, three colour
/// schemes and up to 36 buttons, as the DVD-Video specification lays them out.
/// </summary>
public static class DvdButtons
{
    /// <summary>Where the PCI's data starts in a navigation pack's sector.</summary>
    const int Pci = 0x2d;

    /// <summary>The highlight in the navigation pack <paramref name="sector"/> (at <paramref name="at"/>); null when it has none.</summary>
    public static DvdHighlight? Read(byte[] sector, int at = 0)
    {
        if (at + 0x400 > sector.Length) return null;
        // The pack header, the system header, then the PCI packet.
        if (sector[at + 0x26] != 0 || sector[at + 0x27] != 0 || sector[at + 0x28] != 1 || sector[at + 0x29] != 0xbf || sector[at + 0x2c] != 0) return null;
        var p = at + Pci;
        var status = ((sector[p + 0x60] << 8) | sector[p + 0x61]) & 3;
        if (status == 0) return null;
        int count = sector[p + 0x71];
        var schemes = new List<DvdColourScheme>();
        for (var s = 0; s < 3; s++)
        {
            var c = p + 0x76 + s * 8;
            // Each pair of nibbles is emphasis 2 / emphasis 1, then pattern / background: as codes 3, 2, 1, 0.
            int[] Four(int a, int b) => new[] { sector[b] & 15, sector[b] >> 4, sector[a] & 15, sector[a] >> 4 };
            schemes.Add(new DvdColourScheme(Four(c, c + 1), Four(c + 2, c + 3), Four(c + 4, c + 5), Four(c + 6, c + 7)));
        }
        var buttons = new List<DvdButton>();
        for (var i = 0; i < count && i < 36; i++)
        {
            var b = p + 0x8e + i * 18;
            var x1 = ((sector[b] & 0x3f) << 4) | (sector[b + 1] >> 4);
            var x2 = ((sector[b + 1] & 3) << 8) | sector[b + 2];
            var y1 = ((sector[b + 3] & 0x3f) << 4) | (sector[b + 4] >> 4);
            var y2 = ((sector[b + 4] & 3) << 8) | sector[b + 5];
            var command = new byte[8];
            System.Array.Copy(sector, b + 10, command, 0, 8);
            buttons.Add(new DvdButton(i + 1, x1, y1, x2, y2, sector[b + 6] & 0x3f, sector[b + 7] & 0x3f, sector[b + 8] & 0x3f, sector[b + 9] & 0x3f,
                sector[b] >> 6, (sector[b + 3] >> 6) != 0, command));
        }
        return new DvdHighlight(buttons, schemes, sector[p + 0x74], sector[p + 0x75]);
    }
}
