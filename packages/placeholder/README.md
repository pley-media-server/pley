# Pley's placeholder

A title whose picture hasn't come shows a gradient of its own colors.
Once no picture will come, Pley's mark is on it: the logo's three posters
in white, each lighter than the one behind, with a glyph cut out of the
front one for the title's kind of library: a clapperboard for movies, a
TV for shows, notes for music, an open book for audiobooks, and the logo's
play for anything else (extras, collections).

[placeholder.json](placeholder.json) is what every app draws it from.

## The gradient

- The hue comes from the title: `h = h * 31 + unit` over its UTF-16 code
  units (a character beyond the first plane counts as two), kept to an
  unsigned 32-bit number, then `hue = h % 360`. The same title gets the
  same colors on every app.
- `gradient`: from `hsl(hue, from.saturation, from.lightness)` to
  `hsl(hue + to.hueShift, to.saturation, to.lightness)`, along `angle`
  degrees as CSS draws it (135: top left to bottom right).
- `examples`: titles with their hue and their two colors as CSS turns
  hsl() into sRGB. An app's test checks its own gradient against them.

## The mark

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
front poster.

An app draws the mark only where the title's name is beside or under the
picture (a caption, a row's line): elsewhere a mark would stand for a title
no one can read. Backdrops, people, Live TV channels and a mosaic's
quarters get the gradient alone; a picture nothing names keeps its title.

## Who reads it

The web app and the Samsung web app import this package
(`placeholderGradient`, `markGlyph`, `markLayers`). Android, LG, Apple and
the native Samsung app keep their own copy of the gradient and the mark
in code; Kodi and Roku ship pictures of them. `bun
scripts/render-placeholder-marks.mjs` draws the mark's pictures (SVGs for
the native Samsung app, PNGs for Kodi and Roku), and
`bun scripts/render-kodi-skin.mjs` Kodi's gradients. A test in each app
holds its copy to this file, so a change here fails each app until it
follows; `test/placeholder.test.ts` holds the file to itself (the examples
to the rule, the glyphs inside the poster).
