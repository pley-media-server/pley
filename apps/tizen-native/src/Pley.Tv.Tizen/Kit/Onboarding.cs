using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Kit;

/// <summary>The sign-in screens' pieces (the Android app's OnboardingKit.kt).</summary>
static class Onboarding
{
    /// <summary>
    /// The sign-in screens' background (design spec §2.8): BrandDeep at 18 % glowing from the
    /// bottom-right corner, gone by 60 % of the width. A square centred on the corner, its radial
    /// gradient drawn once; the window cuts off what lies beyond it.
    /// </summary>
    public static View BrandGlow()
    {
        var radius = Ui.Dp(PleyTv.CanvasWidth * 0.6f);
        var stops = new PropertyArray();
        stops.Add(new PropertyValue(0f));
        stops.Add(new PropertyValue(1f));
        var colors = new PropertyArray();
        colors.Add(new PropertyValue(new Color(PleyColors.BrandDeep.R, PleyColors.BrandDeep.G, PleyColors.BrandDeep.B, 0.18f)));
        colors.Add(new PropertyValue(new Color(PleyColors.BrandDeep.R, PleyColors.BrandDeep.G, PleyColors.BrandDeep.B, 0f)));
        var map = new PropertyMap();
        map.Add(Visual.Property.Type, new PropertyValue((int)Visual.Type.Gradient));
        map.Add(GradientVisualProperty.Center, new PropertyValue(new Vector2(0, 0)));
        map.Add(GradientVisualProperty.Radius, new PropertyValue(0.5f));
        map.Add(GradientVisualProperty.StopOffset, new PropertyValue(stops));
        map.Add(GradientVisualProperty.StopColor, new PropertyValue(colors));
        return new View
        {
            Size = new Size(radius * 2, radius * 2),
            Position = new Position(Ui.Dp(PleyTv.CanvasWidth) - radius, Ui.Dp(PleyTv.CanvasHeight) - radius),
            Background = map,
        };
    }

    /// <summary>The Pley mark on its tile and the wordmark, <paramref name="height"/> dp tall (40 on the link screen).</summary>
    public static View Logo(float height = 40)
    {
        var px = Ui.DpInt(height);
        var row = new View { Size = new Size(0, px) };
        var mark = new ImageView
        {
            ResourceUrl = $"{(global::Tizen.Applications.Application.Current.DirectoryInfo.SharedResource)}pley-mark.svg",
            Size = new Size(px, px),
            DesiredWidth = px,
            DesiredHeight = px,
        };
        var word = new TextLabel("Pley")
        {
            PixelSize = Ui.Dp(height * 0.75f),
            TextColor = PleyColors.Fg,
            FontFamily = PleyType.Roboto,
            FontStyle = Text.Weight(PleyType.Bold),
            VerticalAlignment = VerticalAlignment.Center,
            Position = new Position(px + Ui.Dp(height * 0.15f), 0),
        };
        word.Size = new Size(word.NaturalSize.Width + 2, px);
        row.SizeWidth = word.PositionX + word.SizeWidth;
        row.Add(mark);
        row.Add(word);
        return row;
    }

    /// <summary>Width in dp of a view made here.</summary>
    public static float WidthDp(View view) => view.SizeWidth / Ui.Scale;
}

/// <summary>
/// The 8 dp Accent dot of "Looking for Pley…": it breathes between 40 % and 100 % in 500 ms steps
/// (100, 70, 40, 70), redrawn twice a second only (design spec §6).
/// </summary>
sealed class BreathingDot : View
{
    static readonly float[] Steps = { 1f, 0.7f, 0.4f, 0.7f };
    readonly Timer timer = new Timer(500);
    int step;

    public BreathingDot()
    {
        Size = new Size(Ui.Dp(8), Ui.Dp(8));
        CornerRadius = Ui.Dp(4);
        BackgroundColor = PleyColors.Accent;
        timer.Tick += (s, e) =>
        {
            Opacity = Steps[++step % Steps.Length];
            return true;
        };
        timer.Start();
    }

    protected override void Dispose(global::Tizen.NUI.DisposeTypes type)
    {
        if (Disposed) return;
        timer.Stop();
        timer.Dispose();
        base.Dispose(type);
    }
}

/// <summary>A status line: the breathing dot when waiting, then its text, two lines at most.</summary>
sealed class StatusLine : View
{
    readonly TextLabel label;
    BreathingDot? dot;
    readonly float width;

    public StatusLine(float width, TextStyle? style = null, Color? color = null)
    {
        this.width = width;
        label = Text.Label("", style ?? PleyType.Label, color ?? PleyColors.Muted, maxLines: 2, width: width);
        label.VerticalAlignment = VerticalAlignment.Top;
        Size = new Size(Ui.Dp(width), label.SizeHeight);
        Add(label);
    }

    public void Set(string text, bool waiting = false, Color? color = null)
    {
        label.Text = text;
        if (color != null) label.TextColor = color;
        if (waiting && dot == null)
        {
            dot = new BreathingDot { Position = new Position(0, Ui.Dp(PleyType.Label.Line / 2 - 4)) };
            Add(dot);
        }
        else if (!waiting && dot != null)
        {
            dot.Unparent();
            dot.Dispose();
            dot = null;
        }
        label.PositionX = waiting ? Ui.Dp(20) : 0;
        label.SizeWidth = Ui.Dp(width - (waiting ? 20 : 0));
    }
}

/// <summary>The sweep's 160 × 4 bar, filling over the 5 s a sweep takes at most.</summary>
sealed class SweepBar : View
{
    readonly View fill;
    Animation? run;

    public SweepBar()
    {
        Size = new Size(Ui.Dp(160), Ui.Dp(4));
        CornerRadius = Ui.Dp(2);
        BackgroundColor = PleyColors.Surface4;
        fill = new View { Size = new Size(0, Ui.Dp(4)), CornerRadius = Ui.Dp(2), BackgroundColor = PleyColors.Accent };
        Add(fill);
    }

    public void Run(int ms)
    {
        run?.Stop();
        fill.SizeWidth = 0;
        run = new Animation(ms) { DefaultAlphaFunction = new AlphaFunction(AlphaFunction.BuiltinFunctions.Linear) };
        run.AnimateTo(fill, "SizeWidth", Ui.Dp(160));
        run.Play();
    }
}
