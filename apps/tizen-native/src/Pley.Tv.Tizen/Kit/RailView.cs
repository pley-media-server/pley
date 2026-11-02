using System;
using System.Collections.Generic;
using Pley.Tv.Core.Ui;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Kit;

/// <summary>Who the rail's avatar shows: a name, and the profile's colour or picture.</summary>
sealed record RailProfile(string Name, Color? Color = null)
{
    public string Initial => Name.Length > 0 ? char.ToUpperInvariant(Name[0]).ToString() : "";
}

/// <summary>
/// The rail (the Android app's TvRail, design spec §2.1): 56 dp entries collapsed, 256 open. The
/// avatar at the top switches profiles; then the entries; Settings at the foot, at y = 465.
/// Collapsed, the current entry has a 4 × 20 accent mark and its icon in Fg; open, it is a Surface3
/// pill. The focused entry is the accent pill at 1.05. Open (focus in it), the rail is Surface at
/// 96 % with a soft edge, and the page behind it is dimmed.
///
/// It moves as tv-material's drawer does: the panel and the entries widen, the labels fade in as
/// they slide in from the left and fade out where they are, and the dim comes and goes at once.
/// Over 200 ms, as the design spec gives the drawer (PleyMotion.OverlayMs); with Reduce motion, it
/// opens and closes in place.
/// </summary>
sealed class RailView : View
{
    /// <summary>The avatar's entry: 56 tall from y = 15, so its circle sits at y = 27 and Search starts at y = 75.</summary>
    const float AvatarTop = 15, AvatarHeight = 56, AvatarSize = 32, FirstEntry = 75;
    /// <summary>48 dp entries on a 56 dp pitch; Settings at the foot.</summary>
    const float EntryHeight = 48, Pitch = 56, SettingsY = 465;
    /// <summary>The open rail's soft right edge.</summary>
    const float Shadow = 64;
    /// <summary>How far a label slides in from: a little, not tv-material's whole width, since an entry doesn't clip it and it would cross the icon.</summary>
    const float LabelSlide = 24;

    readonly FocusTree tree;
    readonly View panel;
    readonly View edge;
    readonly List<Entry> entries = new List<Entry>();
    readonly Entry avatar;
    readonly View avatarCircle;
    readonly TextLabel avatarInitial;
    readonly Func<bool> reduced;
    Animation? motion;
    string? current;
    bool open;

    public RailView(FocusTree tree, Func<bool> reducedMotion, Action<RailItem> onSelect, Action onProfile)
    {
        this.tree = tree;
        reduced = reducedMotion;
        // Over the page and under the rail, as tv-material's scrim is: Bg at 50 %.
        Scrim = new View
        {
            Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(PleyTv.CanvasHeight)),
            BackgroundColor = new Color(PleyColors.Bg.R, PleyColors.Bg.G, PleyColors.Bg.B, 0.5f),
        };
        Scrim.Hide();
        Size = new Size(Ui.Dp(PleyTv.RailExpanded + Shadow), Ui.Dp(PleyTv.CanvasHeight));
        Group = new FocusGroup(Axis.Column, "rail");
        panel = new View { Size = new Size(Ui.Dp(PleyTv.RailExpanded), SizeHeight), BackgroundColor = new Color(PleyColors.Surface.R, PleyColors.Surface.G, PleyColors.Surface.B, 0.96f) };
        edge = new View { Size = new Size(Ui.Dp(Shadow), SizeHeight), Position = new Position(Ui.Dp(PleyTv.RailExpanded), 0), Background = Gradients.Horizontal(new Color(PleyColors.Surface.R, PleyColors.Surface.G, PleyColors.Surface.B, 0.85f), Color.Transparent) };
        panel.Hide();
        edge.Hide();
        Add(panel);
        Add(edge);

        avatar = new Entry(this, Rail.ProfileItem, null, "", AvatarTop, AvatarHeight, onProfile);
        avatarCircle = new View
        {
            Size = new Size(Ui.Dp(AvatarSize), Ui.Dp(AvatarSize)),
            Position = new Position(Ui.Dp(12), Ui.Dp((AvatarHeight - AvatarSize) / 2)),
            CornerRadius = Ui.Dp(AvatarSize / 2),
            BackgroundColor = PleyColors.Surface3,
        };
        avatarInitial = Text.Label("", PleyType.Label, PleyColors.Fg);
        avatarInitial.FontStyle = Text.Weight(PleyType.Bold);
        avatarInitial.Size = avatarCircle.Size;
        avatarInitial.HorizontalAlignment = HorizontalAlignment.Center;
        avatarCircle.Add(avatarInitial);
        avatar.Box.Add(avatarCircle);
        avatar.SetSecondLine("Switch profile");
        Group.Add(avatar.Item);
        Add(avatar.Box);
        this.onSelect = onSelect;
        Group.Add(AddEntry(Rail.SettingsItem, SettingsY).Item);
#if PROFILE
        // A profiling build says so, under Settings, where it's never in the way of a page.
        var debug = new TextLabel("DEBUG\nMODE")
        {
            PixelSize = Ui.Dp(8),
            TextColor = Color.Yellow,
            MultiLine = true,
            HorizontalAlignment = HorizontalAlignment.Center,
            Size = new Size(Ui.Dp(PleyTv.RailCollapsed), Ui.Dp(24)),
            Position = new Position(0, Ui.Dp(SettingsY + EntryHeight + 2)),
        };
        Add(debug);
#endif
        // Focus in the rail is what opens it.
        tree.Moved += item => Open(item?.Parent == Group);
    }

    readonly Action<RailItem> onSelect;

    public FocusGroup Group { get; }

    /// <summary>The open rail's dim over the page, for the shell to put between the page and the rail.</summary>
    public View Scrim { get; }

    /// <summary>The entries above Settings, as <see cref="Rail.Items"/> lists them.</summary>
    public void SetItems(IReadOnlyList<RailItem> items)
    {
        foreach (var e in entries)
        {
            if (e.Id == Rail.SettingsItem.Id) continue;
            Group.Remove(e.Item);
            e.Box.Unparent();
        }
        entries.RemoveAll(e => e.Id != Rail.SettingsItem.Id);
        for (var i = 0; i < items.Count; i++) Group.Insert(1 + i, AddEntry(items[i], FirstEntry + i * Pitch).Item);
        Mark();
    }

    public void SetProfile(RailProfile? profile)
    {
        avatarInitial.Text = profile?.Initial ?? "";
        avatarCircle.BackgroundColor = profile?.Color ?? PleyColors.Surface3;
        avatar.SetLabel(profile?.Name ?? "");
    }

    /// <summary>The entry for the page showing (<see cref="Rail.Current"/>); null on sign-in and Who's watching?.</summary>
    public void SetCurrent(string? id)
    {
        current = id;
        Mark();
    }

    /// <summary>Focus to the current entry (D-pad rule 3: Left from content, Back from a top page), which opens the rail.</summary>
    public bool FocusCurrent()
    {
        var target = entries.Find(e => e.Id == (current ?? "home")) ?? entries.Find(e => e.Id == "home");
        return target != null && tree.Focus(target.Item);
    }

    Entry AddEntry(RailItem item, float y)
    {
        var e = new Entry(this, item.Id, item.Icon, item.Label, y, EntryHeight, () => onSelect(item));
        e.Open(open, null);
        entries.Add(e);
        Add(e.Box);
        return e;
    }

    void Open(bool on)
    {
        if (on == open) return;
        open = on;
        motion?.Stop();
        motion = null;
        if (on) Scrim.Show();
        else Scrim.Hide();
        if (reduced())
        {
            panel.SizeWidth = Ui.Dp(PleyTv.RailExpanded);
            edge.PositionX = Ui.Dp(PleyTv.RailExpanded);
            edge.Opacity = 1;
            Panel(on);
            avatar.Open(on, null);
            foreach (var e in entries) e.Open(on, null);
            Mark();
            return;
        }
        var a = new Animation(PleyMotion.OverlayMs) { DefaultAlphaFunction = new AlphaFunction(AlphaFunction.BuiltinFunctions.EaseOut) };
        if (on && !panel.Visibility)
        {
            // From the closed rail's width: the panel grows out of it, its edge riding on its right.
            panel.SizeWidth = Ui.Dp(PleyTv.RailCollapsed);
            edge.PositionX = Ui.Dp(PleyTv.RailCollapsed);
            edge.Opacity = 0;
            Panel(true);
        }
        var width = Ui.Dp(on ? PleyTv.RailExpanded : PleyTv.RailCollapsed);
        a.AnimateTo(panel, "SizeWidth", width);
        a.AnimateTo(edge, "PositionX", width);
        a.AnimateTo(edge, "Opacity", on ? 1f : 0f);
        avatar.Open(on, a);
        foreach (var e in entries) e.Open(on, a);
        // Closed, what only the open rail shows goes once it has narrowed: hidden, it costs nothing to draw.
        a.Finished += (_, _) =>
        {
            if (a != motion || open) return;
            Panel(false);
            avatar.Closed();
            foreach (var e in entries) e.Closed();
        };
        motion = a;
        a.Play();
        Mark();
    }

    void Panel(bool shown)
    {
        if (shown)
        {
            panel.Show();
            edge.Show();
        }
        else
        {
            panel.Hide();
            edge.Hide();
        }
    }

    void Mark()
    {
        avatar.Look(open, current == Rail.ProfileItem);
        foreach (var e in entries) e.Look(open, e.Id == current);
    }

    /// <summary>One entry: its pill, icon and label, and the collapsed rail's current mark.</summary>
    sealed class Entry
    {
        readonly RailView rail;
        readonly ImageView? icon;
        readonly TextLabel label;
        TextLabel? second;
        readonly View mark;
        readonly float height;
        bool lastOpen, lastCurrent, lastFocused, labelsShown;
        Animation? focusAnimation;

        public Entry(RailView rail, string id, string? iconName, string text, float y, float height, Action select)
        {
            this.rail = rail;
            this.height = height;
            Id = id;
            Item = new FocusItem(id) { Selected = select };
            Box = new View
            {
                Size = new Size(Ui.Dp(56), Ui.Dp(height)),
                Position = new Position(Ui.Dp(8), Ui.Dp(y)),
                CornerRadius = Ui.Dp(PleyShape.ControlRadius),
                PivotPoint = global::Tizen.NUI.PivotPoint.CenterLeft,
            };
            mark = new View
            {
                Size = new Size(Ui.Dp(4), Ui.Dp(20)),
                Position = new Position(0, Ui.Dp((height - 20) / 2)),
                CornerRadius = Ui.Dp(2),
                BackgroundColor = PleyColors.Accent,
            };
            mark.Hide();
            Box.Add(mark);
            if (iconName != null)
            {
                icon = Icons.Icon(iconName, PleyTv.IconSize, PleyColors.Muted);
                icon.Position = new Position(Ui.Dp(18), Ui.Dp((height - PleyTv.IconSize) / 2));
                Box.Add(icon);
            }
            label = Text.Label(text, PleyType.Label, PleyColors.Muted, width: 256 - 56 - 12);
            label.Position = new Position(Ui.Dp(56), Ui.Dp((height - PleyType.Label.Line) / 2));
            label.Hide();
            Box.Add(label);
            Item.FocusChanged += _ => Look(lastOpen, lastCurrent);
        }

        public string Id { get; }
        public FocusItem Item { get; }
        public View Box { get; }

        public void SetLabel(string text) => Text.Show(label, text);

        /// <summary>The avatar's second line, under its name.</summary>
        public void SetSecondLine(string text)
        {
            label.Position = new Position(Ui.Dp(56), Ui.Dp(height / 2 - 17));
            second = Text.Label(text, PleyType.Caption, PleyColors.Muted, width: 256 - 56 - 12);
            second.Position = new Position(Ui.Dp(56), Ui.Dp(height / 2));
            second.Hide();
            Box.Add(second);
        }

        /// <summary>The pill's width and the labels, open or closed: over <paramref name="motion"/>, or at once without one.</summary>
        public void Open(bool open, Animation? motion)
        {
            var width = Ui.Dp(open ? 256 : 56);
            if (motion == null)
            {
                Box.SizeWidth = width;
                foreach (var text in Labels())
                {
                    text.Opacity = 1;
                    text.PositionX = Ui.Dp(56);
                }
                if (open) ShowLabels();
                else Closed();
                return;
            }
            motion.AnimateTo(Box, "SizeWidth", width);
            if (open && !labelsShown)
            {
                foreach (var text in Labels())
                {
                    text.Opacity = 0;
                    text.PositionX = Ui.Dp(56 - LabelSlide);
                }
                ShowLabels();
            }
            foreach (var text in Labels())
            {
                motion.AnimateTo(text, "Opacity", open ? 1f : 0f);
                if (open) motion.AnimateTo(text, "PositionX", Ui.Dp(56));
            }
        }

        /// <summary>The rail has closed: its labels go.</summary>
        public void Closed()
        {
            labelsShown = false;
            foreach (var text in Labels()) text.Hide();
        }

        void ShowLabels()
        {
            labelsShown = true;
            foreach (var text in Labels()) text.Show();
        }

        IEnumerable<TextLabel> Labels()
        {
            yield return label;
            if (second != null) yield return second;
        }

        public void Look(bool open, bool current)
        {
            lastOpen = open;
            lastCurrent = current;
            var focused = Item.Focused;
            Box.BackgroundColor = focused ? PleyColors.Accent : open && current ? PleyColors.Surface3 : Color.Transparent;
            var ink = focused ? PleyColors.OnAccent : current ? PleyColors.Fg : PleyColors.Muted;
            if (icon != null) icon.Color = ink;
            label.TextColor = ink;
            if (second != null) second.TextColor = focused ? PleyColors.OnAccent : PleyColors.Muted;
            if (current && !open) mark.Show();
            else mark.Hide();
            if (focused == lastFocused) return;
            lastFocused = focused;
            var size = focused ? PleyShape.PillScale : 1f;
            var scale = new Vector3(size, size, 1);
            focusAnimation?.Stop();
            focusAnimation = null;
            if (rail.reduced())
            {
                Box.Scale = scale;
                return;
            }
            focusAnimation = new Animation(focused ? PleyMotion.FocusMs : PleyMotion.UnfocusMs)
            {
                DefaultAlphaFunction = new AlphaFunction(focused ? AlphaFunction.BuiltinFunctions.EaseOut : AlphaFunction.BuiltinFunctions.EaseIn),
            };
            focusAnimation.AnimateTo(Box, "Scale", scale);
            focusAnimation.Play();
        }
    }
}

/// <summary>Gradients as NUI draws them: a visual on the view's background.</summary>
static class Gradients
{
    /// <summary>A 135° gradient, from the top left corner to the bottom right, as CSS draws it.</summary>
    public static PropertyMap Diagonal(Color from, Color to)
    {
        var offsets = new PropertyArray();
        offsets.Add(new PropertyValue(0f));
        offsets.Add(new PropertyValue(1f));
        var colors = new PropertyArray();
        colors.Add(new PropertyValue(from));
        colors.Add(new PropertyValue(to));
        var map = new PropertyMap();
        map.Add(Visual.Property.Type, new PropertyValue((int)Visual.Type.Gradient));
        map.Add(GradientVisualProperty.StartPosition, new PropertyValue(new Vector2(-0.5f, -0.5f)));
        map.Add(GradientVisualProperty.EndPosition, new PropertyValue(new Vector2(0.5f, 0.5f)));
        map.Add(GradientVisualProperty.StopOffset, new PropertyValue(offsets));
        map.Add(GradientVisualProperty.StopColor, new PropertyValue(colors));
        return map;
    }

    public static PropertyMap Horizontal(Color from, Color to, float stop = 0) => Linear(from, to, stop, new Vector2(-0.5f, 0), new Vector2(0.5f, 0));

    /// <summary>From the top down: <paramref name="from"/> until <paramref name="stop"/> (0 to 1), then to <paramref name="to"/> at the foot.</summary>
    public static PropertyMap Vertical(Color from, Color to, float stop = 0) => Linear(from, to, stop, new Vector2(0, -0.5f), new Vector2(0, 0.5f));

    static PropertyMap Linear(Color from, Color to, float stop, Vector2 start, Vector2 end)
    {
        var offsets = new PropertyArray();
        var colors = new PropertyArray();
        offsets.Add(new PropertyValue(0f));
        colors.Add(new PropertyValue(from));
        if (stop > 0)
        {
            offsets.Add(new PropertyValue(stop));
            colors.Add(new PropertyValue(from));
        }
        offsets.Add(new PropertyValue(1f));
        colors.Add(new PropertyValue(to));
        var map = new PropertyMap();
        map.Add(Visual.Property.Type, new PropertyValue((int)Visual.Type.Gradient));
        map.Add(GradientVisualProperty.StartPosition, new PropertyValue(start));
        map.Add(GradientVisualProperty.EndPosition, new PropertyValue(end));
        map.Add(GradientVisualProperty.StopOffset, new PropertyValue(offsets));
        map.Add(GradientVisualProperty.StopColor, new PropertyValue(colors));
        return map;
    }
}
