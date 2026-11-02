"""Tiles as pley's window draws them (ArtworkTileModel.kt): what a tile's
two caption lines say, its picture, progress, watched tick or unwatched
count, as list items for the window's rows and grids."""

from __future__ import annotations

import xbmcgui

from . import words
from .listing import image

# What a tile asks the server for, at the size it is drawn (and a little
# more for the focused one's growth): smaller pictures load and decode
# faster, and hold less of a box's memory.
WIDTHS = {"poster": 320, "grid": 320, "square": 320, "landscape": 480, "episode": 480, "wide": 480, "person": 240}
BACKDROP_WIDTH = 1280

PLAYABLE = ("movie", "episode", "extra", "track")
VIDEO = ("movie", "show", "season", "episode")


def picture(base: str, item: dict, shape: str):
    """The picture a tile of `shape` shows of `item` (ArtworkCandidates):
    wide tiles a 16:9 still (a movie's backdrop, as its thumb is a frame
    grab; an episode's still, else its show's backdrop), the rest a poster
    (its own, else its season's or show's)."""
    width = WIDTHS[shape]
    kind = item.get("kind")
    if shape in ("landscape", "episode", "wide"):
        if kind == "movie":
            url = item.get("backdrop_url") or item.get("thumb_url")
        elif kind == "episode":
            url = item.get("thumb_url") or (item.get("grandparent_id") and "/api/items/%d/images/backdrop" % item["grandparent_id"])
        else:
            url = item.get("thumb_url") or item.get("backdrop_url")
        url = url or item.get("poster_url")
    else:
        url = item.get("poster_url")
        for parent in ("parent_id", "grandparent_id"):
            if not url and item.get(parent):
                url = "/api/items/%d/images/poster" % item[parent]
    return image(base, url, width)


def backdrop(base: str, item: dict):
    """An item's backdrop: its own, else its show's (the server falls back)."""
    url = item.get("backdrop_url")
    if not url and item.get("id") is not None and item.get("kind") in VIDEO + ("extra",):
        version = (item.get("poster_url") or item.get("thumb_url") or "").partition("?")[2]
        url = "/api/items/%d/images/backdrop%s" % (item["id"], "?" + version if version else "")
    return image(base, url, BACKDROP_WIDTH)


def captions(item: dict, shape: str) -> tuple:
    """(title, subtitle): an episode tile numbers its title and says its
    runtime or what is left; a wide tile of an episode says its show, its
    number and what is left; the rest their title and card line."""
    left = words.time_left(words.state(item).get("position_ms"), item.get("duration_ms")) if words.in_progress(item) else ""
    title = item.get("title") or ""
    if shape in ("episode", "wide") and item.get("kind") == "episode":
        numbered = "%d. %s" % (item["index"], title) if item.get("index") is not None else title
        return numbered, left or words.runtime(item.get("duration_ms"))
    if shape == "episode" and item.get("kind") == "track":
        # An album's tracks, numbered, with their length (the web's track list).
        numbered = "%d. %s" % (item["index"], title) if item.get("index") is not None else title
        return numbered, words.clock(item.get("duration_ms"))
    if shape == "landscape" and item.get("kind") == "episode":
        show = item.get("grandparent_title") or title
        second = left or (title if show != title else "")
        return show, " · ".join(p for p in (words.episode_label(item.get("parent_index"), item.get("index")), second) if p)
    return title, left or words.card_subtitle(item)


def tile(base: str, item: dict, shape: str) -> xbmcgui.ListItem:
    title, subtitle = captions(item, shape)
    li = xbmcgui.ListItem(title, subtitle, offscreen=True)
    found = picture(base, item, shape)
    if found:
        li.setArt({"tile": found})
    li.setProperty("ph", words.placeholder(title))
    li.setProperty("id", str(item["id"]))
    li.setProperty("kind", item.get("kind") or "")
    count = words.unwatched(item)
    if count:
        li.setProperty("count", str(count))
        li.setProperty("countsize", str(min(len(str(count)), 3)))
    elif words.state(item).get("watched"):
        li.setProperty("state", "watched")
    if words.in_progress(item):
        # ListItem.IsResumable and .PercentPlayed draw the progress bar.
        tag = li.getVideoInfoTag()
        tag.setResumePoint(words.state(item)["position_ms"] / 1000.0, item["duration_ms"] / 1000.0)
    return li


def person(base: str, credit: dict) -> xbmcgui.ListItem:
    """A face of Cast & Crew: the name, and the part played or the job."""
    li = xbmcgui.ListItem(credit.get("name") or "", credit.get("character") or credit.get("job") or "", offscreen=True)
    found = image(base, credit.get("photo_url"), WIDTHS["person"])
    if found:
        li.setArt({"tile": found})
    li.setProperty("ph", words.placeholder(credit.get("name") or ""))
    li.setProperty("id", str(credit.get("id")))
    li.setProperty("kind", "person")
    return li


def discover(base: str, title: dict, subtitle: str) -> xbmcgui.ListItem:
    """A title found on TMDB (search's Discover row): its poster, through the
    server as library artwork is, its title and `subtitle` (its badge, or
    its year and kind)."""
    name = title.get("title") or ""
    li = xbmcgui.ListItem(name, subtitle, offscreen=True)
    found = image(base, title.get("poster_url"), WIDTHS["poster"])
    if found:
        li.setArt({"tile": found})
    li.setProperty("ph", words.placeholder(name))
    li.setProperty("kind", "tmdb")
    return li


def see_all(text: str) -> xbmcgui.ListItem:
    """The tile at the end of a library's row that opens the library (or of
    search's Discover row, that opens Discover's results for the words, or asks
    TMDB again)."""
    li = xbmcgui.ListItem(text, offscreen=True)
    li.setProperty("seeall", "1")
    li.setProperty("kind", "seeall")
    return li


def row_shape(hub: dict, library_kind=None) -> str:
    """A row's tiles, as the web's hubAspect: Continue Watching as stills,
    music as squares, the rest by their first item."""
    if hub.get("key", "").startswith("continue"):
        return "landscape"
    if library_kind == "music":
        return "square"
    first = (hub.get("items") or [{}])[0].get("kind")
    if first == "episode":
        return "landscape"
    if first in ("artist", "album", "track"):
        return "square"
    return "poster"


def see_all_of(hub: dict):
    """The library a row's "See all" opens (recently added, released, top
    rated), or None."""
    if hub.get("library_id") is None:
        return None
    if any(hub.get("key", "").startswith(p) for p in ("recently_added", "recently_released", "top_rated")):
        return hub["library_id"]
    return None


def hero_of(rows: list):
    """The item Home's hero shows: the first of Continue Watching, else the
    newest video in the rows, else the newest item."""
    for row in rows:
        if row["key"].startswith("continue") and row["items"]:
            return row["items"][0]
    every = [item for row in rows for item in row["items"]]
    videos = [item for item in every if item.get("kind") in VIDEO]
    pool = videos or every
    return max(pool, key=lambda item: item.get("added_at") or 0) if pool else None
