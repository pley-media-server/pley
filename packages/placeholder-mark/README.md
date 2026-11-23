# Pley's placeholder mark

A title without a picture shows its gradient with Pley's mark on it, the
logo's three posters in white, each lighter than the one behind. The front
poster has a glyph cut out of it for the title's kind of library: a
clapperboard for movies, a TV for shows, notes for music, an open book for
audiobooks, and the logo's play for anything else (extras, collections).

[marks.json](marks.json) is what every app draws it from:

- `grid`: the side of the square the paths are on (64, the logo's).
- `share`, `max`: the mark is `share` of its box's shorter side, at most
  `max` dp or px. Over a title (a playlist's cover), it is `shareOverTitle`
  of the side, `gap` above the title, and left out when the title leaves
  it no room.
- `posters`: back to front, each filled white at its `alpha`.
- `glyphs`: cut out of the front poster. Draw the front poster's path and
  the glyph's as one path filled even-odd.
- `kinds`: an item's `kind` (the API's) to its glyph; any other kind takes
  `default`.

Paths use only absolute `M`, `L`, `C` and `Z`, so that an app without an
SVG path parser reads them in a few lines. A glyph stays 3 units inside the
front poster; `test/marks.test.ts` holds both.

The web app and the Samsung web app import this package. Android, LG and
Apple copy the paths, the numbers and `kinds` into their code; the native
Samsung app, Kodi and Roku copy `kinds` and ship pictures that
`bun scripts/render-placeholder-marks.mjs` draws from this file (SVGs for
the native Samsung app, PNGs for Kodi and Roku). A test in each app holds
its copy to this file, so a change here fails them until they follow.

An app draws the mark only where the title's name is beside or under the
picture (a caption, a row's line): elsewhere a mark would stand for a title
no one can read. Backdrops, people, Live TV channels and a mosaic's
quarters get the gradient alone; a picture nothing names keeps its title.
