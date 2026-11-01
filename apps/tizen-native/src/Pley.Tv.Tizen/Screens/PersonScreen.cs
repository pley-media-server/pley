using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// A person's page on TV (design spec §4.9; the Android app's TvPersonScreen): the photo as a 160 dp
/// circle at (96, 58), the name in PageTitle beside it and what of theirs is in the libraries; then
/// a row per role (Acting, Directing…), captioned with the year and the part. Focus starts on the
/// first title. A title opens its page; holding OK or Menu on it opens its actions.
/// </summary>
sealed class PersonScreen : ShellPage
{
    const float PhotoTop = 58, PhotoSize = 160, PhotoGap = 32, RowsTop = 240, MessageWidth = 464, MessageIcon = 40;
    /// <summary>The column beside the photo: from its right to the safe edge.</summary>
    const float TextLeft = PleyTv.Gutter + PhotoSize + PhotoGap, TextWidth = PleyTv.CanvasWidth - PleyTv.SafeHorizontal - TextLeft;

    readonly PersonModel model;
    readonly Images images;
    readonly KeyRepeatGate gate;
    readonly Action<PageKey> go;
    readonly List<IDisposable> watches = new List<IDisposable>();
    readonly View header = new View();
    readonly View body = new View { WidthResizePolicy = ResizePolicyType.FillToParent, HeightResizePolicy = ResizePolicyType.FillToParent };
    string shownKind = "";
    /// <summary>The header and rows as shown, so a page asked for again that hasn't changed isn't drawn again.</summary>
    string shownHeader = "", shownRows = "";
    RowColumnView? column;

    /// <param name="go">Opens another page: a title's.</param>
    public PersonScreen(TvShell shell, PersonModel model, Images images, KeyRepeatGate gate, Action<PageKey> go)
        : base(shell, new PageKey.Person(model.PersonId), "person")
    {
        this.model = model;
        this.images = images;
        this.gate = gate;
        this.go = go;
        View.BackgroundColor = PleyColors.Bg;
        View.Add(header);
        View.Add(body);
        watches.Add(model.State.Watch(Show));
    }

    public override void Enter()
    {
        if (!Tree.Enter(Group)) Wait();
    }

    public override void Gone()
    {
        foreach (var w in watches) w.Dispose();
        model.Dispose();
    }

    void Show(PersonState state)
    {
        switch (state)
        {
            case PersonState.Loading:
                if (shownKind == "loading") return;
                shownKind = "loading";
                Clear();
                Header(null);
                return;
            case PersonState.Ready ready:
                var first = shownKind != "ready";
                if (first) Clear();
                shownKind = "ready";
                Header(ready.Page);
                Rows(ready.Page, first);
                return;
            default:
                if (PersonText.Message(state) is not { } message) return;
                shownKind = state.GetType().Name;
                Clear();
                Message(message, state is PersonState.NotFound ? "user-round" : "circle-alert");
                return;
        }
    }

    void Clear()
    {
        Group.Clear();
        Empty(header);
        Empty(body);
        column = null;
        shownHeader = "";
        shownRows = "";
    }

    static void Empty(View view)
    {
        foreach (var child in view.Children.ToList())
        {
            view.Remove(child);
            child.Dispose();
        }
    }

    /// <summary>
    /// The photo on Surface2 (without one, the initials in PageTitle at 80 %), and 32 dp to its right
    /// the name (2 lines at most) over the count line, 8 apart, centred on the photo. Still blocks
    /// while nothing is known.
    /// </summary>
    void Header(PersonPage? page)
    {
        var key = page == null ? "loading" : string.Join("\n", page.Name, page.Photo ?? "", page.CountLine ?? "");
        if (key == shownHeader) return;
        shownHeader = key;
        Empty(header);
        var circle = new PhotoView { Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(PhotoTop)) };
        header.Add(circle);
        var middle = PhotoTop + PhotoSize / 2;
        if (page == null)
        {
            var top = middle - (38 + 8 + 20) / 2f;
            header.Add(Block(TextLeft, top, 280, 38));
            header.Add(Block(TextLeft, top + 38 + 8, 200, 20));
            return;
        }
        if (page.Photo is { } photo) images.Load(new[] { photo }, PhotoSize, circle.Picture);
        else
        {
            var initials = Text.Label(page.Initials, PleyType.PageTitle, new Color(PleyColors.Fg.R, PleyColors.Fg.G, PleyColors.Fg.B, 0.8f), width: PhotoSize);
            initials.HorizontalAlignment = HorizontalAlignment.Center;
            initials.SizeHeight = Ui.Dp(PhotoSize);
            circle.Add(initials);
        }
        var name = Text.Label(page.Name, PleyType.PageTitle, PleyColors.Fg, maxLines: 2, width: TextWidth);
        name.VerticalAlignment = VerticalAlignment.Top;
        name.SizeHeight = Math.Min(name.GetHeightForWidth(Ui.Dp(TextWidth)), Text.LineHeight(PleyType.PageTitle) * 2);
        var nameHeight = name.SizeHeight / Ui.Scale;
        var total = nameHeight + (page.CountLine != null ? 8 + PleyType.Body.Line : 0);
        var y = middle - total / 2;
        name.Position = new Position(Ui.Dp(TextLeft), Ui.Dp(y));
        header.Add(name);
        if (page.CountLine is { } counts)
        {
            var line = Text.Label(counts, PleyType.Body, PleyColors.Muted, width: TextWidth);
            line.Position = new Position(Ui.Dp(TextLeft), Ui.Dp(y + nameHeight + 8));
            header.Add(line);
        }
    }

    static View Block(float x, float y, float w, float h) => new View
    {
        Position = new Position(Ui.Dp(x), Ui.Dp(y)),
        Size = new Size(Ui.Dp(w), Ui.Dp(h)),
        CornerRadius = Ui.Dp(PleyShape.BadgeRadius),
        BackgroundColor = PleyColors.Surface2,
    };

    /// <summary>A row per role, the first header at y = 240, where each focused row's header comes; focus starts on the first title.</summary>
    void Rows(PersonPage page, bool first)
    {
        var key = string.Join("\n", page.Rows.Select(r => r.Role + ":" + string.Join(",", r.Tiles.Select(t => $"{t.Id}/{t.Title}/{t.Subtitle}"))));
        if (key == shownRows) return;
        shownRows = key;
        // The page asked again and it changed: focus goes back to the title it was on, if it's still there.
        var focusedName = Tree.Current != null && column != null && Inside(Tree.Current, column.Group) ? Tree.Current.Name : null;
        Empty(body);
        Group.Clear();
        column = new RowColumnView(gate, top: RowsTop, headerY: RowsTop);
        foreach (var row in page.Rows)
        {
            column.Add(new RowView(row.Title, row.Tiles, row.Kind, gate, images,
                tile => go(PersonRules.Opens(tile.Id)),
                tile => Shell.Actions?.Invoke(tile.Id, false),
                Shell.FocusRail, focusItem: Tree.Focus));
        }
        body.Add(column);
        Group.Add(column.Group);
        if (focusedName != null && Find(column.Group, focusedName) is { } again) Tree.Focus(again);
        else if ((first || focusedName != null) && Shell.Top == this) Tree.Enter(column.Group);
    }

    static bool Inside(FocusNode node, FocusGroup group)
    {
        for (var n = node.Parent; n != null; n = n.Parent)
            if (n == group) return true;
        return false;
    }

    static FocusItem? Find(FocusGroup group, string name)
    {
        foreach (var child in group.Children)
        {
            if (child is FocusItem item && item.Name == name) return item;
            if (child is FocusGroup g && Find(g, name) is { } found) return found;
        }
        return null;
    }

    /// <summary>
    /// Not found (Back) or failed (Retry), as the Android app's TvMessage: centred down the page at
    /// the gutter, an icon in Muted, 16 dp, the title in PageTitle, 8 dp, the sentence in Body Muted
    /// (464 wide at most), 24 dp, and the button, which has focus.
    /// </summary>
    void Message(LibraryMessage message, string iconName)
    {
        var words = Text.Label(message.Body, PleyType.Body, PleyColors.Muted, maxLines: 4, width: MessageWidth);
        words.VerticalAlignment = VerticalAlignment.Top;
        words.SizeHeight = Math.Min(words.GetHeightForWidth(Ui.Dp(MessageWidth)), Text.LineHeight(PleyType.Body) * 4);
        var wordsHeight = words.SizeHeight / Ui.Scale;
        var total = MessageIcon + 16 + PleyType.PageTitle.Line + 8 + wordsHeight + 24 + PleyTv.ButtonHeight;
        var y = (PleyTv.CanvasHeight - total) / 2;
        var icon = Icons.Icon(iconName, MessageIcon, PleyColors.Muted);
        icon.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(y));
        body.Add(icon);
        y += MessageIcon + 16;
        var title = Text.Label(message.Title, PleyType.PageTitle, PleyColors.Fg, width: PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal);
        title.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(y));
        body.Add(title);
        y += PleyType.PageTitle.Line + 8;
        words.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(y));
        body.Add(words);
        y += wordsHeight + 24;
        if (message.ActionLabel is not { } action) return;
        var buttons = new ButtonRow();
        buttons.Add(new ButtonView(action, ButtonKind.Pill, () =>
        {
            if (message.Action == MessageAction.Back) Shell.Back?.Invoke();
            else model.Retry();
        }));
        buttons.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(y));
        buttons.Group.Exit = d => d == Direction.Left && Scrolling.Left(gate, Shell.FocusRail);
        body.Add(buttons);
        Group.Add(buttons.Group);
        if (Shell.Top == this) Tree.Enter(buttons.Group);
    }

    /// <summary>The photo's 160 dp circle on Surface2; the picture fades in over it once it has come.</summary>
    sealed class PhotoView : View
    {
        public PhotoView()
        {
            Size = new Size(Ui.Dp(PhotoSize), Ui.Dp(PhotoSize));
            CornerRadius = Ui.Dp(PhotoSize / 2);
            ClippingMode = ClippingModeType.ClipChildren;
            BackgroundColor = PleyColors.Surface2;
        }

        public void Picture(string? file)
        {
            // Gone already: the page moved on before the picture came.
            if (file == null || Disposed) return;
            var px = Ui.Dp(PhotoSize);
            var picture = new ImageView { ResourceUrl = file, Size = new Size(px, px), DesiredWidth = (int)px, DesiredHeight = (int)px, FittingMode = FittingModeType.ScaleToFill, Opacity = 0 };
            Add(picture);
            var fade = new Animation(PleyMotion.ImageFadeMs);
            fade.AnimateTo(picture, "Opacity", 1f);
            fade.Play();
        }
    }
}
