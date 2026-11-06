using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core;
using Pley.Tv.Core.Local;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Live;
using Pley.Tv.Core.Requests;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Pley.Tv.Tizen.Screens;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen;

/// <summary>Pley for Samsung TV, native: a NUI window the remote drives.</summary>
class App : NUIApplication
{
    Shell shell = null!;
    AppState state = null!;
    Images images = null!;
    KeyRepeatGate gate = null!;
    AppLiveSource live = null!;
    /// <summary>Requests and Discover for the profile in use: the rail's way in, its number, and the news that pops up.</summary>
    AppRequests requests = null!;
    LiveTvEntry? liveEntry;
    /// <summary>The app's one music player: it plays on behind every page, and ends with the profile, the sign-in or the server.</summary>
    MusicPlayer music = null!;
    MediaKeys? musicKeys;
    /// <summary>The USB drive plugged into the TV, followed as it comes and goes.</summary>
    Drives drives = null!;
    /// <summary>The music was playing as the app went to the background: it plays again when the app is back.</summary>
    bool musicWasPlaying;
    /// <summary>The rail and the pages, while they show: the app takes casts only then, not while signing in or asking who's watching.</summary>
    TvShell? pages;
    /// <summary>The player showing, a film's or a channel's: a cast takes its place.</summary>
    Sheet? player;
    /// <summary>What a cast plays once the player it takes the place of has let go of the TV's player.</summary>
    Action? afterPlayer;
    /// <summary>Casts from the profile's remote, on another of its devices.</summary>
    CastReceiver casts = null!;
    /// <summary>The UI thread's queue: a new language redraws what shows once what changed it has run its course.</summary>
    ContextScheduler ui = null!;
    /// <summary>The language what shows was drawn in: another redraws it (<see cref="Redraw"/>).</summary>
    string? drawnIn;
    /// <summary>The screen of its own that shows while the pages don't: the servers, signing in, Who's watching?.</summary>
    PageKey? shown;
    /// <summary>The code signing in is about, for the link screen to be drawn again with it.</summary>
    LinkModel? link;
    /// <summary>Choose your server, while it shows: drawn again in another language, with the address being typed over it.</summary>
    ServersScreen? servers;
    /// <summary>The globe of the screens before signing in, which picks this TV's language.</summary>
    LanguageGlobe globe = null!;

    protected override void OnCreate()
    {
        base.OnCreate();
        var window = Window.Instance;
        Ui.Measure(window);
        // A profiling build's marks, stalls and frame rate, from here (none in other builds).
        Profile.Begin(window, Current.DirectoryInfo.Data);
        Profile.Mark("app: starting");
        var fonts = Profile.Start();
        Text.LoadFonts();
        Profile.Took("app: fonts", fonts);
        window.BackgroundColor = PleyColors.Bg;
        // NUI draws its own highlight round the view with key focus (a text field while typing),
        // at the field's inner text box, not its edge; the app draws focus itself, as Android does.
        FocusManager.Instance.FocusIndicator = new View();
        gate = new KeyRepeatGate();
        shell = new Shell(window, gate, Exit);
        TizenMediaEngine.Covered = () => shell.Covered;
        images = new Images(Current.DirectoryInfo.Cache);
        var made = Profile.Start();
        // Joining or leaving a network makes the server prove itself again before its next token.
        Core.Auth.ServerProofs.FollowNetworks();
        state = new AppState(Platform.Of());
        Profile.Took("app state", made);
        globe = new LanguageGlobe(state);
        // The language last shown, before the first page; the account's or the server's follows when it is another.
        ApplyLanguage(state.Language.Value);
        ui = new ContextScheduler();
        state.Language.Changed += language =>
        {
            ApplyLanguage(language);
            // Posted: a page that went while a signal's listeners ran would still hear from them. A
            // change is mostly heard in an awaited continuation, whose culture lasts only as long as
            // it does: the UI thread's is set again there.
            ui.Post(() =>
            {
                UseCulture();
                Redraw();
            });
        };
        // Pictures come through the API in use, with its token.
        state.Api.Watch(api => images.Api = api);
        var services = Profile.Start();
        live = new AppLiveSource(state);
        requests = new AppRequests(state, new ContextScheduler());
        // The lineup changed (the server's events, a reconnection): whether Live TV has channels may have too.
        live.OnChange(() => liveEntry?.Refresh());
        drives = new Drives();
        music = AppMusicServer.Player(state, () => new TizenMediaEngine(video: false), () => Platform.Facts(), new ContextScheduler());
        // The remote's media keys drive the music wherever the app is, as Android's media session has
        // them, except under a player, which has its own: its panels and lists over it included, as the
        // TV's media key service reaches every listener whatever is on top.
        musicKeys = new MediaKeys(shell.Input, () => music.Active && !shell.Covered, Media);
        shell.Shown += MusicShown;
        // What the profile's remote on another device sends plays here, as Play would (Core's Player/Cast.cs).
        casts = new CastReceiver(new AppCastServer(state), new Casting(this), new ContextScheduler());
        state.OnEvent(e =>
        {
            if (e.Type == "cast") casts.Take(e);
        });
        Profile.Took("app: Live TV, music and the media keys", services);
        // The server ended the sign-in (revoked, or the refresh token spent): sign in again.
        state.SignedOut.Changed += signedOut =>
        {
            if (signedOut && state.Server.Value != null) Go(new PageKey.Link(state.Server.Value));
        };
        var first = Profile.Start();
        Go(state.Start() switch
        {
            StartPage.Servers => new PageKey.Servers(),
            StartPage.Link => new PageKey.Link(state.Server.Value ?? ""),
            StartPage.Profiles => new PageKey.Profiles(),
            StartPage.Pin => new PageKey.Profiles(Pin: true),
            _ => new PageKey.Home(),
        });
        Profile.Took("app: the first page", first);
    }

    /// <summary>The app went to the background: what plays lets go (Live TV's connection), and the server's events stop.</summary>
    protected override void OnPause()
    {
        base.OnPause();
        Profile.Mark("app: hidden by the TV");
        state.Visible.Value = false;
        shell.Visible(false);
        state.Listen(false);
    }

    /// <summary>Back in front: what was let go plays again, and the server's events are followed again when signed in.</summary>
    protected override void OnResume()
    {
        base.OnResume();
        Profile.Mark("app: shown again");
        // Back from the TV's other apps or standby, maybe on another network.
        Core.Auth.ServerProofs.NetworkMayHaveChanged();
        state.Visible.Value = true;
        shell.Visible(true);
        state.Listen(true);
    }

    /// <summary>The TV was set to another language: the app follows when neither the account nor the server has one.</summary>
    protected override void OnLocaleChanged(global::Tizen.Applications.LocaleChangedEventArgs e)
    {
        base.OnLocaleChanged(e);
        state.TvLanguagesChanged(Platform.Languages());
        // Tizen set the thread's cultures to the TV's own: the language shown's again.
        ApplyLanguage(state.Language.Value);
    }

    /// <summary>The TV's region format changed: dates and numbers follow it when it is in the language shown.</summary>
    protected override void OnRegionFormatChanged(global::Tizen.Applications.RegionFormatChangedEventArgs e)
    {
        base.OnRegionFormatChanged(e);
        ApplyLanguage(state.Language.Value);
    }

    /// <summary>
    /// The words in <paramref name="language"/> from now on, and dates and numbers in its culture
    /// (the TV's region when the TV is in that language), for the code that writes them with the
    /// thread's culture too.
    /// </summary>
    static void ApplyLanguage(string language)
    {
        Strings.Use(language, Platform.CultureTags());
        UseCulture();
    }

    /// <summary>The thread's cultures, and those of threads to come, as the language shown's (<see cref="Strings.Culture"/>).</summary>
    static void UseCulture()
    {
        var culture = Strings.Culture;
        CultureInfo.DefaultThreadCurrentCulture = culture;
        CultureInfo.DefaultThreadCurrentUICulture = culture;
        CultureInfo.CurrentCulture = culture;
        CultureInfo.CurrentUICulture = culture;
    }

    /// <summary>
    /// What shows, again in the language shown and where it was: the pages with their back stack
    /// (Settings on its row), the servers (with the address being typed over them), or the code being
    /// signed in with; after the globe, with the globe focused. A player keeps its
    /// words until it closes, and the pages under it are drawn again then. Signing in once the code
    /// is approved, and Who's watching?, keep theirs for the moment before the next page, which
    /// the profile chosen brings its own language to.
    /// </summary>
    void Redraw()
    {
        if (drawnIn == Strings.Language || player != null) return;
        if (pages is { } tv)
        {
            ShowHome(tv.Keys, (tv.Top as SettingsScreen)?.Spot);
            return;
        }
        switch (shown)
        {
            case PageKey.Servers:
                // Typing an address: open again, with what was typed.
                var typing = servers?.Typing;
                Go(new PageKey.Servers());
                if (typing != null) servers?.ReopenAddress(typing);
                break;
            case PageKey.Link key when link is { Approved: false } model:
                ShowLink(model, key);
                break;
        }
    }

    /// <summary>A screen of its own shows (not the pages), drawn in the language shown now.</summary>
    void Showing(PageKey key)
    {
        shown = key;
        drawnIn = Strings.Language;
    }

    protected override void OnTerminate()
    {
        // The music's session ends with the app, its place reported (AppState.OnLeave).
        state.EndPlayback();
        Profile.End();
        musicKeys?.Dispose();
        drives.Dispose();
        base.OnTerminate();
    }

    /// <summary>
    /// The app went to the background (false) or came back (true). A hidden app on a Samsung TV
    /// gives up the TV's one media pipeline, so the music lets go of it and of its session, keeping
    /// its place, and plays on from there when the app is back.
    /// </summary>
    void MusicShown(bool shown)
    {
        if (!shown)
        {
            musicWasPlaying = music.Now.Value is { } now && (now.Playing || (now.Loading && now.Error == null));
            music.YieldToVideo();
            return;
        }
        if (!musicWasPlaying) return;
        musicWasPlaying = false;
        music.Play();
    }

    /// <summary>A media key for the music: Play, Pause, either, the next or previous track, 10 s back or on, and Stop, which pauses.</summary>
    void Media(MediaAction action)
    {
        var now = music.Now.Value;
        var playing = now != null && (now.Playing || (now.Loading && now.Error == null));
        switch (action)
        {
            case MediaAction.Play:
                if (!playing) music.Play();
                break;
            case MediaAction.Pause:
            case MediaAction.Stop:
                if (playing) music.Pause();
                break;
            case MediaAction.PlayPause:
                music.Toggle();
                break;
            case MediaAction.Next:
                music.Next();
                break;
            case MediaAction.Previous:
                music.Previous();
                break;
            case MediaAction.Rewind:
            case MediaAction.Forward:
                var p = music.Position();
                var to = p.PositionMs + (action == MediaAction.Forward ? NowPlayingModel.SeekMs : -NowPlayingModel.SeekMs);
                music.SeekTo(Math.Max(0, p.DurationMs > 0 ? Math.Min(to, p.DurationMs) : to));
                break;
        }
    }

    /// <summary>Shows the page for <paramref name="key"/>, in place of what is on screen.</summary>
    void Go(PageKey key)
    {
        switch (key)
        {
            case PageKey.Servers:
                var screen = new ServersScreen(shell, new ServersModel(new AppServersSource(state), new ContextScheduler()), globe)
                {
                    Chosen = chosen => Go(chosen.Next),
                };
                servers = screen;
                screen.Gone += () =>
                {
                    if (servers == screen) servers = null;
                };
                shell.Show(screen);
                Showing(key);
                break;
            case PageKey.Link signIn:
                // Asks the server whether it requires two-step sign-in, and so hears its language.
                var model = new LinkModel(new AppLinkSource(state), new ContextScheduler(), signIn.Server);
                model.Done += () => Go(new PageKey.Home());
                link = model;
                ShowLink(model, signIn);
                break;
            case PageKey.Profiles who:
                var profiles = new ProfilesModel(new AppProfilesSource(state), gate: !state.ProfileGatePassed, pin: who.Pin);
                profiles.Done += () =>
                {
                    state.ProfileGatePassed = true;
                    Go(new PageKey.Home());
                };
                profiles.SignedOut += () => Go(new PageKey.Link(state.Server.Value ?? ""));
                shell.Show(new ProfilesScreen(shell, profiles, images, Approval));
                Showing(key);
                _ = profiles.Load();
                break;
            default:
                ShowHome();
                break;
        }
    }

    /// <summary>The link screen on <paramref name="model"/>'s code: a new one, or the same code drawn again in another language.</summary>
    void ShowLink(LinkModel model, PageKey.Link key)
    {
        var screen = new LinkScreen(model, ChangeServer, shell: shell, globe: globe);
        shell.Show(screen);
        Showing(key);
        screen.Started();
    }

    /// <summary>
    /// Home, with the rail; what was saved shows at once, and the server is asked for the rest.
    /// <paramref name="reopen"/>: the pages to show again in their order (another language), and
    /// Settings back at <paramref name="spot"/>.
    /// </summary>
    void ShowHome(IReadOnlyList<PageKey>? reopen = null, (SettingsCategory, string?)? spot = null)
    {
        var made = Profile.Start();
        TvShell? tv = null;
        tv = new TvShell(state,
            item =>
            {
                if (item.Slot == RailSlot.Home) tv!.Restart(Home(tv));
                else if (tv!.PageOf?.Invoke(item.Key) is { } page) tv.Restart(page);
                else tv.Toast(Strings.T("shell.comesNext", ("page", item.Label)));
            },
            () => Go(new PageKey.Profiles()));
        tv.PageOf = key => key switch
        {
            PageKey.Item item => new ItemScreen(tv, shell, new ItemModel(new AppItemSource(state), item.ItemId, new ContextScheduler()), images, gate, k => Go(tv, k),
                new ItemRequestModel(requests, new ContextScheduler(), item.ItemId)),
            PageKey.Home => Home(tv),
            PageKey.Libraries => new LibrariesScreen(tv, shell, new LibrariesModel(new AppLibrarySource(state), new ContextScheduler()), gate, k => Go(tv, k)),
            PageKey.Library library => new LibraryScreen(tv, shell, new LibraryModel(new AppLibrarySource(state), library, new ContextScheduler()), images, gate, k => Go(tv, k)),
            PageKey.Person person => new PersonScreen(tv, new PersonModel(new AppPersonSource(state), person.PersonId, new ContextScheduler()), images, gate, k => Go(tv, k)),
            // A channel found plays over the search, which is there again when it's left.
            PageKey.Search search => new SearchScreen(tv, shell, new SearchModel(new AppSearchSource(state), new ContextScheduler(), search.Query), images, gate, k => Go(tv, k),
                watch => WatchLive(watch, () => { }), new SearchDiscoverModel(requests)),
            // Every movie and show library's collections, whichever library the key names, as the Android TV app has it.
            PageKey.Collections collections => new CollectionsScreen(tv, collections, new CollectionsModel(new AppCollectionSource(state), new ContextScheduler(), only: null), images, gate, k => Go(tv, k)),
            PageKey.Collection collection => new CollectionScreen(tv, new CollectionModel(new AppCollectionSource(state), collection.CollectionId, new ContextScheduler()), images, gate, k => Go(tv, k)),
            // Settings' Watch history row opens it, as on Android TV.
            PageKey.History => new HistoryScreen(tv, new HistoryModel(new AppHistorySource(state), new ContextScheduler()), images, gate, k => Go(tv, k)),
            PageKey.Playlists => new PlaylistsScreen(tv, new PlaylistsModel(new AppPlaylistSource(state), new ContextScheduler()), images, gate, k => Go(tv, k)),
            PageKey.Playlist playlist => new PlaylistScreen(tv, shell, new PlaylistModel(new AppPlaylistSource(state), playlist.PlaylistId, new ContextScheduler()), images, gate, k => Go(tv, k)),
            AddToPlaylistPage add => new AddToPlaylistScreen(tv, shell, new AddToPlaylistModel(new AppPlaylistSource(state), add, new ContextScheduler()), add, images),
            NewPlaylistPage or SaveSmartPlaylistPage => new NewPlaylistScreen(tv, shell, new NewPlaylistModel(new AppPlaylistSource(state), NewPlaylistMode.Of(key)!, new ContextScheduler()), key),
            PageKey.Settings => new SettingsScreen(tv, shell, Settings(), images, k => Go(tv, k), k => Restart(tv, k)),
            // What the TV reports when it asks to play, Settings' passthrough choices with it.
            DiagnosticsPage => new DiagnosticsScreen(tv, new DiagnosticsModel(Platform.Version(), () => Task.FromResult(Platform.Facts()), new ContextScheduler(),
                facts => TizenCaps.Of(facts, state.Settings.Value.Passthrough))),
            PageKey.LiveTv => new LiveTvScreen(tv, shell, new LiveTvModel(live, new ContextScheduler()), images, gate, WatchLive),
            ArtistPage artist => new ArtistScreen(tv, new ArtistModel(new AppItemSource(state), music, new ContextScheduler(), artist.ArtistId), images, gate, k => Go(tv, k)),
            AlbumPage album => new AlbumScreen(tv, new AlbumModel(new AppItemSource(state), music, new ContextScheduler(), album.AlbumId, album.TrackId), images, gate, k => Go(tv, k)),
            PageKey.NowPlaying => new NowPlayingScreen(tv, shell, new NowPlayingModel(music, state.Server.Value ?? ""), images, gate),
            PageKey.Discover => new DiscoverScreen(tv, shell, new DiscoverModel(requests, new ContextScheduler()), images, gate, k => Go(tv, k), k => Restart(tv, k)),
            PageKey.DiscoverList or PageKey.DiscoverResults => new DiscoverGridScreen(tv, shell,
                new DiscoverGridModel(requests, key is PageKey.DiscoverList l ? new DiscoverSource.Row(l.List) : new DiscoverSource.Words(((PageKey.DiscoverResults)key).Query),
                    DiscoverGridScreen.Columns * DiscoverGridScreen.FirstRows), images, gate, k => Go(tv, k), k => Restart(tv, k)),
            PageKey.DiscoverTitle title => new DiscoverTitleScreen(tv, shell, new DiscoverTitleModel(requests, new ContextScheduler(), title), title, images, gate, k => Go(tv, k), k => Restart(tv, k)),
            PageKey.Requests => new RequestsScreen(tv, shell, new RequestsModel(requests, new ContextScheduler()), images, gate, k => Go(tv, k), k => Restart(tv, k)),
            PageKey.Storage storage => new StorageScreen(tv, storage.Path, drives.Root, state.Storage, gate, k => Go(tv, k),
                (path, startMs) => PlayVideo(new PlayRequest(LocalPlayerSource.ItemId, startMs, FromStart: false), localFile: path)),
            _ => null,
        };
        // Live TV in the rail while this profile has channels: asked again for each sign-in and profile.
        // The remotes are told this TV takes channels as long as it does.
        if (liveEntry == null)
        {
            liveEntry = new LiveTvEntry(live, new ContextScheduler());
            liveEntry.Channels.Watch(n => state.TakesLive(n > 0));
        }
        else liveEntry.Refresh();
        tv.Follow(liveEntry);
        // Now Playing in the rail while music has a queue.
        tv.Follow(music.Now);
        // The USB drive in the rail while one is plugged in.
        tv.Follow(drives.Root);
        // Discover, or the requests, in the rail as the summary says, with their number.
        tv.Follow(requests);
        // What became ready to watch pops up over the pages.
        var notices = new RequestNotices(requests, tv, shell, gate, images, () => state.Visible.Value, k => Go(tv, k));
        tv.Gone += notices.Dispose;
        tv.Play = request => PlayVideo(request);
        Profile.Took("app: the shell and its rail", made);
        tv.Actions = (id, continueWatching) => TileActions.Open(shell, state, id, continueWatching, r => tv.Play?.Invoke(r), k => Go(tv, k), text => tv.Toast(text));
        // The pages show: casts are taken until they go (another profile, signing out).
        pages = tv;
        tv.Gone += () =>
        {
            if (pages == tv) pages = null;
        };
        shell.Show(tv);
        shown = null;
        drawnIn = Strings.Language;
        var homeShown = Profile.Start();
        var root = reopen is { Count: > 0 } && reopen[0] is not PageKey.Home ? tv.PageOf?.Invoke(reopen[0]) : null;
        tv.Restart(root ?? Home(tv));
        Profile.Took("Home shown", homeShown);
        foreach (var key in reopen?.Skip(1) ?? Enumerable.Empty<PageKey>()) tv.Open(key);
        if (spot is { } at && tv.Top is SettingsScreen settings) settings.Return(at);
        // Requests are followed from just after the first frame: reading them starts their events, catching up and polling.
        // Home again (a sign-in, a profile chosen), their summary is read again.
        requests.Start();
        // The first film opens without making the TV's first player on the UI thread.
        TizenMediaEngine.Warm();
        _ = state.LoadAccount();
        // The server's language, for a profile without its own.
        _ = state.LoadServer();
        var home = state.LoadHome();
        Profile.Ends("Home's data", home);
        // Signed in now (at start, after a sign-in or a profile chosen): the server's events from here.
        state.Listen(true);
    }

    /// <summary>
    /// A video in the player over everything, from a page, a drive (<paramref name="localFile"/>) or a
    /// cast (<paramref name="playingFrom"/>, shown over it as it starts). While it is on top, Home and the
    /// pages wait; back from it, they ask again (its reports moved resume points and watched marks), and
    /// the item details kept go.
    /// </summary>
    void PlayVideo(PlayRequest request, string? localFile = null, string? playingFrom = null)
    {
        // A video takes the TV's one media pipeline: the music pauses and lets go of its session, keeping its place.
        music.YieldToVideo();
        var opened = Profile.Start();
        state.HoldRefresh(true);
        PlayerScreen? screen = null;
        screen = new PlayerScreen(shell, state, request, state.Reporter, images, gate, () =>
        {
            state.HoldRefresh(false);
            PlayerGone(screen!);
        }, localFile, playingFrom);
        player = screen;
        screen.Show();
        Profile.Took("player shown", opened);
    }

    /// <summary>The live player on <paramref name="watch"/>'s channel, from Live TV or search; <paramref name="back"/> runs as it closes.</summary>
    void WatchLive(LiveWatch watch, Action back) => Watch(watch, back, null);

    /// <summary>The live player on <paramref name="watch"/>'s channel; <paramref name="playingFrom"/>, a cast's, shows over it as it starts.</summary>
    void Watch(LiveWatch watch, Action back, string? playingFrom)
    {
        // A channel takes the TV's one media pipeline: the music lets go of it first.
        music.YieldToVideo();
        LivePlayerScreen? screen = null;
        screen = new LivePlayerScreen(shell, live, watch, images, gate, () =>
        {
            back();
            PlayerGone(screen!);
        }, playingFrom);
        player = screen;
        screen.Show();
    }

    /// <summary>A player went: a cast waiting for it to let go of the TV's player plays now.</summary>
    void PlayerGone(Sheet sheet)
    {
        if (player != sheet) return;
        player = null;
        // The language changed while it played: the pages under it in the new one.
        if (drawnIn != Strings.Language) ui.Post(Redraw);
        var then = afterPlayer;
        afterPlayer = null;
        then?.Invoke();
    }

    /// <summary>
    /// What a cast sent plays in place of the player showing, once that one has let go of the TV's
    /// one media pipeline; at once when none shows.
    /// </summary>
    void Replacing(Action start)
    {
        switch (player)
        {
            case PlayerScreen film:
                afterPlayer = start;
                film.Close();
                break;
            case LivePlayerScreen channel:
                afterPlayer = start;
                channel.Close();
                break;
            default:
                start();
                break;
        }
    }

    /// <summary>The player showing closes: done once it has let go of the TV's player, at once when none shows.</summary>
    Task ClosePlayer()
    {
        var gone = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        Replacing(() => gone.TrySetResult(true));
        return gone.Task;
    }

    /// <summary>
    /// What a cast does here (Core's <see cref="CastReceiver"/> decides what): it plays in place of
    /// what plays, over the rail's pages, with "Playing from …" over it as it starts. A cast from
    /// another profile's remote switches to that profile first, which starts on its Home.
    /// </summary>
    sealed class Casting : ICastTarget
    {
        readonly App app;

        public Casting(App app) => this.app = app;

        public string ReceiverId => app.state.InstallId();

        public bool Shown => app.state.Visible.Value && app.state.SignedIn && !app.state.SignedOut.Value && app.pages != null;

        public long? Profile => app.state.Profile;

        public async Task SwitchTo(long profile, string ticket, Action switched)
        {
            // What plays as the profile before ends while its sign-in still works, as signing out has it.
            await app.ClosePlayer();
            await app.state.SwitchForCast(profile, ticket);
            // As a switch on "Who's watching?": the profile's Home, under what the cast plays.
            app.Go(new PageKey.Home());
            switched();
        }

        public void Video(PlayRequest request, string banner) => app.Replacing(() =>
        {
            if (Shown) app.PlayVideo(request, playingFrom: banner);
        });

        public void Music(IReadOnlyList<Core.Api.ItemSummary> tracks, int at, bool shuffle, long fromMs, string banner) => app.Replacing(() =>
        {
            if (!Shown || app.pages is not { } tv) return;
            app.music.Play(tracks, at, shuffle, fromMs);
            if (tv.Top?.Key is not PageKey.NowPlaying) tv.Open(new PageKey.NowPlaying());
            tv.Toast(banner, Casts.BannerMs);
        });

        public void Channel(long channelId, string banner) => app.Replacing(() =>
        {
            if (Shown) app.Watch(new LiveWatch(channelId, new Core.Api.LiveCategory.All().Key), () => { }, banner);
        });

        public void Say(string text)
        {
            switch (app.player)
            {
                case PlayerScreen film:
                    film.Notice(text, PlayerText.ToastMs);
                    break;
                case LivePlayerScreen channel:
                    channel.Notice(text, PlayerText.ToastMs);
                    break;
                default:
                    app.pages?.Toast(text);
                    break;
            }
        }

        public void Failed(Exception error) => Say(Core.Api.Errors.Message(error));
    }

    /// <summary>Home: a tile opens its item's page, an artist's or album's their music page, a track its album.</summary>
    HomeScreen Home(TvShell tv) => new HomeScreen(tv, new HomeModel(state, new ContextScheduler()), images, gate, item => Go(tv, SearchRules.Opens(item)));

    /// <summary>Settings, with the Me page for its Profile and the TV's facts under "Open diagnostics".</summary>
    SettingsModel Settings() =>
        new SettingsModel(new AppSettingsSource(state, DiagnosticsText.DeviceFacts(Platform.Facts())), new MeModel(new AppMeSource(state), new ContextScheduler()));

    /// <summary>Starts over at <paramref name="key"/>: a page of the shell's, or a screen of its own (Who's watching?, the servers).</summary>
    void Restart(TvShell tv, PageKey key)
    {
        if (tv.PageOf?.Invoke(key) is { } page) tv.Restart(page);
        else Go(key);
    }

    /// <summary>Opens a page from another: an item's, over the one it was opened from; one the app doesn't have yet says so.</summary>
    void Go(TvShell tv, PageKey key)
    {
        if (!tv.Open(key)) tv.Toast(Strings.T("shell.pageComesNext"));
    }

    /// <summary>A code for a phone to approve: a profile that needs its password, or (id 0) any profile.</summary>
    LinkScreen Approval(ProfileTile profile)
    {
        var model = new LinkModel(new AppLinkSource(state, profile.Id), new ContextScheduler(), state.Server.Value ?? "");
        model.Done += () => Go(new PageKey.Home());
        LinkScreen? screen = null;
        screen = profile.Id == 0
            ? new LinkScreen(model, () => shell.Pop(screen!), Strings.T("signin.approve.anyTitle"), Strings.T("signin.approve.anyIntro"), Strings.T("signin.approve.otherProfile"))
            : new LinkScreen(model, () => shell.Pop(screen!), Strings.T("signin.approve.title"),
                Strings.T("signin.approve.intro", ("name", profile.Name)), Strings.T("signin.approve.otherProfile"));
        return screen;
    }

    void ChangeServer()
    {
        state.ForgetServer();
        Go(new PageKey.Servers());
    }

    static void Main(string[] args) => new App().Run(args);
}
