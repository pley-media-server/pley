import type { AppState } from "../../state";
import type { Entry } from "../shell/navigator";
import { AppContext } from "./common";
import { HomeScreen } from "./home";
import { LinkScreen } from "./onboarding/link";
import { ProfilesScreen } from "./onboarding/profiles";
import { ServersScreen } from "./onboarding/servers";
import { LibrariesScreen, LibraryScreen } from "./library";
import { ItemScreen } from "./item";
import { ItemActionsScreen } from "./actions";
import { SearchScreen } from "./search";
import { PersonScreen } from "./person";
import { CollectionScreen, CollectionsScreen } from "./collections";
import { AlbumScreen, ArtistScreen, NowPlayingScreen } from "./music";
import { AddToPlaylistScreen, NewPlaylistScreen, PlaylistScreen, PlaylistsScreen } from "./playlists";
import { DiagnosticsScreen, HistoryScreen, SettingsScreen } from "./settings";
import { DiscoverListScreen, DiscoverScreen } from "./discover";
import { TitleScreen } from "./title";
import { RequestsScreen } from "./requests-page";
import { PlayerScreen } from "../player/player";
import { LivePlayerScreen } from "../player/live";
import { LiveTvScreen } from "./livetv";

/** One page of the back stack, by its key (the Android app's TvEntries). */
export function Screen(p: { entry: Entry; state: AppState }) {
  const key = p.entry.key;
  let page;
  switch (key.name) {
    case "servers":
      page = <ServersScreen />;
      break;
    case "link":
      page = <LinkScreen server={key.server} />;
      break;
    case "profiles":
      page = <ProfilesScreen />;
      break;
    case "home":
      page = <HomeScreen entry={p.entry} />;
      break;
    case "libraries":
      page = <LibrariesScreen entry={p.entry} sort={key.sort} />;
      break;
    case "library":
      page = <LibraryScreen entry={p.entry} id={key.id} tab={key.tab} sort={key.sort} />;
      break;
    case "item":
      page = <ItemScreen entry={p.entry} id={key.id} />;
      break;
    case "actions":
      page = <ItemActionsScreen id={key.id} continueWatching={!!key.continueWatching} />;
      break;
    case "search":
      page = <SearchScreen entry={p.entry} query={key.query} />;
      break;
    case "person":
      page = <PersonScreen entry={p.entry} id={key.id} />;
      break;
    case "collections":
      page = <CollectionsScreen entry={p.entry} libraryId={key.libraryId} />;
      break;
    case "collection":
      page = <CollectionScreen entry={p.entry} id={key.id} />;
      break;
    case "artist":
      page = <ArtistScreen entry={p.entry} id={key.id} />;
      break;
    case "album":
      page = <AlbumScreen entry={p.entry} id={key.id} trackId={key.trackId} />;
      break;
    case "playlists":
      page = <PlaylistsScreen entry={p.entry} />;
      break;
    case "playlist":
      page = <PlaylistScreen entry={p.entry} id={key.id} />;
      break;
    case "addToPlaylist":
      page = <AddToPlaylistScreen itemIds={key.itemIds} title={key.title} />;
      break;
    case "newPlaylist":
      page = <NewPlaylistScreen smart={key.smart} />;
      break;
    case "settings":
      page = <SettingsScreen entry={p.entry} />;
      break;
    case "history":
      page = <HistoryScreen entry={p.entry} />;
      break;
    case "diagnostics":
      page = <DiagnosticsScreen />;
      break;
    case "nowPlaying":
      page = <NowPlayingScreen />;
      break;
    case "discover":
      page = <DiscoverScreen entry={p.entry} query={key.query} />;
      break;
    case "discoverList":
      page = <DiscoverListScreen entry={p.entry} list={key.list} />;
      break;
    case "title":
      page = <TitleScreen key={`${key.kind}:${key.tmdbId}`} entry={p.entry} kind={key.kind} tmdbId={key.tmdbId} pick={key.pick} />;
      break;
    case "requests":
      page = <RequestsScreen entry={p.entry} />;
      break;
    case "player":
      page = <PlayerScreen request={key.request} pressedAt={key.pressedAt} />;
      break;
    case "live":
      page = <LiveTvScreen entry={p.entry} />;
      break;
    case "livePlayer":
      page = <LivePlayerScreen channelId={key.channelId} category={key.category} pressedAt={key.pressedAt} />;
      break;
  }
  return <AppContext.Provider value={p.state}>{page}</AppContext.Provider>;
}
