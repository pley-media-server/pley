using System;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// A page of Pley's web app, for a TV, which opens no browser (the Android app's TvWebDialog,
/// design spec §4.8): the address to go to on a phone or computer, and its QR code to scan. A
/// Surface2 card over the scrim: on the left, 300 dp wide and 10 apart, the title, the words, the
/// address in Title and AccentBright, and Close; on the right, 28 dp on, the QR code, 220 dp on a
/// white tile. Back or Close dismisses it; focus starts on Close, and is where it was once it goes.
/// </summary>
sealed class WebDialog : Sheet
{
    const float Padding = 28, ColumnWidth = 300, Gap = 10, QrSize = 220, QrPadding = 4;

    readonly Shell shell;
    readonly ButtonView close;

    /// <param name="server">The server's address, "http://192.168.1.20:7539".</param>
    /// <param name="path">The web app's page on it: "/account".</param>
    public WebDialog(Shell shell, string server, string path) : base("web", Axis.Row)
    {
        this.shell = shell;
        View.BackgroundColor = PleyColors.Scrim;
        var url = server.TrimEnd('/') + path;

        var column = new View();
        float y = 0;
        TextLabel Words(string text, TextStyle style, Color color, int maxLines)
        {
            var label = Text.Label(text, style, color, maxLines, ColumnWidth);
            label.VerticalAlignment = VerticalAlignment.Top;
            var line = Text.LineHeight(style);
            label.SizeHeight = Math.Min(Math.Max(label.GetHeightForWidth(Ui.Dp(ColumnWidth)), line), line * maxLines);
            label.PositionY = y;
            y += label.SizeHeight + Ui.Dp(Gap);
            column.Add(label);
            return label;
        }
        Words(SettingsText.WebTitle, PleyType.Title, PleyColors.Fg, 2);
        Words(SettingsText.WebBody, PleyType.Body, PleyColors.Muted, 5);
        Words(SettingsText.HostPort(url), PleyType.Title, PleyColors.AccentBright, 2);
        // An 8 dp spacer between the address and Close, with the column's gaps either side.
        y += Ui.Dp(8 + Gap);
        close = new ButtonView(SettingsText.Close, ButtonKind.Pill, Close);
        close.PositionY = y;
        column.Add(close);
        var columnHeight = y + Ui.Dp(PleyTv.ButtonHeight);

        var tile = QrSize + QrPadding * 2;
        var inner = Math.Max(columnHeight, Ui.Dp(tile));
        var width = Padding * 3 + ColumnWidth + tile;
        var card = new View
        {
            Size = new Size(Ui.Dp(width), inner + Ui.Dp(Padding * 2)),
            CornerRadius = Ui.Dp(PleyShape.DialogRadius),
            BackgroundColor = PleyColors.Surface2,
        };
        card.Position = new Position(Ui.Dp((PleyTv.CanvasWidth - width) / 2), (Ui.Dp(PleyTv.CanvasHeight) - card.SizeHeight) / 2);
        column.Size = new Size(Ui.Dp(ColumnWidth), columnHeight);
        column.Position = new Position(Ui.Dp(Padding), Ui.Dp(Padding) + (inner - columnHeight) / 2);
        card.Add(column);
        var qr = new View
        {
            Size = new Size(Ui.Dp(tile), Ui.Dp(tile)),
            Position = new Position(Ui.Dp(Padding * 2 + ColumnWidth), Ui.Dp(Padding) + (inner - Ui.Dp(tile)) / 2),
            CornerRadius = Ui.Dp(PleyShape.DialogRadius),
            BackgroundColor = Color.White,
        };
        var code = new View { Size = new Size(Ui.Dp(QrSize), Ui.Dp(QrSize)), Position = new Position(Ui.Dp(QrPadding), Ui.Dp(QrPadding)) };
        Qr.Draw(code, url, (int)Ui.Dp(QrSize));
        qr.Add(code);
        card.Add(qr);
        View.Add(card);
        Root.Add(close.Item);
    }

    public void Show()
    {
        shell.Push(this);
        Tree.Focus(close.Item);
    }

    public void Close() => shell.Pop(this);
}
