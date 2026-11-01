using System;
using System.Linq;
using Pley.Tv.Core;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// A tile's actions (the Android app's ItemActions screen, which TV shows as a side panel): OK held
/// or the Menu key on a tile anywhere. Play, the watched marks, Continue Watching and My List are
/// done at once and the panel closes; the rest open their page. What it says, a play and a failure
/// arrive after the panel closed too.
/// </summary>
static class TileActions
{
    public static void Open(Shell layers, AppState state, long itemId, bool continueWatching, Action<PlayRequest> play, Action<PageKey> go, Action<string> toast)
    {
        var model = new ItemActionsModel(new AppItemSource(state), itemId, new ContextScheduler(), continueWatching);
        model.PlayRequested += play;
        model.Told += e => toast(ItemText.Toast(e).Text);
        IDisposable? watch = null;
        var shown = false;
        watch = model.State.Watch(s =>
        {
            if (shown) return;
            switch (s)
            {
                case ItemActionsState.Ready ready:
                    shown = true;
                    var lines = ready.Actions.Select(a => new SidePanelSheet.Line(ItemActions.Label(a), Icon: ItemActions.Icon(a))).ToList();
                    new SidePanelSheet(layers, ready.Title, lines, 0, i =>
                    {
                        var action = ready.Actions[i];
                        if (!model.Perform(action) && ItemActions.Destination(action, ready.Item) is { } key) go(key);
                        return false;
                    }).Show();
                    // The panel has what it shows: the model only carries on with what was chosen.
                    watch?.Dispose();
                    model.Dispose();
                    break;
                case ItemActionsState.Failed failed:
                    shown = true;
                    toast(Pley.Tv.Core.Api.Errors.Message(failed.Error));
                    watch?.Dispose();
                    model.Dispose();
                    break;
            }
        });
    }
}
