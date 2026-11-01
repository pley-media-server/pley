using System;
using Net.Codecrete.QrCodeGenerator;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Kit;

/// <summary>QR codes (the Android app's QrCodeImage): the sign-in code's address, a page of the web app.</summary>
static class Qr
{
    const int Quiet = 4;

    /// <summary>
    /// Draws a QR code of <paramref name="text"/> in <paramref name="box"/> (a white square of
    /// <paramref name="sizePx"/>), black, with its 4-module quiet zone: each module a whole number
    /// of pixels, the code centred in what is left (tv-addendum §4.2). Each row's dark runs are one
    /// view. False, with nothing drawn, when the text is too long for a QR code.
    /// </summary>
    public static bool Draw(View box, string text, int sizePx)
    {
        QrCode qr;
        try
        {
            qr = QrCode.EncodeText(text, QrCode.Ecc.Medium);
        }
        catch (Exception)
        {
            return false;
        }
        var modules = qr.Size + Quiet * 2;
        var cell = Math.Max(1, sizePx / modules);
        var origin = (sizePx - cell * modules) / 2 + Quiet * cell;
        for (var y = 0; y < qr.Size; y++)
        {
            var x = 0;
            while (x < qr.Size)
            {
                if (!qr.GetModule(x, y))
                {
                    x++;
                    continue;
                }
                var start = x;
                while (x < qr.Size && qr.GetModule(x, y)) x++;
                box.Add(new View
                {
                    Position = new Position(origin + start * cell, origin + y * cell),
                    Size = new Size((x - start) * cell, cell),
                    BackgroundColor = Color.Black,
                });
            }
        }
        return true;
    }
}
