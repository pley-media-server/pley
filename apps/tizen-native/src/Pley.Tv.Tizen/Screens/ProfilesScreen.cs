using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// "Who's watching?" (design spec §2.9; the Android app's TvProfilesScreen): one row of large
/// avatars, focus on the last profile used so one OK continues; the PIN pad for a profile with a
/// PIN; a new code for a profile that needs its password, approved on a phone.
/// </summary>
sealed class ProfilesScreen : Sheet
{
    const float Avatar = 100, AvatarGap = 40, RowTop = 204;

    readonly Shell shell;
    readonly ProfilesModel model;
    readonly Images images;
    readonly Func<ProfileTile, LinkScreen> approve;
    readonly View picker = new View { WidthResizePolicy = ResizePolicyType.FillToParent, HeightResizePolicy = ResizePolicyType.FillToParent };
    readonly View row = new View();
    readonly FocusGroup rowGroup = new FocusGroup(Axis.Row, "profiles");
    readonly View below = new View();
    readonly ButtonRow belowButtons = new ButtonRow(12);
    readonly List<ProfileChoice> choices = new List<ProfileChoice>();
    readonly IDisposable watch;
    readonly IDisposable digits;
    IReadOnlyList<ProfileTile> shownProfiles = Array.Empty<ProfileTile>();
    PinPad? pad;
    LinkScreen? approval;
    string shownBelow = "";

    public ProfilesScreen(Shell shell, ProfilesModel model, Images images, Func<ProfileTile, LinkScreen> approve) : base("profiles")
    {
        this.shell = shell;
        this.model = model;
        this.images = images;
        this.approve = approve;
        View.BackgroundColor = PleyColors.Bg;
        var title = Text.Label(Strings.T("profiles.title"), PleyType.PageTitle, PleyColors.Fg, width: PleyTv.CanvasWidth);
        title.HorizontalAlignment = HorizontalAlignment.Center;
        title.PositionY = Ui.Dp(110);
        picker.Add(title);
        row.PositionY = Ui.Dp(RowTop);
        picker.Add(row);
        below.PositionY = Ui.Dp(400);
        picker.Add(below);
        View.Add(picker);
        Root.Add(rowGroup);
        Root.Add(belowButtons.Group);
        // Back on the pad goes back to the picker; on the picker it leaves the app.
        Back = () =>
        {
            if (model.State.Value.Step is ProfilesStep.Pick) return false;
            model.BackToPick();
            return true;
        };
        // D-pad rule 10: the remote's digit keys type into the PIN pad.
        digits = shell.Input.Push((key, down) => shell.Top == this && pad != null && pad.OnKey(key, down));
        watch = model.State.Watch(Show);
        Gone += () =>
        {
            watch.Dispose();
            digits.Dispose();
        };
    }

    void Show(ProfilesState state)
    {
        switch (state.Step)
        {
            case ProfilesStep.Pin step:
                CloseApproval();
                if (pad == null || pad.Profile.Id != step.Profile.Id)
                {
                    ClosePad();
                    pad = new PinPad(step.Profile, model, images, Tree);
                    picker.Hide();
                    View.Add(pad);
                    Root.Add(pad.Group);
                    Tree.Focus(pad.Initial);
                }
                pad.Show(step);
                if (step.LockedOut && pad.Another != null) Tree.Focus(pad.Another);
                return;
            case ProfilesStep.Approve a:
                ClosePad();
                picker.Show();
                if (approval == null)
                {
                    approval = approve(a.Profile);
                    approval.Gone += () =>
                    {
                        approval = null;
                        if (model.State.Value.Step is ProfilesStep.Approve) model.BackToPick();
                    };
                    shell.Push(approval);
                    approval.Started();
                }
                break;
            default:
                CloseApproval();
                ClosePad();
                picker.Show();
                break;
        }
        ShowPicker(state);
    }

    void ClosePad()
    {
        if (pad == null) return;
        Root.Remove(pad.Group);
        pad.Unparent();
        pad.Dispose();
        pad = null;
        var focused = choices.FirstOrDefault(c => c.Tile.Id == model.State.Value.Focus);
        if (focused != null) Tree.Focus(focused.Item);
    }

    void CloseApproval()
    {
        if (approval == null) return;
        var a = approval;
        approval = null;
        shell.Pop(a);
    }

    void ShowPicker(ProfilesState state)
    {
        foreach (var c in choices) c.SetBusy(state.Busy == c.Tile.Id);
        if (!state.Profiles.SequenceEqual(shownProfiles))
        {
            shownProfiles = state.Profiles;
            foreach (var c in choices)
            {
                c.Unparent();
                c.Dispose();
            }
            choices.Clear();
            rowGroup.Clear();
            var count = state.Profiles.Count;
            var width = count * Avatar + Math.Max(0, count - 1) * AvatarGap;
            // Centred while they fit; from the side margin when they don't.
            var left = Math.Max(48, (PleyTv.CanvasWidth - width) / 2);
            for (var i = 0; i < count; i++)
            {
                var tile = state.Profiles[i];
                var choice = new ProfileChoice(tile, images, () => _ = model.Pick(tile.Id))
                {
                    PositionX = Ui.Dp(left + i * (Avatar + AvatarGap)),
                };
                choices.Add(choice);
                row.Add(choice);
                rowGroup.Add(choice.Item);
            }
            var initial = choices.FirstOrDefault(c => c.Tile.Id == state.Focus) ?? choices.FirstOrDefault();
            if (initial != null && pad == null) Tree.Focus(initial.Item);
        }
        ShowBelow(state);
    }

    /// <summary>Under the row: what went wrong, Retry when nothing loaded, and "Sign in as another profile" without the profiles scope.</summary>
    void ShowBelow(ProfilesState state)
    {
        var retry = state.Profiles.Count == 0 && !state.Loading;
        var key = $"{state.Error}|{retry}|{state.CanSwitch}";
        if (key == shownBelow) return;
        shownBelow = key;
        foreach (var child in below.Children.ToList())
        {
            if (child == belowButtons) continue;
            below.Remove(child);
            child.Dispose();
        }
        foreach (var child in belowButtons.Children.ToList())
        {
            belowButtons.Remove(child);
            child.Dispose();
        }
        belowButtons.Group.Clear();
        belowButtons.Arrange();
        float y = 0;
        if (state.Error != null)
        {
            var error = Text.Label(state.Error, PleyType.Body, PleyColors.DangerText, maxLines: 2, width: 560);
            error.HorizontalAlignment = HorizontalAlignment.Center;
            error.VerticalAlignment = VerticalAlignment.Top;
            error.PositionX = Ui.Dp((PleyTv.CanvasWidth - 560) / 2);
            below.Add(error);
            y += PleyType.Body.Line * 2 + 12;
        }
        ButtonView? first = null;
        if (retry) first = belowButtons.Add(new ButtonView(Strings.T("common.actions.retry"), ButtonKind.Pill, () => _ = model.Reload(), iconName: "refresh-cw"));
        if (!state.CanSwitch)
        {
            var another = belowButtons.Add(new ButtonView(Strings.T("profiles.signInAnother"), ButtonKind.Text, model.SignInAgain));
            if (state.Profiles.Count == 0) first ??= another;
        }
        if (belowButtons.WidthDp > 0)
        {
            belowButtons.Position = new Position(Ui.Dp((PleyTv.CanvasWidth - belowButtons.WidthDp) / 2), Ui.Dp(y));
            if (belowButtons.GetParent() == null) below.Add(belowButtons);
        }
        if (first != null && choices.Count == 0) Tree.Focus(first.Item);
    }
}

/// <summary>
/// A profile: its 100 dp avatar and name. Focused, the avatar grows to 1.1 with the focus ring; the
/// name turns bold. A PIN profile wears a lock.
/// </summary>
sealed class ProfileChoice : View
{
    const float AvatarSize = 100;

    readonly View avatar;
    readonly View ring;
    readonly TextLabel name;
    Animation? focusAnimation;

    public ProfileChoice(ProfileTile tile, Images images, Action onPick)
    {
        Tile = tile;
        Size = new Size(Ui.Dp(AvatarSize), Ui.Dp(AvatarSize + 12 + PleyType.Label.Line));
        Item = new FocusItem(tile.Name) { Selected = onPick };
        avatar = new View { Size = new Size(Ui.Dp(AvatarSize), Ui.Dp(AvatarSize)), PivotPoint = global::Tizen.NUI.PivotPoint.Center };
        avatar.Add(new AvatarView(tile.Name, tile.Color, tile.AvatarUrl, AvatarSize, images));
        // The ring: a 3 dp AccentBright stroke centred 4.5 dp outside the circle.
        var reach = PleyShape.RingOffset + PleyShape.RingStroke / 2;
        ring = new View
        {
            Size = new Size(Ui.Dp(AvatarSize + reach * 2), Ui.Dp(AvatarSize + reach * 2)),
            Position = new Position(-Ui.Dp(reach), -Ui.Dp(reach)),
            CornerRadius = Ui.Dp(AvatarSize / 2 + reach),
            BorderlineWidth = Ui.Dp(PleyShape.RingStroke),
            BorderlineColor = PleyColors.AccentBright,
            BorderlineOffset = -1,
            Opacity = 0,
        };
        avatar.Add(ring);
        if (tile.Locked)
        {
            var badge = new View
            {
                Size = new Size(Ui.Dp(28), Ui.Dp(28)),
                Position = new Position(Ui.Dp(AvatarSize * 0.86f - 14), Ui.Dp(AvatarSize * 0.86f - 14)),
                CornerRadius = Ui.Dp(14),
                BackgroundColor = PleyColors.Bg,
            };
            var inner = new View { Size = new Size(Ui.Dp(24), Ui.Dp(24)), Position = new Position(Ui.Dp(2), Ui.Dp(2)), CornerRadius = Ui.Dp(12), BackgroundColor = PleyColors.Surface3 };
            var lockIcon = Icons.Icon("lock", 14, PleyColors.Fg);
            lockIcon.Position = new Position(Ui.Dp(5), Ui.Dp(5));
            inner.Add(lockIcon);
            badge.Add(inner);
            avatar.Add(badge);
        }
        Add(avatar);
        name = Text.Label(tile.Name, PleyType.Label, PleyColors.Fg, width: AvatarSize + 40);
        name.HorizontalAlignment = HorizontalAlignment.Center;
        name.Position = new Position(-Ui.Dp(20), Ui.Dp(AvatarSize + 12));
        Add(name);
        Item.FocusChanged += Focus;
    }

    public ProfileTile Tile { get; }
    public FocusItem Item { get; }

    public void SetBusy(bool busy) => avatar.Opacity = busy ? 0.6f : 1f;

    void Focus(bool on)
    {
        // Let go already: a focus change can still reach it on its way to the view in its place.
        if (Disposed) return;
        name.FontStyle = Text.Weight(on ? PleyType.Bold : PleyType.Medium);
        focusAnimation?.Stop();
        var scale = on ? PleyShape.AvatarScale : 1f;
        focusAnimation = new Animation(on ? PleyMotion.FocusMs : PleyMotion.UnfocusMs)
        {
            DefaultAlphaFunction = new AlphaFunction(on ? AlphaFunction.BuiltinFunctions.EaseOut : AlphaFunction.BuiltinFunctions.EaseIn),
        };
        focusAnimation.AnimateTo(avatar, "Scale", new Vector3(scale, scale, 1));
        focusAnimation.AnimateTo(ring, "Opacity", on ? 1f : 0f);
        focusAnimation.Play();
    }
}

/// <summary>
/// The PIN pad (design spec §2.9): the avatar, "Enter Anna's PIN", up to 8 dots, the 3 × 4 keypad
/// with focus on 5, and a hint about the remote's digit keys. A wrong PIN shakes the dots, clears
/// them and shows the server's sentence. A locked PIN dims the pad, and "Use another profile" takes focus.
/// </summary>
sealed class PinPad : View
{
    readonly ProfilesModel model;
    readonly FocusTree tree;
    readonly View dots = new View();
    readonly TextLabel error;
    readonly Keypad keypad;
    readonly TextLabel hint;
    readonly ButtonView? anotherButton;
    int shownDigits = -1;
    int shownShakes;
    Animation? shake;

    public PinPad(ProfileTile profile, ProfilesModel model, Images images, FocusTree tree)
    {
        this.tree = tree;
        Profile = profile;
        this.model = model;
        WidthResizePolicy = ResizePolicyType.FillToParent;
        HeightResizePolicy = ResizePolicyType.FillToParent;
        var center = PleyTv.CanvasWidth / 2;
        var avatar = new AvatarView(profile.Name, profile.Color, profile.AvatarUrl, 56, images) { Position = new Position(Ui.Dp(center - 28), Ui.Dp(30)) };
        Add(avatar);
        var title = Text.Label(Strings.T("profiles.pin.title", ("name", profile.Name)), PleyType.Title, PleyColors.Fg, width: 600);
        title.HorizontalAlignment = HorizontalAlignment.Center;
        title.Position = new Position(Ui.Dp(center - 300), Ui.Dp(30 + 56 + 14));
        Add(title);
        dots.PositionY = Ui.Dp(30 + 56 + 14 + PleyType.Title.Line + 16);
        Add(dots);
        error = Text.Label("", PleyType.Body, PleyColors.DangerText, width: 560);
        error.HorizontalAlignment = HorizontalAlignment.Center;
        error.SizeHeight = Ui.Dp(22);
        error.Position = new Position(Ui.Dp(center - 280), dots.PositionY + Ui.Dp(14));
        Add(error);
        keypad = new Keypad(PadKey.PinKeys, OnPadKey);
        keypad.Position = new Position(Ui.Dp(center) - keypad.SizeWidth / 2, error.PositionY + Ui.Dp(22));
        Add(keypad);
        var underY = keypad.PositionY + keypad.SizeHeight + Ui.Dp(16);
        hint = Text.Label(Strings.T("profiles.pin.keysHint"), PleyType.Caption, PleyColors.Muted, width: 600);
        hint.HorizontalAlignment = HorizontalAlignment.Center;
        hint.Position = new Position(Ui.Dp(center - 300), underY);
        Add(hint);
        Group = new FocusGroup(Axis.Column, "pin-pad");
        Group.Add(keypad.Group);
        anotherButton = new ButtonView(Strings.T("profiles.pin.otherProfile"), ButtonKind.Text, model.BackToPick);
        anotherButton.Position = new Position(Ui.Dp(center - anotherButton.WidthDp / 2), underY);
        anotherButton.Hide();
        anotherButton.Item.Shown = false;
        Add(anotherButton);
        Group.Add(anotherButton.Item);
    }

    public ProfileTile Profile { get; }
    public FocusGroup Group { get; }
    public FocusItem Initial => keypad.Initial;
    public FocusItem? Another => anotherButton?.Item.Shown == true ? anotherButton.Item : null;

    public void Show(ProfilesStep.Pin step)
    {
        if (step.Digits != shownDigits)
        {
            shownDigits = step.Digits;
            foreach (var d in dots.Children.ToList())
            {
                dots.Remove(d);
                d.Dispose();
            }
            var count = Math.Max(PinEntry.Min, step.Digits);
            var width = count * 14 + (count - 1) * 16;
            dots.Size = new Size(Ui.Dp(width), Ui.Dp(14));
            dots.PositionX = Ui.Dp(PleyTv.CanvasWidth / 2 - width / 2f);
            for (var i = 0; i < count; i++)
            {
                var filled = i < step.Digits;
                dots.Add(new View
                {
                    Size = new Size(Ui.Dp(14), Ui.Dp(14)),
                    PositionX = Ui.Dp(i * 30),
                    CornerRadius = Ui.Dp(7),
                    BackgroundColor = filled ? PleyColors.Fg : Color.Transparent,
                    BorderlineWidth = filled ? 0 : Ui.Dp(2),
                    BorderlineColor = PleyColors.Control,
                    BorderlineOffset = -1,
                });
            }
        }
        Text.Show(error, step.Error ?? "");
        if (step.Shakes != shownShakes)
        {
            shownShakes = step.Shakes;
            Shake();
        }
        keypad.Opacity = step.LockedOut ? 0.4f : 1f;
        foreach (var item in keypad.Group.Children.OfType<FocusItem>()) item.Enabled = !step.LockedOut;
        if (anotherButton != null)
        {
            anotherButton.Item.Shown = step.LockedOut;
            if (step.LockedOut)
            {
                anotherButton.Show();
                hint.Hide();
            }
            else
            {
                anotherButton.Hide();
                hint.Show();
            }
        }
    }

    /// <summary>The remote's digit keys and Delete: true when the pad took the key.</summary>
    public bool OnKey(string key, bool down)
    {
        int digit;
        if (key.Length == 1 && char.IsDigit(key[0])) digit = key[0] - '0';
        else if (key.StartsWith("KP_") && key.Length == 4 && char.IsDigit(key[3])) digit = key[3] - '0';
        else if (key is "BackSpace" or "Delete" or "Clear")
        {
            if (down)
            {
                keypad.Flash(new PadKey.Delete());
                model.Delete();
            }
            return true;
        }
        else return false;
        if (down)
        {
            keypad.Flash(new PadKey.Digit(digit));
            model.Digit(digit);
            // A PIN typed with the remote's number keys is sent with OK: OK takes focus once it could be sent.
            if (model.State.Value.Step is ProfilesStep.Pin { LockedOut: false } p && p.Digits >= PinEntry.Min) {
                var ok = keypad.Ok;
                if (ok != null) tree.Focus(ok);
            }
        }
        return true;
    }

    void OnPadKey(PadKey key)
    {
        switch (key)
        {
            case PadKey.Digit d:
                model.Digit(d.Value);
                break;
            case PadKey.Delete:
                model.Delete();
                break;
            case PadKey.Ok:
                _ = model.Submit();
                break;
        }
    }

    /// <summary>3 × 6 dp over 240 ms.</summary>
    void Shake()
    {
        shake?.Stop();
        var x = dots.PositionX;
        shake = new Animation(240);
        var steps = new[] { -6f, 6f, -6f, 6f, -6f, 0f };
        for (var i = 0; i < steps.Length; i++)
            shake.AnimateTo(dots, "PositionX", x + Ui.Dp(steps[i]), i * 40, (i + 1) * 40, new AlphaFunction(AlphaFunction.BuiltinFunctions.Linear));
        shake.Play();
    }
}
