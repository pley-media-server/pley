using System;
using System.Collections.Generic;
using Pley.Tv.Core;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// The TV's way in (design spec §2.8; the Android app's TvLinkScreen): the steps and the code on
/// the left from x = 96, the QR code and "Change server" on the right from x = 600. "Change server"
/// has focus, except after a decline or once the code expired, when "Get a new code" has it.
/// Signing in, a server that requires two-step sign-in is said so under the title, once it answers,
/// and the globe at the top right picks the TV's language (Up reaches it).
/// </summary>
sealed class LinkScreen : Sheet
{
    const float Left = 96, ColumnWidth = 440, QrLeft = 600, QrTop = 120, QrSize = 268, StatusTop = 478;

    readonly LinkModel model;
    readonly View column;
    /// <summary>The steps, the code and what's under it: they move down when an intro comes under the title.</summary>
    readonly View steps;
    /// <summary>Where the intro goes, under the title.</summary>
    readonly float introTop;
    /// <summary>The screen's own intro (approving a profile): the server's two-step sign-in isn't said over it.</summary>
    readonly bool ownIntro;
    readonly View codeFields;
    readonly TextLabel address;
    readonly TextLabel addressLead;
    readonly View notes;
    readonly ButtonView newCode;
    readonly View qrBox;
    readonly View qrSide;
    readonly ButtonView back;
    readonly ButtonView? globeButton;
    /// <summary>Get a new code and the way back, side by side under the globe.</summary>
    readonly FocusGroup actions = new FocusGroup(Axis.Row, "link-actions");
    readonly LanguageGlobe? globe;
    readonly StatusLine status;
    readonly Timer countdown = new Timer(1000);
    readonly IDisposable watch;
    View? signedIn;
    LinkCode? shownCode;
    bool? neededNewCode;
    TextLabel? introLabel;
    bool twoStepShown;

    /// <param name="intro">
    /// Under the title: why a code (approving a profile on a phone); null when signing in, where it
    /// says when the server requires two-step sign-in.
    /// </param>
    /// <param name="title">The heading; "Sign in to Pley" without.</param>
    /// <param name="backLabel">The way back; "Change server" without.</param>
    /// <param name="globe">The globe, signing in; none approving a profile, where a household is signed in.</param>
    public LinkScreen(LinkModel model, Action onBack, string? title = null, string? intro = null, string? backLabel = null, Shell? shell = null, LanguageGlobe? globe = null)
        : base("link")
    {
        this.model = model;
        this.globe = globe;
        View.BackgroundColor = PleyColors.Bg;
        View.Add(Onboarding.BrandGlow());

        column = new View { Position = new Position(Ui.Dp(Left), Ui.Dp(56)), Size = new Size(Ui.Dp(ColumnWidth), Ui.Dp(400)) };
        View.Add(column);
        column.Add(Onboarding.Logo(40));
        var heading = Text.Label(title ?? Strings.T("signin.link.title"), PleyType.PageTitle, PleyColors.Fg, width: ColumnWidth);
        heading.PositionY = Ui.Dp(40 + 28);
        column.Add(heading);
        introTop = 40 + 28 + PleyType.PageTitle.Line + 6;
        steps = new View { Size = new Size(Ui.Dp(ColumnWidth), Ui.Dp(400)) };
        column.Add(steps);
        ownIntro = intro != null;
        Intro(intro);
        float y = 0;
        steps.Add(Step(1, y, out _, Strings.T("signin.link.scan")));
        y += 28 + 12;
        var step2 = Step(2, y, out var stepText, Strings.T("signin.link.goTo"));
        addressLead = stepText;
        address = Text.Label("…", PleyType.Section, PleyColors.Fg);
        address.SizeHeight = Ui.Dp(28);
        step2.Add(address);
        steps.Add(step2);
        y += 28 + 12;
        steps.Add(Step(3, y, out _, Strings.T("signin.link.enterCode")));
        y += 28 + 12;
        codeFields = new View { PositionY = Ui.Dp(y), Size = new Size(Ui.Dp(ColumnWidth), Ui.Dp(76)) };
        steps.Add(codeFields);
        y += 76 + 16;
        notes = new View { PositionY = Ui.Dp(y) };
        var sameWifi = Text.Label(Strings.T("signin.link.sameWifi"), PleyType.Body, PleyColors.Muted, width: ColumnWidth);
        var otherDevice = Text.Label(Strings.T("signin.link.otherDevice"), PleyType.Body, PleyColors.Muted, width: ColumnWidth);
        otherDevice.PositionY = Ui.Dp(PleyType.Body.Line + 6);
        notes.Add(sameWifi);
        notes.Add(otherDevice);
        steps.Add(notes);
        newCode = new ButtonView(Strings.T("signin.link.newCode"), ButtonKind.Pill, model.NewCode, iconName: "refresh-cw") { PositionY = Ui.Dp(y) };
        newCode.Hide();
        newCode.Item.Shown = false;
        steps.Add(newCode);

        status = new StatusLine(ColumnWidth) { Position = new Position(Ui.Dp(Left), Ui.Dp(StatusTop)) };
        View.Add(status);

        // The white tile is the code's quiet zone: its modules as big as 4 modules of margin allow.
        qrSide = new View { Position = new Position(Ui.Dp(QrLeft), Ui.Dp(QrTop)), Size = new Size(Ui.Dp(QrSize), Ui.Dp(QrSize + 12 + 17 + 18 + PleyTv.ButtonHeight)) };
        qrBox = new View { Size = new Size(Ui.Dp(QrSize), Ui.Dp(QrSize)), CornerRadius = Ui.Dp(PleyShape.DialogRadius), BackgroundColor = PleyColors.Surface2 };
        qrSide.Add(qrBox);
        var scan = Text.Label(Strings.T("signin.link.scanCamera"), PleyType.Caption, PleyColors.Muted, width: QrSize);
        scan.HorizontalAlignment = HorizontalAlignment.Center;
        scan.PositionY = Ui.Dp(QrSize + 12);
        qrSide.Add(scan);
        back = new ButtonView(backLabel ?? Strings.T("signin.link.changeServer"), ButtonKind.Text, onBack);
        back.Position = new Position(Ui.Dp((QrSize - back.WidthDp) / 2), Ui.Dp(QrSize + 12 + 17 + 18));
        qrSide.Add(back);
        View.Add(qrSide);

        if (shell != null && globe != null)
        {
            globeButton = GlobeButton.Of(shell, globe);
            globeButton.Position = new Position(Ui.Dp(PleyTv.CanvasWidth - GlobeButton.Right) - globeButton.SizeWidth, Ui.Dp(GlobeButton.Top));
            View.Add(globeButton);
            Root.Add(globeButton.Item);
        }
        actions.Add(newCode.Item);
        actions.Add(back.Item);
        Root.Add(actions);

        countdown.Tick += (s, e) =>
        {
            if (model.State.Value.Status is LinkStatus.Unreachable u) ShowStatus(u);
            return true;
        };
        watch = model.State.Watch(Show);
        Gone += () =>
        {
            watch.Dispose();
            countdown.Stop();
            countdown.Dispose();
            model.Stopped();
        };
    }

    /// <summary>On screen: the code is asked about. Drawn again after the globe changed the language, the globe has focus.</summary>
    public void Started()
    {
        if (globeButton != null && globe!.TakeFocus()) Tree.Focus(globeButton.Item);
        else Tree.Focus(back.Item);
        model.Started();
    }

    /// <summary>
    /// Under the title: why a code (approving a profile on a phone), or that the server requires
    /// two-step sign-in; the steps start under it, or 18 dp under the title without.
    /// </summary>
    void Intro(string? text)
    {
        if (text == null)
        {
            introLabel?.Hide();
            steps.PositionY = Ui.Dp(introTop - 6 + 18);
            return;
        }
        if (introLabel == null)
        {
            introLabel = Text.Label(text, PleyType.Body, PleyColors.Muted, maxLines: 2, width: ColumnWidth);
            introLabel.VerticalAlignment = VerticalAlignment.Top;
            introLabel.PositionY = Ui.Dp(introTop);
            column.Add(introLabel);
        }
        else
        {
            introLabel.Text = text;
            introLabel.Show();
        }
        steps.PositionY = Ui.Dp(introTop + PleyType.Body.Line * 2 + 10);
    }

    void Show(LinkState state)
    {
        if (!ownIntro && state.TwoStepRequired != twoStepShown)
        {
            twoStepShown = state.TwoStepRequired;
            Intro(twoStepShown ? LinkText.TwoStepRequired : null);
        }
        var s = state.Shown;
        if (s is LinkStatus.SignedIn done)
        {
            ShowSignedIn(done.Name);
            return;
        }
        if (state.Code != shownCode)
        {
            shownCode = state.Code;
            ShowCode(state.Code);
        }
        var dimmed = state.Code == null || s is LinkStatus.Expired || (s is LinkStatus.Starting && state.Code != null);
        codeFields.Opacity = dimmed ? 0.4f : 1f;
        qrBox.Opacity = s is LinkStatus.Expired ? 0.4f : 1f;
        if (neededNewCode != state.NeedsNewCode)
        {
            neededNewCode = state.NeedsNewCode;
            if (state.NeedsNewCode)
            {
                notes.Hide();
                newCode.Show();
                newCode.Item.Shown = true;
                Tree.Focus(newCode.Item);
            }
            else
            {
                newCode.Item.Shown = false;
                newCode.Hide();
                notes.Show();
                Tree.Focus(back.Item);
            }
        }
        ShowStatus(s);
    }

    /// <summary>The status line at y = 478: waiting (the breathing dot), declined, expired, retrying, or what failed.</summary>
    void ShowStatus(LinkStatus s)
    {
        countdown.Stop();
        switch (s)
        {
            case LinkStatus.Starting:
                status.Set(Strings.T("signin.link.gettingCode"), waiting: true, color: PleyColors.Muted);
                break;
            case LinkStatus.Waiting:
                status.Set(Strings.T("signin.link.waiting"), waiting: true, color: PleyColors.Muted);
                break;
            case LinkStatus.Unreachable u:
                var seconds = Math.Max(0, (int)((u.RetryAt - DateTimeOffset.UtcNow.ToUnixTimeMilliseconds() + 999) / 1000));
                status.Set(Strings.T("signin.link.retrying", ("seconds", seconds)), color: PleyColors.DangerText);
                countdown.Start();
                break;
            case LinkStatus.Declined:
                status.Set(Strings.T("signin.link.declined"), color: PleyColors.DangerText);
                break;
            case LinkStatus.Expired:
                status.Set(Strings.T("signin.link.expired"), color: PleyColors.Muted);
                break;
            case LinkStatus.Failed f:
                status.Set(f.Message, color: PleyColors.DangerText);
                break;
        }
    }

    void ShowCode(LinkCode? code)
    {
        // Step 2: "or go to" and the address, bold and a size up.
        address.Text = code?.Address ?? "…";
        address.PositionX = addressLead.PositionX + Ui.Dp(addressLead.NaturalSize.Width / Ui.Scale + 6);
        address.SizeWidth = Ui.Dp(ColumnWidth) - address.PositionX;

        foreach (var child in new List<View>(codeFields.Children))
        {
            codeFields.Remove(child);
            child.Dispose();
        }
        var halves = code?.Halves ?? new[] { "····", "····" };
        float x = 0;
        for (var i = 0; i < halves.Count; i++)
        {
            if (i > 0)
            {
                codeFields.Add(new View
                {
                    Size = new Size(Ui.Dp(20), Ui.Dp(4)),
                    Position = new Position(Ui.Dp(x + 16), Ui.Dp(36)),
                    CornerRadius = Ui.Dp(2),
                    BackgroundColor = PleyColors.Subtle,
                });
                x += 52;
            }
            var text = Text.Label(halves[i], PleyType.Code, PleyColors.Fg);
            var width = text.NaturalSize.Width / Ui.Scale + 2;
            text.Size = new Size(Ui.Dp(width), Ui.Dp(76));
            text.Position = new Position(Ui.Dp(20), 0);
            var field = new View
            {
                Size = new Size(Ui.Dp(width + 40), Ui.Dp(76)),
                PositionX = Ui.Dp(x),
                CornerRadius = Ui.Dp(PleyShape.CardRadius),
                BackgroundColor = PleyColors.Surface2,
            };
            field.Add(text);
            codeFields.Add(field);
            x += width + 40;
        }

        foreach (var child in new List<View>(qrBox.Children))
        {
            qrBox.Remove(child);
            child.Dispose();
        }
        qrBox.BackgroundColor = code != null ? Color.White : PleyColors.Surface2;
        // Too long for a QR code: the address and code still show.
        if (code != null) Qr.Draw(qrBox, code.Qr, (int)Ui.Dp(QrSize));
    }

    /// <summary>For about half a second: a 64 dp success tile with a check, and "Signed in as Anna"; the code and the way back go.</summary>
    void ShowSignedIn(string? name)
    {
        if (signedIn != null) return;
        countdown.Stop();
        column.Hide();
        qrSide.Hide();
        status.Hide();
        if (globeButton != null)
        {
            globeButton.Hide();
            globeButton.Item.Shown = false;
        }
        Tree.Blur();
        signedIn = new View { Position = new Position(Ui.Dp(Left), Ui.Dp(200)), Size = new Size(Ui.Dp(ColumnWidth), Ui.Dp(64 + 20 + PleyType.PageTitle.Line * 2)) };
        var tile = new View { Size = new Size(Ui.Dp(64), Ui.Dp(64)), CornerRadius = Ui.Dp(PleyShape.DialogRadius), BackgroundColor = PleyColors.Success };
        var check = Icons.Icon("check", 32, PleyColors.Bg);
        check.Position = new Position(Ui.Dp(16), Ui.Dp(16));
        tile.Add(check);
        signedIn.Add(tile);
        var words = Text.Label(name != null ? Strings.T("signin.link.signedInAs", ("name", name)) : Strings.T("signin.link.signedIn"), PleyType.PageTitle, PleyColors.Fg, maxLines: 2,
            width: ColumnWidth);
        words.VerticalAlignment = VerticalAlignment.Top;
        words.PositionY = Ui.Dp(64 + 20);
        signedIn.Add(words);
        View.Add(signedIn);
    }

    /// <summary>A numbered step: a 28 dp Surface3 square with the number, then its words in BodyLarge.</summary>
    static View Step(int number, float y, out TextLabel text, string words)
    {
        var row = new View { PositionY = Ui.Dp(y), Size = new Size(Ui.Dp(ColumnWidth), Ui.Dp(28)) };
        var badge = new View { Size = new Size(Ui.Dp(28), Ui.Dp(28)), CornerRadius = Ui.Dp(PleyShape.FieldRadius), BackgroundColor = PleyColors.Surface3 };
        var digit = Text.Label(number.ToString(), PleyType.Label, PleyColors.Fg, width: 28);
        digit.SizeHeight = Ui.Dp(28);
        digit.HorizontalAlignment = HorizontalAlignment.Center;
        badge.Add(digit);
        row.Add(badge);
        text = Text.Label(words, PleyType.BodyLarge, PleyColors.Fg);
        text.Size = new Size(text.NaturalSize.Width + 2, Ui.Dp(28));
        text.PositionX = Ui.Dp(28 + 14);
        row.Add(text);
        return row;
    }
}
