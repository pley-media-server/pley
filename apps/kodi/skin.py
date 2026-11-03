"""Writes pley's window for Kodi (resources/skins/default/1080i/pley-main.xml)
from its layout (resources/lib/pley/layout.py), which the add-on's window
code reads too:

    python apps/kodi/skin.py           # writes it
    python apps/kodi/skin.py --check   # fails if it isn't what this writes

One window holds every screen, each a group the add-on shows alone, under
one rail and one backdrop, as the Android TV app's shell does: going from a
screen to another moves no window, and the rail and backdrop stay put.
Kodi reads nothing of a window but its XML, so the many rows, tiles and
buttons that repeat are written here from one description of each.
"""

from __future__ import annotations

import os
import sys
import xml.etree.ElementTree as ET

HERE = os.path.dirname(os.path.abspath(__file__))
ADDON = os.path.join(HERE, "plugin.video.pley")
sys.path.insert(0, os.path.join(ADDON, "resources", "lib"))

from pley import layout as L  # noqa: E402

SKIN = os.path.join(ADDON, "resources", "skins", "default", "1080i")
OUT = os.path.join(SKIN, L.WINDOW)

FADE = "effect=fade time=200"


# ------------------------------------------------------------ elements


def el(tag: str, text=None, children=(), **attrs) -> ET.Element:
    e = ET.Element(tag, {k.rstrip("_"): str(v) for k, v in attrs.items() if v is not None})
    if text is not None:
        e.text = str(text)
    for child in children:
        if child is not None:
            e.append(child)
    return e


def anim(kind: str, spec: str, condition=None) -> ET.Element:
    """An animation: `spec` is "effect=fade time=200 …"."""
    attrs = dict(part.split("=", 1) for part in spec.split())
    if condition:
        attrs["condition"] = condition
    return el("animation", kind, **attrs)


def fades(time: int = 200):
    return [anim("Visible", "effect=fade start=0 end=100 time=%d" % time), anim("Hidden", "effect=fade start=100 end=0 time=%d" % time)]


def faded_unless(mode: str) -> ET.Element:
    """Faded out while the page isn't in `mode` (pley.mode): the hero and
    the info block stay in the window, so focus can go to the hero's
    buttons from the rows, and Kodi draws what they say from the start."""
    return anim("Conditional", "effect=fade start=100 end=0 time=200", "!" + is_("pley.mode", mode))


def control(kind: str, id_=None, left=None, top=None, width=None, height=None, visible=None, animations=(), children=(), **props):
    """A control: its type, place, visibility and animations, then
    `props` as child elements in order (a dict value gives attributes:
    {"text": …, "attrs": {…}}), then `children`."""
    c = el("control", type=kind, id=id_)
    for name, value in (("left", left), ("top", top), ("width", width), ("height", height)):
        if value is not None:
            c.append(el(name, value))
    if visible:
        c.append(el("visible", visible))
    for a in animations:
        c.append(a)
    for name, value in props.items():
        if value is None:
            continue
        tag = name.rstrip("_")
        if isinstance(value, dict):
            c.append(el(tag, value.get("text"), **value.get("attrs", {})))
        elif isinstance(value, ET.Element):
            c.append(value)
        else:
            c.append(el(tag, value))
    for child in children:
        if child is not None:
            c.append(child)
    return c


def tex(name: str, **attrs) -> dict:
    return {"text": name, "attrs": attrs}


def image(left, top, width, height, texture, id_=None, color=None, visible=None, animations=(), border=None, diffuse=None, aspect=None, fade=None, background=None):
    if diffuse and aspect == "scale":
        # A picture that fills its place is cropped, and Kodi stretches a
        # mask over the whole picture unless told otherwise: a portrait
        # photo's circle was a tall oval cut flat at the top and bottom.
        # The mask keeps to the place, as the picture is seen.
        aspect = {"text": aspect, "attrs": {"scalediffuse": "false"}}
    return control(
        "image",
        id_,
        left,
        top,
        width,
        height,
        visible,
        animations,
        texture=tex(texture, colordiffuse=color, border=border, diffuse=diffuse, background=background),
        aspectratio=aspect,
        fadetime=fade,
    )


def label(left, top, width, height, font, color, text=None, id_=None, align="left", aligny="center", wrap=False, visible=None, animations=()):
    return control(
        "label",
        id_,
        left,
        top,
        width,
        height,
        visible,
        animations,
        font=font,
        textcolor=color,
        align=align,
        aligny=aligny,
        wrapmultiline="true" if wrap else None,
        scroll="false",
        label=text if text is not None else "",
    )


def textbox(left, top, width, height, font, color, id_=None, visible=None):
    return control("textbox", id_, left, top, width, height, visible, font=font, textcolor=color, autoscroll="false")


def nav(onup=None, ondown=None, onleft=None, onright=None) -> dict:
    return {"onup": onup, "ondown": ondown, "onleft": onleft, "onright": onright}


def button(id_, left, top, width, height, focus, nofocus, text="", font=L.FONT_LABEL, color=L.FG, focused=L.ON_ACCENT, offset=40, align="center", visible=None, animations=(), border=14, auto=None, navigation=None, enable=None, nofocus_color=None):
    """A button. `auto`: (min, max) to size it to its label."""
    c = control(
        "button",
        id_,
        left,
        top,
        None,
        height,
        visible,
        animations,
        texturefocus=tex(focus, border=border, colordiffuse=L.ACCENT if focus == "round12.png" else None) if focus else {"text": ""},
        texturenofocus=tex(nofocus, border=border, colordiffuse=nofocus_color) if nofocus else {"text": ""},
        font=font,
        textcolor=color,
        focusedcolor=focused,
        disabledcolor=color,
        align=align,
        aligny="center",
        textoffsetx=offset,
        label=text,
        enable=enable,
        **(navigation or {}),
    )
    c.append(el("width", "auto", min=auto[0], max=auto[1]) if auto else el("width", width))
    return c


def group(id_, left, top, width, height, children, visible=None, animations=()):
    return control("group", id_, left, top, width, height, visible, animations, children=children)


def grouplist(id_, left, top, width, height, children, orientation="horizontal", gap=0, align=None, visible=None, navigation=None, animations=()):
    # A round button is a group (the button and its label). Kodi moves focus
    # by the focused control's own navigation, which a grouplist gives only
    # its children: the button inside gets its group's, the neighbours by id
    # (a hidden one passes focus on along the grouplist's chain).
    ids = [child.get("id") for child in children if child is not None]
    around = navigation or {}
    before, after = ("onleft", "onright") if orientation == "horizontal" else ("onup", "ondown")
    across = ("onup", "ondown") if orientation == "horizontal" else ("onleft", "onright")
    for i, child in enumerate(c for c in children if c is not None):
        inner = child.find("control[@type='button']") if child.get("type") == "group" else None
        if inner is None:
            continue
        moves = {before: ids[i - 1] if i else around.get(before), after: ids[i + 1] if i + 1 < len(ids) else around.get(after)}
        moves.update({tag: around.get(tag) for tag in across})
        for tag, target in moves.items():
            if target is not None and inner.find(tag) is None:
                el_ = ET.SubElement(inner, tag)
                el_.text = str(target)
    return control(
        "grouplist",
        id_,
        left,
        top,
        width,
        height,
        visible,
        animations,
        orientation=orientation,
        itemgap=gap,
        align=align,
        usecontrolcoords="true",
        children=children,
        **(navigation or {}),
    )


def prop(name: str) -> str:
    return "Window.Property(%s)" % name


def is_(name: str, value: str) -> str:
    return "String.IsEqual(%s,%s)" % (prop(name), value)


# Each shape's mask, as big as its tiles.
MASKS = {
    "poster": "mask-poster.png",
    "landscape": "mask-landscape.png",
    "episode": "mask-episode.png",
    "wide": "mask-episode.png",
    "grid": "mask-grid.png",
    "square": "mask-square.png",
    "person": "mask-circle.png",
    "channel": "mask-episode.png",
}

# The tiles that can show an unwatched count (shows, seasons), and those a
# library's row can end with "See all" in: the others' layouts go without.
COUNTED = ("poster", "grid", "square")
SEE_ALL = ("poster", "square")

ZOOM_TILE = "effect=zoom start=100 end=108 time=150 tween=cubic easing=out center=auto"
ZOOM_PILL = "effect=zoom start=100 end=105 time=150 tween=cubic easing=out center=auto"
ZOOM_ROUND = "effect=zoom start=100 end=108 time=150 tween=cubic easing=out center=auto"


# ---------------------------------------------------------- the pieces


def backdrop() -> list:
    """The backdrop (TvBackdrop): the picture, the scrim from the left, and
    one from below for each place the page can be."""

    def bottom(id_, mode, start, end):
        return group(
            id_,
            0,
            0,
            L.WIDTH,
            L.HEIGHT,
            [
                image(0, start, L.WIDTH, end - start, "fade-down.png", color=L.BG),
                image(0, end, L.WIDTH, L.HEIGHT - end, "pley-white.png", color=L.BG),
            ],
            visible=is_("pley.scrim", mode),
            animations=fades(300),
        )

    return [
        group(
            1000,
            0,
            0,
            L.WIDTH,
            L.HEIGHT,
            [
                image(0, 0, L.WIDTH, L.HEIGHT, "$INFO[%s]" % prop("pley.backdrop"), id_=L.BACKDROP, aspect=el("aspectratio", "scale", aligny="top"), fade=300, background="true"),
                image(0, 0, 1600, L.HEIGHT, "scrim-left.png", color=L.BG),
                bottom(L.SCRIM_HERO, "hero", 440, 820),
                bottom(L.SCRIM_ROWS, "rows", 240, 600),
                bottom(L.SCRIM_ITEM, "item", 520, 1000),
            ],
            visible="!String.IsEmpty(%s)" % prop("pley.backdrop.on"),
            animations=fades(300),
        )
    ]


def meta_line(line: int, left: int, top: int) -> ET.Element:
    """The metadata line: year · rating badge · runtime · ★ 7.4 · badges."""
    pieces = []
    for i in range(L.META_PIECES):
        dot, star, text, badge = L.meta_piece(line, i)
        pieces += [
            control("label", dot, width="auto", height=36, font=L.FONT_LABEL, textcolor=L.SUBTLE, aligny="center", label="·"),
            image(None, 4, 28, 28, "icon-star.png", id_=star, color=L.ACCENT),
            control("label", text, width="auto", height=36, font=L.FONT_LABEL, textcolor=L.MUTED, aligny="center", label=""),
            button(badge, None, 0, None, 36, None, "badge.png", font=L.FONT_BADGE, color=L.MUTED, offset=12, border=6, auto=(36, 400), enable="false"),
        ]
    return group(line, left, top, 1500, 36, [grouplist(line + 1, 0, 0, 1500, 36, pieces, gap=14)])


def round_button(id_, icon: str, text: str, navigation=None) -> ET.Element:
    """A round button (TvRoundButton) and its label, which fades in under it
    while it has focus."""
    return group(
        L.button_box(id_),
        None,
        None,
        L.BUTTON,
        L.BUTTON + L.BUTTON_LABEL_ROOM,
        [
            button(
                id_,
                0,
                0,
                L.BUTTON,
                L.BUTTON,
                "button-%s-focus.png" % icon,
                "button-%s.png" % icon,
                border=0,
                animations=[anim("Focus", ZOOM_ROUND)],
                navigation=navigation,
            ),
            label(-110, L.BUTTON + 12, 300, 34, L.FONT_CAPTION, L.FG, text, id_=L.button_label(id_), align="center", visible="Control.HasFocus(%d)" % id_, animations=fades(150)),
        ],
    )


def pill(id_, icon: str | None, text="", navigation=None) -> ET.Element:
    """A pill (TvPillButton): with an icon, its label is pushed past it."""
    if icon:
        focus, rest, border = "pill-%s-focus.png" % icon, "pill-%s.png" % icon, "80,0,40,0"
    else:
        focus, rest, border = "pill-focus.png", "pill.png", "14"
    return button(
        id_,
        None,
        0,
        None,
        L.BUTTON,
        focus,
        rest,
        text,
        offset=40,
        align="left",
        border=border,
        auto=(256, 900),
        animations=[anim("Focus", ZOOM_PILL)],
        navigation=navigation,
    )


def tile_parts(shape: L.Shape, list_id: int, focused: bool) -> list:
    """What a tile draws (ArtworkTile): its placeholder and title, its art,
    progress, watched tick or unwatched count, and the caption under it.
    Focused, it grows and takes the ring, and its title is bold."""
    w, h, m = shape.width, shape.height, L.LIST_MARGIN
    mask = MASKS[shape.name]
    plain = "String.IsEmpty(ListItem.Property(seeall))"
    has_focus = "Control.HasFocus(%d)" % list_id
    if shape.name == "channel":
        # A logo, whole, on the dark well; the channel's name without one.
        no_logo = "String.IsEmpty(ListItem.Art(tile))"
        well = [
            image(0, 0, w, h, "round8.png", color=L.SURFACE2, border=10),
            control("textbox", left=16, top=h // 2 - 46, width=w - 32, height=92, visible=no_logo, font=L.FONT_TITLE, textcolor=L.FG, align="center", autoscroll="false", label="$INFO[ListItem.Label]"),
            image(32, 24, w - 64, h - 48, "$INFO[ListItem.Art(tile)]", aspect="keep", fade=150, background="true"),
        ]
    else:
        well = [
            image(0, 0, w, h, "$INFO[ListItem.Property(ph)]", diffuse=mask, visible=plain),
            # The title over the placeholder, on two lines (list labels keep to one).
            control("textbox", left=16, top=h // 2 - 46, width=w - 32, height=92, visible=plain, font=L.FONT_TITLE, textcolor=L.FG, align="center", autoscroll="false", label="$INFO[ListItem.Label]"),
            image(0, 0, w, h, "$INFO[ListItem.Art(tile)]", diffuse=mask, aspect="scale", fade=150, background="true", visible=plain),
        ]
    if not shape.round:
        well += [
            image(0, 0, w, h, "hairline.png", border=10, visible=plain),
            control(
                "progress",
                left=0,
                top=h - 8,
                width=w,
                height=8,
                visible="ListItem.IsResumable",
                texturebg=tex("pley-white.png", colordiffuse="99000000"),
                midtexture=tex("pley-white.png", colordiffuse=L.ACCENT),
                info="ListItem.PercentPlayed",
            ),
            image(w - 52, 12, 40, 40, "tick.png", visible="String.IsEqual(ListItem.Property(state),watched)"),
        ]
    if shape.name in COUNTED:
        # Shows' and seasons' unwatched episodes, as wide as their digits.
        for digits, width in ((1, 40), (2, 52), (3, 64)):
            shown = "String.IsEqual(ListItem.Property(countsize),%d)" % digits
            well += [
                image(w - 12 - width, 12, width, 36, "round4.png", color=L.ACCENT, border=6, visible=shown),
                label(w - 12 - width, 12, width, 36, L.FONT_BADGE, L.ON_ACCENT, "$INFO[ListItem.Property(count)]", align="center", visible=shown),
            ]
    if shape.name in SEE_ALL:
        # "See all", the last tile of a library's rows.
        see_all = "!" + plain
        well += [
            image(0, 0, w, h, "round8.png", color=L.SURFACE2, border=10, visible=see_all),
            image((w - 48) // 2, h // 2 - 64, 48, 48, "icon-arrow-right.png", color=L.FG, visible=see_all),
            label(0, h // 2, w, 48, L.FONT_TITLE, L.FG, "$INFO[ListItem.Label]", align="center", visible=see_all),
        ]
    if focused:
        ring = image(-12, -12, w + 24, h + 24, "ring-circle.png" if shape.round else "ring.png", border=None if shape.round else 24, visible=has_focus)
        tile = group(None, 0, m, w, h, well + [ring], animations=[anim("Focus", ZOOM_TILE)])
    else:
        tile = group(None, 0, m, w, h, well)
    top = m + h + L.TILES_TO_CAPTION
    caption = []
    align = "center" if shape.round else "left"
    # Captions take most of the gap too: Estuary's Noto Sans runs wider than
    # the Android app's Roboto.
    cw = w if shape.round else w + shape.gap - 10
    if focused:
        caption += [
            label(0, top, cw, 34, L.FONT_CAPTION_BOLD, L.FG, "$INFO[ListItem.Label]", align=align, visible=has_focus + " + " + plain),
            label(0, top, cw, 34, L.FONT_CAPTION, L.FG, "$INFO[ListItem.Label]", align=align, visible="!" + has_focus + " + " + plain),
        ]
    else:
        caption.append(label(0, top, cw, 34, L.FONT_CAPTION, L.FG, "$INFO[ListItem.Label]", align=align, visible=plain))
    caption.append(label(0, top + 34, cw, 34, L.FONT_CAPTION, L.MUTED, "$INFO[ListItem.Label2]", align=align, visible=plain))
    return [tile] + caption


def row_list(list_id: int, shape: L.Shape, navigation: dict) -> ET.Element:
    """A row of tiles (TvRow): focus stays at the page's edge and the row
    moves under it, the tile before peeking out under the rail. A season's
    episodes (TvEpisodes) start at the first, and focus moves along them
    until the row has to scroll."""
    stride = L.stride(shape)
    height = L.item_height(shape)
    if shape.name == "episode":
        margin = L.LIST_MARGIN
        return control(
            "list",
            list_id,
            L.GUTTER - margin,
            L.ROW_HEADER + L.HEADER_TO_TILES - L.LIST_MARGIN,
            L.WIDTH - L.GUTTER + margin,
            height,
            orientation="horizontal",
            scrolltime={"text": "200", "attrs": {"tween": "cubic", "easing": "out"}},
            itemlayout=el("itemlayout", None, shift(tile_parts(shape, list_id, False), margin), width=stride, height=height),
            focusedlayout=el("focusedlayout", None, shift(tile_parts(shape, list_id, True), margin), width=stride, height=height),
            **navigation,
        )
    return control(
        "fixedlist",
        list_id,
        L.GUTTER - stride,
        L.ROW_HEADER + L.HEADER_TO_TILES - L.LIST_MARGIN,
        L.WIDTH - L.GUTTER + stride,
        height,
        orientation="horizontal",
        focusposition="1",
        movement="0",
        scrolltime={"text": "200", "attrs": {"tween": "cubic", "easing": "out"}},
        itemlayout=el("itemlayout", None, tile_parts(shape, list_id, False), width=stride, height=height),
        focusedlayout=el("focusedlayout", None, tile_parts(shape, list_id, True), width=stride, height=height),
        **navigation,
    )


def rows(base: int, slots: int, shapes: tuple, first_up: str, left: str = str(L.RAIL)) -> list:
    """Row slots, which the window fills and places: a header and a list
    for each shape a row may take. Up and down go from row to row (Kodi
    skips the empty ones), Left to the rail."""
    out = []
    for i in range(slots):
        up = str(L.slot(base, i - 1)) if i else first_up
        down = str(L.slot(base, i + 1)) if i + 1 < slots else None
        navigation = nav(onup=up, ondown=down, onleft=left)
        children = [label(L.GUTTER, 0, 1500, L.ROW_HEADER, L.FONT_SECTION, L.FG, id_=L.slot_header(base, i))]
        for name in shapes:
            children.append(row_list(L.slot_list(base, i, name), L.SHAPES[name], navigation))
        out.append(control("group", L.slot(base, i), 0, L.HEIGHT, L.WIDTH, 600, children=children, **navigation))
    return out


def grid(base: int, top: int, navigation: dict) -> list:
    """Grids (TvPosterGrid): one for each shape, the window showing one."""
    out = []
    for name in L.GRID_SHAPES:
        shape = L.GRID[name]
        stride = L.stride(shape)
        height = L.item_height(shape) + 26
        list_id = L.grid_list(base, name)
        out.append(
            control(
                "panel",
                list_id,
                L.GUTTER - 12,
                top - L.LIST_MARGIN,
                L.WIDTH - L.GUTTER - L.SAFE_X + 24,
                L.HEIGHT - top + L.LIST_MARGIN,
                orientation="vertical",
                scrolltime={"text": "200", "attrs": {"tween": "cubic", "easing": "out"}},
                itemlayout=el("itemlayout", None, shift(tile_parts(shape, list_id, False), 12), width=stride, height=height),
                focusedlayout=el("focusedlayout", None, shift(tile_parts(shape, list_id, True), 12), width=stride, height=height),
                **navigation,
            )
        )
    return out


def shift(controls: list, dx: int) -> list:
    """`controls` moved `dx` right, inside a panel's left margin."""
    return [group(None, dx, 0, None, None, controls)]


# ------------------------------------------------------------- screens


def rail() -> list:
    """The rail (TvRail): collapsed, icons with the current page's marked;
    with focus, open over the page with each entry's name."""
    entries = [
        (L.RAIL_SEARCH, "search", "icon-search.png", "$ADDON[plugin.video.pley 30003]"),
        (L.RAIL_HOME, "home", "icon-house.png", "$ADDON[plugin.video.pley 30100]"),
        (L.RAIL_LIVE, "live", "icon-tv-minimal.png", "$ADDON[plugin.video.pley 30247]"),
    ]
    entries += [(e, "lib%d" % i, "icon-folder.png", "") for i, e in enumerate(L.RAIL_LIBRARIES)]
    entries += [
        (L.RAIL_ALL_LIBRARIES, "libraries", "icon-layers.png", "$ADDON[plugin.video.pley 30207]"),
        (L.RAIL_PLAYLISTS, "playlists", "icon-list-music.png", "$ADDON[plugin.video.pley 30004]"),
        (L.RAIL_COLLECTIONS, "collections", "icon-layers.png", "$ADDON[plugin.video.pley 30005]"),
        (L.RAIL_SETTINGS, "settings", "icon-settings.png", "$ADDON[plugin.video.pley 30027]"),
    ]
    order = [L.RAIL_AVATAR] + [e for e, _, _, _ in entries]
    opened = "ControlGroup(%d).HasFocus" % L.RAIL
    out = [
        image(0, 0, 544, L.HEIGHT, "pley-white.png", color="F512151A", visible=opened, animations=fades(150)),
        image(544, 0, 48, L.HEIGHT, "shadow.png", color="FF000000", visible=opened, animations=fades(150)),
    ]

    def links(entry):
        i = order.index(entry)
        return nav(onup=str(order[i - 1]) if i else None, ondown=str(order[i + 1]) if i + 1 < len(order) else None)

    # The avatar: the profile's initial on its colour, or its picture; open,
    # its name and the server. The window colours the circle and the initial
    # itself: Kodi reads a colour's $INFO in the window on top, a dialog's
    # while one is open.
    avatar = L.RAIL_AVATAR
    focused = "Control.HasFocus(%d)" % avatar
    out.append(
        group(
            L.rail_group(avatar),
            0,
            L.RAIL_AVATAR_Y - 50,
            544,
            100,
            [
                button(avatar, 8, 0, 528, 100, "round12.png", None, animations=[anim("Focus", ZOOM_PILL)], navigation=links(avatar)),
                image(39, 18, 64, 64, "circle.png", id_=L.RAIL_COLOR, color=L.SURFACE3),
                label(39, 18, 64, 64, L.FONT_TITLE, L.FG, "", id_=L.RAIL_INITIAL, align="center"),
                image(39, 18, 64, 64, "$INFO[%s]" % prop("pley.avatar.picture"), id_=L.RAIL_PICTURE, diffuse="mask-circle.png", aspect="scale", background="true"),
                label(128, 12, 400, 44, L.FONT_BODY_LARGE, L.FG, "$INFO[%s]" % prop("pley.avatar.name"), id_=L.RAIL_NAME, visible=opened + " + !" + focused, animations=fades(150)),
                label(128, 52, 400, 34, L.FONT_CAPTION, L.MUTED, "$INFO[%s]" % prop("pley.avatar.server"), id_=L.RAIL_SUBTITLE, visible=opened + " + !" + focused, animations=fades(150)),
                label(128, 12, 400, 44, L.FONT_BODY_LARGE, L.ON_ACCENT, "$INFO[%s]" % prop("pley.avatar.name"), visible=focused),
                label(128, 52, 400, 34, L.FONT_CAPTION, L.ON_ACCENT, "$INFO[%s]" % prop("pley.avatar.server"), visible=focused),
            ],
        )
    )
    # Each entry's group is a pill tall; the window places them (setup_rail),
    # and the icon and name stay in its middle when it makes pills shorter.
    row = L.RAIL_ROW
    middle = (row - 40) // 2
    y = L.RAIL_TOP
    for entry, key, icon, text in entries:
        if entry == L.RAIL_SETTINGS:
            y = L.RAIL_SETTINGS_Y
        focused = "Control.HasFocus(%d)" % entry
        current = is_("pley.rail", key)
        rest_icon, current_icon, focus_icon = L.rail_icons(entry)
        rest_label, current_label, focus_label = L.rail_labels(entry)
        out.append(
            group(
                L.rail_group(entry),
                0,
                y - row // 2,
                544,
                row,
                [
                    image(8, 0, 528, row, "round12.png", id_=L.rail_pill(entry), color=L.SURFACE3, border=14, visible=opened + " + " + current + " + !" + focused),
                    image(16, middle, 8, 40, "round4.png", color=L.ACCENT, border=4, visible="!" + opened + " + " + current),
                    button(entry, 8, 0, 528, row, "round12.png", None, animations=[anim("Focus", ZOOM_PILL)], navigation=links(entry)),
                    image(48, middle, 40, 40, icon, id_=rest_icon, color=L.MUTED, visible="!%s + !%s" % (focused, current)),
                    image(48, middle, 40, 40, icon, id_=current_icon, color=L.FG, visible="!%s + %s" % (focused, current)),
                    image(48, middle, 40, 40, icon, id_=focus_icon, color=L.ON_ACCENT, visible=focused),
                    label(113, 0, 400, row, L.FONT_BODY_LARGE, L.MUTED, text, id_=rest_label, visible="%s + !%s + !%s" % (opened, focused, current), animations=fades(150)),
                    label(113, 0, 400, row, L.FONT_BODY_LARGE, L.FG, text, id_=current_label, visible="%s + !%s + %s" % (opened, focused, current), animations=fades(150)),
                    label(113, 0, 400, row, L.FONT_BODY_LARGE, L.ON_ACCENT, text, id_=focus_label, visible=focused),
                ],
            )
        )
        y += L.RAIL_PITCH
    # "Who's watching?" is before any page: no rail.
    return [group(L.RAIL, 0, 0, 600, L.HEIGHT, out, visible="!" + is_("pley.screen", "profiles"))]


def home() -> ET.Element:
    hero_buttons = L.HOME_BUTTONS
    to_rows = str(L.slot(L.HOME_ROWS, 0))
    buttons = grouplist(
        hero_buttons,
        0,
        0,
        1500,
        L.BUTTON + L.BUTTON_LABEL_ROOM,
        [
            pill(L.HOME_PLAY, "play"),
            round_button(L.HOME_FROM_START, "rotate-ccw", "$ADDON[plugin.video.pley 30101]"),
            round_button(L.HOME_INFO, "info", "$ADDON[plugin.video.pley 30102]"),
            round_button(L.HOME_WATCHED, "circle-check", "$ADDON[plugin.video.pley 30019]"),
            round_button(L.HOME_UNWATCHED, "circle-check", "$ADDON[plugin.video.pley 30020]"),
        ],
        gap=L.BUTTON_GAP,
        navigation=nav(ondown=to_rows, onleft=str(L.RAIL)),
    )
    hero = group(
        L.HOME_HERO,
        0,
        0,
        L.WIDTH,
        L.HEIGHT,
        [
            label(L.GUTTER, 104, L.TEXT_WIDTH, 164, L.FONT_HERO, L.FG, id_=L.HOME_TITLE, wrap=True, aligny="top"),
            label(L.GUTTER, 268, L.TEXT_WIDTH, 44, L.FONT_TITLE, L.FG, id_=L.HOME_CONTEXT),
            meta_line(L.HOME_META, L.GUTTER, 330),
            label(L.GUTTER, 380, L.TEXT_WIDTH, 34, L.FONT_CAPTION, L.FG, id_=L.HOME_LEFT),
            textbox(L.GUTTER, 430, L.TEXT_WIDTH, 126, L.FONT_BODY_LARGE, L.FG_85, id_=L.HOME_SUMMARY),
            group(L.HOME_BUTTONS_BOX, L.GUTTER, 600, 1500, L.BUTTON + L.BUTTON_LABEL_ROOM, [buttons]),
        ],
        animations=[faded_unless("hero")],
    )
    info = group(
        L.HOME_INFO_BLOCK,
        0,
        0,
        L.WIDTH,
        340,
        [
            label(L.GUTTER, 92, L.TEXT_WIDTH, 68, L.FONT_PAGE, L.FG, id_=L.HOME_INFO_TITLE),
            meta_line(L.HOME_INFO_META, L.GUTTER, 180),
            textbox(L.GUTTER, 232, L.TEXT_WIDTH, 80, L.FONT_BODY, L.FG_85, id_=L.HOME_INFO_SUMMARY),
        ],
        animations=[faded_unless("rows")],
    )
    return group(
        L.SCREENS["home"],
        0,
        0,
        L.WIDTH,
        L.HEIGHT,
        [hero, info] + rows(L.HOME_ROWS, L.HOME_SLOTS, ("poster", "landscape", "square"), str(hero_buttons)),
        visible=is_("pley.screen", "home"),
    )


def item() -> ET.Element:
    buttons = grouplist(
        L.ITEM_BUTTONS,
        0,
        0,
        1600,
        L.BUTTON + L.BUTTON_LABEL_ROOM,
        [
            pill(L.ITEM_PLAY, "play"),
            round_button(L.ITEM_SHUFFLE, "shuffle", "$ADDON[plugin.video.pley 30214]"),
            round_button(L.ITEM_FROM_START, "rotate-ccw", "$ADDON[plugin.video.pley 30101]"),
            round_button(L.ITEM_WATCHED, "circle-check", "$ADDON[plugin.video.pley 30019]"),
            round_button(L.ITEM_UNWATCHED, "eye-off", "$ADDON[plugin.video.pley 30020]"),
            round_button(L.ITEM_ADD_TO_LIST, "bookmark-plus", "$ADDON[plugin.video.pley 30021]"),
            round_button(L.ITEM_IN_LIST, "bookmark-check", "$ADDON[plugin.video.pley 30103]"),
            round_button(L.ITEM_TRAILER, "film", "$ADDON[plugin.video.pley 30104]"),
            round_button(L.ITEM_ADD_TO_PLAYLIST, "list-plus", "$ADDON[plugin.video.pley 30180]"),
            round_button(L.ITEM_MORE, "ellipsis", "$ADDON[plugin.video.pley 30105]"),
        ],
        gap=L.BUTTON_GAP,
        navigation=nav(ondown=str(L.ITEM_SEASONS), onleft=str(L.RAIL)),
    )
    hero = group(
        L.ITEM_HERO,
        0,
        0,
        L.WIDTH,
        L.HEIGHT,
        [
            label(L.GUTTER, 104, L.TEXT_WIDTH, 164, L.FONT_HERO, L.FG, id_=L.ITEM_TITLE, wrap=True, aligny="top"),
            label(L.GUTTER, 268, L.TEXT_WIDTH, 44, L.FONT_TITLE, L.FG, id_=L.ITEM_CONTEXT),
            meta_line(L.ITEM_META, L.GUTTER, 330),
            label(L.GUTTER, 380, L.TEXT_WIDTH, 34, L.FONT_CAPTION, L.FG, id_=L.ITEM_LEFT),
            label(L.GUTTER, 420, L.TEXT_WIDTH, 34, L.FONT_CAPTION, L.MUTED, id_=L.ITEM_GENRES),
            textbox(L.GUTTER, 470, L.TEXT_WIDTH, 126, L.FONT_BODY_LARGE, L.FG_85, id_=L.ITEM_SUMMARY),
            label(L.GUTTER, 560, L.TEXT_WIDTH, 34, L.FONT_CAPTION, L.FG, id_=L.ITEM_CREDITS),
            group(L.ITEM_BUTTONS_BOX, L.GUTTER, 600, 1600, L.BUTTON + L.BUTTON_LABEL_ROOM, [buttons]),
        ],
        animations=[faded_unless("hero")],
    )
    info = group(
        L.ITEM_INFO_BLOCK,
        0,
        0,
        L.WIDTH,
        430,
        [
            label(L.GUTTER, 60, L.TEXT_WIDTH, 84, L.FONT_HERO, L.FG, id_=L.ITEM_INFO_TITLE),
            meta_line(L.ITEM_INFO_META, L.GUTTER, 166),
            label(L.GUTTER, 228, L.TEXT_WIDTH, 44, L.FONT_TITLE, L.FG, id_=L.ITEM_INFO_EPISODE),
            label(L.GUTTER, 278, L.TEXT_WIDTH, 40, L.FONT_LABEL, L.MUTED, id_=L.ITEM_INFO_LINE),
            textbox(L.GUTTER, 216, L.TEXT_WIDTH, 80, L.FONT_BODY_LARGE, L.FG_85, id_=L.ITEM_INFO_SUMMARY),
        ],
        animations=[faded_unless("rows")],
    )
    # The season tabs: a season's name on nothing, the chosen one on
    # surface3, the focused one on the accent.
    tab = lambda focused: [  # noqa: E731
        image(0, 0, 232, 80, "round12.png", color=L.SURFACE3, border=14, visible="ListItem.IsSelected" if not focused else "ListItem.IsSelected + !Control.HasFocus(%d)" % L.ITEM_SEASONS),
        image(0, 0, 232, 80, "round12.png", color=L.ACCENT, border=14, visible="Control.HasFocus(%d)" % L.ITEM_SEASONS) if focused else None,
        label(0, 0, 232, 80, L.FONT_LABEL, L.MUTED, "$INFO[ListItem.Label]", align="center", visible="!ListItem.IsSelected" if not focused else "!ListItem.IsSelected + !Control.HasFocus(%d)" % L.ITEM_SEASONS),
        label(0, 0, 232, 80, L.FONT_LABEL, L.FG, "$INFO[ListItem.Label]", align="center", visible="ListItem.IsSelected" if not focused else "ListItem.IsSelected + !Control.HasFocus(%d)" % L.ITEM_SEASONS),
        label(0, 0, 232, 80, L.FONT_LABEL, L.ON_ACCENT, "$INFO[ListItem.Label]", align="center", visible="Control.HasFocus(%d)" % L.ITEM_SEASONS) if focused else None,
    ]
    seasons = control(
        "list",
        L.ITEM_SEASONS,
        L.GUTTER,
        L.HEIGHT,
        1600,
        80,
        orientation="horizontal",
        scrolltime="200",
        itemlayout=el("itemlayout", None, tab(False), width=248, height=80),
        focusedlayout=el("focusedlayout", None, tab(True), width=248, height=80),
        **nav(onup=str(L.ITEM_BUTTONS), ondown=str(L.slot(L.ITEM_ROWS, 0)), onleft=str(L.RAIL)),
    )
    return group(
        L.SCREENS["item"],
        0,
        0,
        L.WIDTH,
        L.HEIGHT,
        [hero, info, seasons] + rows(L.ITEM_ROWS, L.ITEM_SLOTS, ("poster", "landscape", "episode", "person", "square"), str(L.ITEM_SEASONS)),
        visible=is_("pley.screen", "item"),
    )


def library() -> ET.Element:
    """A library (TvLibraryScreen): its name, the Recommended, Library and
    Collections tabs, and on Library its sort, filters and grid."""
    down = nav(ondown=str(L.LIBRARY_CONTROLS), onleft=str(L.RAIL))
    tabs = []
    for id_, text in ((L.LIBRARY_RECOMMENDED, 30106), (L.LIBRARY_ALL, 30107), (L.LIBRARY_COLLECTIONS, 30005)):
        # Each tab twice: as it is drawn at rest, and chosen (on surface3); the
        # window shows one.
        for chosen in (False, True):
            tabs.append(
                button(
                    id_ + (100 if chosen else 0),
                    None,
                    0,
                    None,
                    80,
                    "round12.png",
                    "round12.png" if chosen else None,
                    "$ADDON[plugin.video.pley %d]" % text,
                    font=L.FONT_BODY_LARGE,
                    color=L.FG if chosen else L.MUTED,
                    offset=32,
                    auto=(80, 600),
                    animations=[anim("Focus", ZOOM_PILL)],
                    navigation=down,
                    nofocus_color=L.SURFACE3 if chosen else None,
                )
            )

    def chip(id_, icon, text):
        if icon:
            return button(id_, None, 0, None, 80, "chip-%s-focus.png" % icon, "chip-%s.png" % icon, text, offset=32, align="left", border="80,0,32,0", auto=(160, 700), animations=[anim("Focus", ZOOM_PILL)])
        return button(id_, None, 0, None, 80, "round12.png", "chip.png", text, offset=32, auto=(80, 700), animations=[anim("Focus", ZOOM_PILL)])

    controls = grouplist(
        L.LIBRARY_CONTROLS,
        L.GUTTER,
        160,
        1640,
        80,
        [
            chip(L.LIBRARY_SORT, "arrow-up-narrow-wide", ""),
            chip(L.LIBRARY_SORT_DOWN, "arrow-down-wide-narrow", ""),
            chip(L.LIBRARY_FILTER, "sliders-horizontal", ""),
            chip(L.LIBRARY_UNWATCHED, None, "$ADDON[plugin.video.pley 30109]"),
            chip(L.LIBRARY_UNWATCHED_ON, "check", ""),
            chip(L.LIBRARY_GENRE, None, "$ADDON[plugin.video.pley 30110]"),
            chip(L.LIBRARY_GENRE_ON, "check", ""),
            chip(L.LIBRARY_YEAR, None, "$ADDON[plugin.video.pley 30111]"),
            chip(L.LIBRARY_YEAR_ON, "check", ""),
            control("label", L.LIBRARY_COUNT, width="auto", height=80, font=L.FONT_LABEL, textcolor=L.MUTED, aligny="center", label=""),
        ],
        gap=20,
        navigation=nav(onup=str(L.LIBRARY_TABS), ondown=str(L.grid_list(L.LIBRARY_GRID, "grid")), onleft=str(L.RAIL)),
    )
    content = [
        label(L.GUTTER, 64, 900, 68, L.FONT_PAGE, L.FG, id_=L.LIBRARY_TITLE),
        grouplist(L.LIBRARY_TABS, 1000, 58, L.WIDTH - L.SAFE_X - 1000, 80, tabs, gap=16, align="right", navigation=down),
        group(None, 0, 0, L.WIDTH, L.HEIGHT, [controls], visible=is_("pley.library.tab", "all")),
        group(
            None,
            0,
            0,
            L.WIDTH,
            L.HEIGHT,
            grid(L.LIBRARY_GRID, 272, nav(onup=str(L.LIBRARY_CONTROLS), onleft=str(L.RAIL))),
            visible="!" + is_("pley.library.tab", "recommended"),
        ),
        label(L.GUTTER, 320, 1400, 120, L.FONT_BODY_LARGE, L.MUTED, id_=L.LIBRARY_EMPTY, wrap=True, aligny="top"),
        group(
            None,
            0,
            0,
            L.WIDTH,
            L.HEIGHT,
            rows(L.LIBRARY_ROWS, L.LIBRARY_SLOTS, ("poster", "landscape", "square"), str(L.LIBRARY_TABS)),
            visible=is_("pley.library.tab", "recommended"),
        ),
    ]
    return group(L.SCREENS["library"], 0, 0, L.WIDTH, L.HEIGHT, content, visible=is_("pley.screen", "library"))


def search() -> ET.Element:
    field = L.SEARCH_FIELD
    return group(
        L.SCREENS["search"],
        0,
        0,
        L.WIDTH,
        L.HEIGHT,
        [
            button(
                field,
                L.GUTTER,
                54,
                1040,
                88,
                "field-focus.png",
                "field.png",
                "",
                font=L.FONT_BODY_LARGE,
                color=L.FG,
                focused=L.FG,
                offset=96,
                align="left",
                border=10,
                navigation=nav(ondown=str(L.slot(L.SEARCH_ROWS, 0)), onleft=str(L.RAIL)),
            ),
            image(L.GUTTER + 32, 78, 40, 40, "icon-search.png", color=L.MUTED, visible="!Control.HasFocus(%d)" % field),
            image(L.GUTTER + 32, 78, 40, 40, "icon-search.png", color=L.FG, visible="Control.HasFocus(%d)" % field),
            label(L.GUTTER, 240, 1400, 48, L.FONT_TITLE, L.FG, id_=L.SEARCH_EMPTY_TITLE),
            label(L.GUTTER, 296, 1120, 100, L.FONT_BODY, L.MUTED, id_=L.SEARCH_EMPTY_BODY, wrap=True, aligny="top"),
        ]
        + rows(L.SEARCH_ROWS, L.SEARCH_SLOTS, ("poster", "episode", "person", "square", "channel"), str(field)),
        visible=is_("pley.screen", "search"),
    )


def grid_page() -> ET.Element:
    return group(
        L.SCREENS["grid"],
        0,
        0,
        L.WIDTH,
        L.HEIGHT,
        [
            label(L.GUTTER, 64, 1500, 68, L.FONT_PAGE, L.FG, id_=L.GRID_TITLE),
            label(L.GUTTER, 138, 1500, 40, L.FONT_BODY, L.MUTED, id_=L.GRID_SUBTITLE),
        ]
        + grid(L.GRID_LISTS, 232, nav(onleft=str(L.RAIL)))
        + [label(L.GUTTER, 240, 1400, 120, L.FONT_BODY_LARGE, L.MUTED, id_=L.GRID_EMPTY, wrap=True, aligny="top")],
        visible=is_("pley.screen", "grid"),
    )


def live_row(focused: bool) -> list:
    """A channel in Live TV's list: its logo on a dark box (its number or
    name without one), its number and name, what's on now, the time it
    airs, what is left, and how far into it the channel is. Focused, on the
    accent with on-accent text, as a library's card; while focus is
    elsewhere on the page, on surface2."""
    w, h = L.LIVE_LIST_WIDTH, L.LIVE_ROW
    lw, lh = L.LIVE_LOGO
    has_focus = "Control.HasFocus(%d)" % L.LIVE_LIST
    no_logo = "String.IsEmpty(ListItem.Art(thumb))"
    favourite = "String.IsEqual(ListItem.Property(favorite),1)"
    out = []
    fg, muted = L.FG, L.MUTED
    if focused:
        out += [
            image(0, 4, w, h - 8, "round12.png", color=L.ACCENT, border=14, visible=has_focus),
            image(0, 4, w, h - 8, "round12.png", color=L.SURFACE2, border=14, visible="!" + has_focus),
        ]
    top = (h - lh) // 2
    out += [
        image(16, top, lw, lh, "round8.png", color=L.BG if focused else L.SURFACE2, border=10),
        image(28, top + 10, lw - 24, lh - 20, "$INFO[ListItem.Art(thumb)]", aspect="keep", fade=150, background="true"),
        label(16, top, lw, lh, L.FONT_TITLE, L.FG, "$INFO[ListItem.Property(short)]", align="center", visible=no_logo),
    ]
    x = 16 + lw + 24
    tw = w - x - 24
    # Each line twice: as it is drawn on the accent, and elsewhere.
    lines = [
        (lambda c: label(x, 14, tw - 48, 34, L.FONT_LABEL, c, "$INFO[ListItem.Label]"), muted),
        (lambda c: image(w - 24 - 28, 18, 28, 28, "icon-star.png", color=c, visible=favourite), L.ACCENT),
        (lambda c: label(x, 46, tw, 42, L.FONT_TITLE, c, "$INFO[ListItem.Label2]"), fg),
        (lambda c: label(x + 176, 84, tw - 176, 32, L.FONT_CAPTION, c, "$INFO[ListItem.Property(when)]", visible="ListItem.IsResumable"), muted),
        (lambda c: label(x, 84, tw, 32, L.FONT_CAPTION, c, "$INFO[ListItem.Property(when)]", visible="!ListItem.IsResumable"), muted),
    ]
    for make, color in lines:
        if focused:
            on_accent = make(L.ON_ACCENT)
            elsewhere = make(color)
            for piece, shown in ((on_accent, has_focus), (elsewhere, "!" + has_focus)):
                found = piece.find("visible")
                if found is None:
                    piece.append(el("visible", shown))
                else:
                    found.text = "%s + %s" % (found.text, shown)
                out.append(piece)
        else:
            out.append(make(color))
    bar = lambda bg, mid, shown: control(  # noqa: E731
        "progress",
        left=x,
        top=97,
        width=160,
        height=6,
        visible=shown,
        texturebg=tex("pley-white.png", colordiffuse=bg),
        midtexture=tex("pley-white.png", colordiffuse=mid),
        info="ListItem.PercentPlayed",
    )
    if focused:
        out += [bar("33000000", L.ON_ACCENT, "ListItem.IsResumable + " + has_focus), bar("33FFFFFF", L.ACCENT, "ListItem.IsResumable + !" + has_focus)]
    else:
        out.append(bar("33FFFFFF", L.ACCENT, "ListItem.IsResumable"))
    return out


def live_category(focused: bool) -> list:
    """A category in Live TV's first column: its name (a region's groups
    set in under it) and how many channels it has; a region's chevron says
    whether it is open. The one shown is on surface3; focused, the accent
    with on-accent text, as a rail entry."""
    w, h = L.LIVE_CATEGORY_WIDTH, L.LIVE_CATEGORY_ROW
    has_focus = "Control.HasFocus(%d)" % L.LIVE_CATEGORIES
    chosen = "String.IsEqual(ListItem.Property(chosen),1)"
    inner = "String.IsEqual(ListItem.Property(inner),1)"
    region = "!String.IsEmpty(ListItem.Property(region))"
    lit = has_focus if focused else "false"
    out = [
        image(0, 4, w, h - 8, "round12.png", color=L.SURFACE3, border=14, visible="%s + !%s" % (chosen, lit) if focused else chosen),
    ]
    if focused:
        out.append(image(0, 4, w, h - 8, "round12.png", color=L.ACCENT, border=14, visible=has_focus))
    for on_accent in ((False, True) if focused else (False,)):
        shown = (has_focus if on_accent else "!" + has_focus) if focused else None
        fg = L.ON_ACCENT if on_accent else L.FG
        muted = L.ON_ACCENT if on_accent else L.MUTED

        def when(condition):
            return " + ".join(c for c in (condition, shown) if c)

        out += [
            label(24, 0, w - 156, h, L.FONT_LABEL, fg, "$INFO[ListItem.Label]", visible=when("!" + inner)),
            label(56, 0, w - 188, h, L.FONT_LABEL, fg, "$INFO[ListItem.Label]", visible=when(inner)),
            label(w - 140, 0, 116, h, L.FONT_CAPTION, muted, "$INFO[ListItem.Label2]", align="right", visible=when("!" + region)),
            label(w - 172, 0, 116, h, L.FONT_CAPTION, muted, "$INFO[ListItem.Label2]", align="right", visible=when(region)),
            image(w - 48, (h - 28) // 2, 28, 28, "icon-chevron-right.png", color=muted, visible=when("String.IsEqual(ListItem.Property(region),closed)")),
            image(w - 48, (h - 28) // 2, 28, 28, "icon-chevron-down.png", color=muted, visible=when("String.IsEqual(ListItem.Property(region),open)")),
        ]
    return out


def live_page() -> ET.Element:
    """Live TV (livetv.py), as the web's LiveTvPage lays it out: the
    categories, the channels of the one shown by number with
    what's on now, and beside them what the focused channel shows now and
    next, which Kodi reads from the list's item as focus moves."""
    scroll = {"text": "200", "attrs": {"tween": "cubic", "easing": "out"}}
    cw, ch = L.LIVE_CATEGORY_WIDTH, L.LIVE_CATEGORY_ROW
    categories = control(
        "list",
        L.LIVE_CATEGORIES,
        L.LIVE_CATEGORY_X,
        L.LIVE_TOP,
        cw,
        ch * L.LIVE_CATEGORY_ROWS,
        orientation="vertical",
        scrolltime=scroll,
        itemlayout=el("itemlayout", None, live_category(False), width=cw, height=ch),
        focusedlayout=el("focusedlayout", None, live_category(True), width=cw, height=ch),
        **nav(onleft=str(L.RAIL), onright=str(L.LIVE_LIST)),
    )
    width, height = L.LIVE_LIST_WIDTH, L.LIVE_ROW
    channels = control(
        "list",
        L.LIVE_LIST,
        L.LIVE_LIST_X,
        L.LIVE_TOP,
        width,
        height * L.LIVE_ROWS,
        orientation="vertical",
        scrolltime=scroll,
        itemlayout=el("itemlayout", None, live_row(False), width=width, height=height),
        focusedlayout=el("focusedlayout", None, live_row(True), width=width, height=height),
        **nav(onleft=str(L.LIVE_CATEGORIES)),
    )
    x, pw, ph = L.LIVE_PANEL_X, L.LIVE_PANEL_WIDTH, L.LIVE_PICTURE_HEIGHT
    item = "Container(%d).ListItem" % L.LIVE_LIST
    picture = "%s.Property(picture)" % item
    y = L.LIVE_TOP
    panel = group(
        L.LIVE_PANEL,
        0,
        0,
        L.WIDTH,
        L.HEIGHT,
        [
            image(x, y, pw, ph, "round12.png", color=L.SURFACE2, border=14),
            image(x + pw // 4, y + ph // 4, pw // 2, ph // 2, "$INFO[%s.Art(thumb)]" % item, aspect="keep", fade=150, background="true", visible="String.IsEmpty(%s)" % picture),
            label(x + 24, y, pw - 48, ph, L.FONT_TITLE, L.FG, "$INFO[%s.Label]" % item, align="center", wrap=True, visible="String.IsEmpty(%s) + String.IsEmpty(%s.Art(thumb))" % (picture, item)),
            image(x, y, pw, ph, "$INFO[%s]" % picture, diffuse="mask-landscape.png", aspect="scale", fade=150, background="true", visible="!String.IsEmpty(%s)" % picture),
            label(x, y + ph + 20, pw, 88, L.FONT_TITLE, L.FG, "$INFO[%s.Property(title)]" % item, wrap=True, aligny="top"),
            label(x, y + ph + 112, pw, 68, L.FONT_CAPTION, L.MUTED, "$INFO[%s.Property(airs)]" % item, wrap=True, aligny="top"),
            control(
                "textbox",
                left=x,
                top=y + ph + 188,
                width=pw,
                height=210,
                font=L.FONT_BODY,
                textcolor=L.FG_85,
                autoscroll="false",
                label="$INFO[%s.Property(description)]" % item,
            ),
            label(x, y + ph + 414, pw, 34, L.FONT_CAPTION, L.SUBTLE, "$ADDON[plugin.video.pley 30257]", visible="!String.IsEmpty(%s.Property(next))" % item),
            label(x, y + ph + 450, pw, 40, L.FONT_LABEL, L.FG, "$INFO[%s.Property(next)]" % item),
        ],
        visible="Integer.IsGreater(Container(%d).NumItems,0)" % L.LIVE_LIST,
    )
    return group(
        L.SCREENS["live"],
        0,
        0,
        L.WIDTH,
        L.HEIGHT,
        [
            label(L.GUTTER, 64, L.LIVE_LIST_X - L.GUTTER, 68, L.FONT_PAGE, L.FG, "$ADDON[plugin.video.pley 30247]", id_=L.LIVE_TITLE),
            label(L.LIVE_LIST_X + 16, 72, width - 16, 56, L.FONT_SECTION, L.FG, id_=L.LIVE_HEADING),
            label(L.LIVE_LIST_X + 16, 124, width - 16, 36, L.FONT_CAPTION, L.MUTED, id_=L.LIVE_COUNT),
            categories,
            channels,
            panel,
            label(L.LIVE_LIST_X + 16, 240, width - 16, 120, L.FONT_BODY_LARGE, L.MUTED, id_=L.LIVE_EMPTY, wrap=True, aligny="top"),
        ],
        visible=is_("pley.screen", "live"),
    )


def libraries_page() -> ET.Element:
    """Every library (TvLibrariesScreen): the title, Sort libraries at the
    row's other end, and a card for each library, three to a row. A card
    (LibraryCard) is surface2 with its kind's icon in a box of the text's
    colour at 12 %, its name and how much it holds; focused, the accent with
    on-accent content, at 1.05."""
    width, height = L.LIBRARIES_CARD
    margin = 16  # room for a focused card's zoom inside the list
    list_id = L.LIBRARIES_LIST
    lit = "Control.HasFocus(%d)" % list_id

    def card(shown, surface, fg, faint, box) -> list:
        return [
            image(0, 0, width, height, "round12.png", color=surface, border=14, visible=shown),
            image(36, 40, 80, 80, "round12.png", color=box, border=14, visible=shown),
            image(56, 60, 40, 40, "$INFO[ListItem.Property(icon)]", color=fg, visible=shown),
            label(144, 36, width - 180, 46, L.FONT_TITLE, fg, "$INFO[ListItem.Label]", visible=shown),
            label(144, 84, width - 180, 36, L.FONT_CAPTION, faint, "$INFO[ListItem.Label2]", visible=shown),
        ]

    rest = card(None, L.SURFACE2, L.FG, "BFE9EBEF", "1FE9EBEF")
    focused = card("!" + lit, L.SURFACE2, L.FG, "BFE9EBEF", "1FE9EBEF") + card(lit, L.ACCENT, L.ON_ACCENT, "BF03131F", "1F03131F")
    cell = (width + L.LIBRARIES_GAP, height + L.LIBRARIES_GAP)
    cards = control(
        "panel",
        list_id,
        L.GUTTER - margin,
        L.LIBRARIES_TOP - margin,
        3 * cell[0] + margin,
        L.HEIGHT - L.LIBRARIES_TOP + margin,
        orientation="vertical",
        scrolltime={"text": "200", "attrs": {"tween": "cubic", "easing": "out"}},
        itemlayout=el("itemlayout", None, [group(None, margin, margin, width, height, rest)], width=cell[0], height=cell[1]),
        focusedlayout=el(
            "focusedlayout",
            None,
            [group(None, margin, margin, width, height, focused, animations=[anim("Focus", ZOOM_PILL)])],
            width=cell[0],
            height=cell[1],
        ),
        **nav(onup=str(L.LIBRARIES_BUTTONS), onleft=str(L.RAIL)),
    )
    sort = nav(ondown=str(list_id), onleft=str(L.RAIL))
    return group(
        L.SCREENS["libraries"],
        0,
        0,
        L.WIDTH,
        L.HEIGHT,
        [
            label(L.GUTTER, 64, 1000, 68, L.FONT_PAGE, L.FG, "$ADDON[plugin.video.pley 30207]", id_=L.LIBRARIES_TITLE),
            grouplist(
                L.LIBRARIES_BUTTONS,
                1000,
                58,
                L.WIDTH - L.SAFE_X - 1000,
                L.BUTTON,
                [pill(L.LIBRARIES_SORT, "arrow-up-down", navigation=sort)],
                align="right",
                navigation=sort,
            ),
            cards,
        ],
        visible=is_("pley.screen", "libraries"),
    )


def profiles_page() -> ET.Element:
    """"Who's watching?" (TvProfilesScreen): the title, then a face for each
    profile of the household, round, with its name under it: its picture,
    else its initial on its placeholder, and a lock when it has a PIN.
    Focused, it grows and takes the ring, and its name is bold. Under them,
    what a refused switch said."""
    size, m = L.PROFILE, L.PROFILE_MARGIN
    stride = size + L.PROFILE_GAP
    height = m + size + 24 + 44
    list_id = L.PROFILES_LIST
    lit = "Control.HasFocus(%d)" % list_id
    locked = "!String.IsEmpty(ListItem.Property(locked))"

    def face(focused: bool) -> list:
        circle = [
            image(0, 0, size, size, "$INFO[ListItem.Property(ph)]", diffuse="mask-circle.png"),
            label(0, 0, size, size, L.FONT_HERO, L.FG, "$INFO[ListItem.Property(initial)]", align="center"),
            image(0, 0, size, size, "$INFO[ListItem.Art(tile)]", diffuse="mask-circle.png", aspect="scale", fade=150, background="true"),
            image(144, 144, 56, 56, "circle.png", color=L.SURFACE3, visible=locked),
            image(156, 156, 32, 32, "icon-lock.png", color=L.FG, visible=locked),
        ]
        top = m + size + 24
        if focused:
            circle.append(image(-12, -12, size + 24, size + 24, "ring-circle.png", visible=lit))
            return [
                group(None, m, m, size, size, circle, animations=[anim("Focus", ZOOM_TILE)]),
                label(m - 40, top, size + 80, 44, L.FONT_CAPTION_BOLD, L.FG, "$INFO[ListItem.Label]", align="center", visible=lit),
                label(m - 40, top, size + 80, 44, L.FONT_CAPTION, L.FG, "$INFO[ListItem.Label]", align="center", visible="!" + lit),
            ]
        return [group(None, m, m, size, size, circle), label(m - 40, top, size + 80, 44, L.FONT_CAPTION, L.FG, "$INFO[ListItem.Label]", align="center")]

    faces = control(
        "list",
        list_id,
        L.profiles_left(1) - m,
        L.PROFILES_TOP - m,
        L.WIDTH - 2 * L.SAFE_X + 2 * m,
        height,
        orientation="horizontal",
        scrolltime={"text": "200", "attrs": {"tween": "cubic", "easing": "out"}},
        itemlayout=el("itemlayout", None, face(False), width=stride, height=height),
        focusedlayout=el("focusedlayout", None, face(True), width=stride, height=height),
    )
    return group(
        L.SCREENS["profiles"],
        0,
        0,
        L.WIDTH,
        L.HEIGHT,
        [
            image(0, 0, L.WIDTH, L.HEIGHT, "glow.png"),
            label(0, 220, L.WIDTH, 80, L.FONT_PAGE, L.FG, "$ADDON[plugin.video.pley 30219]", id_=L.PROFILES_TITLE, align="center"),
            faces,
            label(400, 800, L.WIDTH - 800, 100, L.FONT_BODY, L.DANGER_TEXT, id_=L.PROFILES_NOTE, align="center", wrap=True, aligny="top"),
        ],
        visible=is_("pley.screen", "profiles"),
    )


def notice() -> ET.Element:
    return group(
        L.SCREENS["notice"],
        0,
        0,
        L.WIDTH,
        L.HEIGHT,
        [
            label(L.GUTTER, 380, L.WIDTH - L.GUTTER - L.SAFE_X, 68, L.FONT_PAGE, L.FG, id_=L.NOTICE_TITLE, align="center"),
            label(560, 464, 896, 120, L.FONT_BODY, L.MUTED, id_=L.NOTICE_BODY, align="center", wrap=True, aligny="top"),
            grouplist(
                L.NOTICE_BUTTONS,
                L.GUTTER,
                600,
                L.WIDTH - L.GUTTER - L.SAFE_X,
                L.BUTTON,
                [pill(L.NOTICE_FIRST, "refresh-cw"), pill(L.NOTICE_SECOND, None)],
                gap=L.BUTTON_GAP,
                align="center",
                navigation=nav(onleft=str(L.RAIL)),
            ),
        ],
        visible=is_("pley.screen", "notice"),
    )


def toast() -> ET.Element:
    return group(
        L.TOAST,
        L.GUTTER,
        L.HEIGHT - L.SAFE_Y - 76,
        1500,
        76,
        [
            grouplist(
                None,
                0,
                0,
                1500,
                76,
                [button(L.TOAST_LABEL, None, 0, None, 76, None, "round12.png", "", offset=40, auto=(76, 1500), enable="false", nofocus_color=L.SURFACE3)],
            )
        ],
        visible="!String.IsEmpty(%s)" % prop("pley.toast"),
        animations=fades(200),
    )


def window() -> ET.Element:
    w = el("window")
    w.append(el("defaultcontrol", L.HOME_PLAY, always="false"))
    controls = el("controls")
    controls.append(image(0, 0, L.WIDTH, L.HEIGHT, "pley-white.png", color=L.BG))
    for c in backdrop():
        controls.append(c)
    for screen in (home(), item(), library(), search(), grid_page(), live_page(), libraries_page(), notice(), profiles_page()):
        controls.append(screen)
    # Rows pass under the rail: bg to 112, clear from 144.
    railless = "!%s + !%s" % (is_("pley.screen", "notice"), is_("pley.screen", "profiles"))
    controls.append(image(0, 0, 112, L.HEIGHT, "pley-white.png", color=L.BG, visible=railless))
    controls.append(image(112, 0, 32, L.HEIGHT, "fade-left.png", color=L.BG, visible=railless))
    controls.append(image(0, 0, L.WIDTH, L.HEIGHT, "pley-white.png", color=L.DIM, visible="ControlGroup(%d).HasFocus" % L.RAIL, animations=fades(150)))
    for c in rail():
        controls.append(c)
    controls.append(toast())
    w.append(controls)
    return w


def signin() -> ET.Element:
    """Signing in with a code (signin.py), as the Android TV app's link
    screen (TvLinkScreen): the logo, "Sign in to pley", three numbered
    steps, the code in two fields, the notes (a third when the server
    requires two-step sign-in), and what is happening; on the right the QR
    code on white, and Change server."""

    def step(id_, number_id, number, top, text, text_id=None):
        return group(
            id_,
            L.GUTTER,
            top,
            1000,
            56,
            [
                image(0, 0, 56, 56, "round8.png", color=L.SURFACE3, border=10),
                # $NUMBER: a bare number would be read as one of Kodi's strings.
                label(0, 0, 56, 56, L.FONT_LABEL, L.FG, "$NUMBER[%d]" % number, id_=number_id, align="center"),
                label(84, 0, 900, 56, L.FONT_BODY_LARGE, L.FG, text, id_=text_id),
            ],
        )

    code_box = lambda left, id_: [  # noqa: E731
        image(left, 592, 300, 152, "round12.png", color=L.SURFACE2, border=14),
        label(left, 592, 300, 152, L.FONT_HERO, L.FG, id_=id_, align="center"),
    ]
    w = el("window")
    w.append(el("defaultcontrol", L.SIGNIN_CHANGE, always="true"))
    controls = el("controls")
    for c in [
        image(0, 0, L.WIDTH, L.HEIGHT, "pley-white.png", color=L.BG),
        image(0, 0, L.WIDTH, L.HEIGHT, "glow.png"),
        image(L.GUTTER, 112, 80, 80, "logo.png"),
        label(L.GUTTER + 112, 112, 400, 80, L.FONT_HERO, L.FG, "Pley"),
        label(L.GUTTER, 248, 1000, 72, L.FONT_PAGE, L.FG, "$ADDON[plugin.video.pley 30171]"),
        step(L.SIGNIN_STEP_SCAN, None, 1, 352, "$ADDON[plugin.video.pley 30172]"),
        step(L.SIGNIN_STEP_GO, L.SIGNIN_NUMBER_GO, 2, 432, "", text_id=L.SIGNIN_ADDRESS),
        step(L.SIGNIN_STEP_ENTER, L.SIGNIN_NUMBER_ENTER, 3, 512, "$ADDON[plugin.video.pley 30174]"),
    ] + code_box(L.GUTTER, L.SIGNIN_CODE_A) + [
        image(L.GUTTER + 328, 664, 40, 8, "round4.png", color=L.SUBTLE, border=4),
    ] + code_box(L.GUTTER + 396, L.SIGNIN_CODE_B) + [
        label(L.GUTTER, 776, 1100, 40, L.FONT_BODY, L.MUTED, "$ADDON[plugin.video.pley 30175]"),
        label(L.GUTTER, 828, 1100, 40, L.FONT_BODY, L.MUTED, "$ADDON[plugin.video.pley 30176]"),
        # Empty until the server says it requires two-step sign-in, which
        # it answers after the screen shows.
        label(L.GUTTER, 880, 1100, 40, L.FONT_BODY, L.MUTED, id_=L.SIGNIN_TWO_STEP),
        image(L.GUTTER, 964, 16, 16, "circle.png", color=L.ACCENT, animations=[anim("Conditional", "effect=fade start=100 end=40 time=1000 pulse=true", "true")]),
        label(L.GUTTER + 40, 944, 1100, 56, L.FONT_BODY_LARGE, L.MUTED, id_=L.SIGNIN_STATUS),
        group(
            L.SIGNIN_QR_CARD,
            1200,
            240,
            536,
            720,
            [
                image(0, 0, 536, 536, "round12.png", color="FFFFFFFF", border=14),
                image(32, 32, 472, 472, "", id_=L.SIGNIN_QR, aspect="keep"),
                label(0, 560, 536, 40, L.FONT_BODY, L.MUTED, "$ADDON[plugin.video.pley 30177]", id_=L.SIGNIN_SCAN, align="center"),
            ],
        ),
        grouplist(None, 1200, 868, 536, L.BUTTON, [pill(L.SIGNIN_CHANGE, None, "$ADDON[plugin.video.pley 30178]")], align="center"),
    ]:
        controls.append(c)
    w.append(controls)
    return w


def indent(e: ET.Element, level: int = 0) -> None:
    """ElementTree.indent, which Python 3.8 lacks."""
    pad = "\n" + "  " * level
    if len(e):
        if not e.text or not e.text.strip():
            e.text = pad + "  "
        for child in e:
            indent(child, level + 1)
        if not child.tail or not child.tail.strip():
            child.tail = pad
    if level and (not e.tail or not e.tail.strip()):
        e.tail = pad


def render(w: ET.Element) -> str:
    indent(w)
    head = '<?xml version="1.0" encoding="UTF-8"?>\n<!-- Written by apps/kodi/skin.py from resources/lib/pley/layout.py: edit those, not this. -->\n'
    return head + ET.tostring(w, encoding="unicode") + "\n"


def windows() -> dict:
    """The skin's files this writes, by path."""
    return {OUT: render(window()), os.path.join(SKIN, L.SIGNIN_WINDOW): render(signin())}


def main(argv) -> int:
    written = windows()
    if "--check" in argv:
        stale = []
        for path, text in written.items():
            with open(path, encoding="utf-8") as f:
                if f.read() != text:
                    stale.append(path)
        for path in stale:
            print("%s is out of date: python apps/kodi/skin.py" % path)
        return 1 if stale else 0
    for path, text in written.items():
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
        print(path)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
