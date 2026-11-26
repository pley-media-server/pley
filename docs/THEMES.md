# Themes

Pley is drawn in one of nine themes, picked on Account → Appearance, in the
theme's own accent or one of six others. The pick belongs to the device it
was made on: each browser, and the desktop app, keeps its own, and starts in
Midnight until one is picked there. The web app
and the desktop app draw all nine. The other apps draw Midnight for now, and
TV apps will draw only the dark ones (see [On a TV](#on-a-tv)). This document is for anyone changing how Pley looks:
what a theme is, the rules every theme keeps, and how to add one.

## What every theme owes people

- **Everyone can read it.** Words meet WCAG's contrast in every theme, on
  every surface they sit on, for people with low vision, on a dim laptop and
  in a sunny room. No theme is allowed to fall short, however good it looks.
- **Contrast goes where it counts.** Main text stands out most, secondary
  text and hints a step less, and state colors (success, danger, warning)
  stay distinct from the accent. A picture never competes with the words in
  front of it.
- **It feels balanced.** Each theme has one accent and a few close surface
  tones. Light from a backdrop (Twilight's sunset, Sunshine's bubbles) stays
  in the background. Nothing flashes, and switching changes every color in
  the same frame.
- **It looks considered.** Every detail belongs to the theme: the logo, the
  tab's icon, shadows, the dim behind a dialog, the picture behind a title.
  A theme that looks right on Home and wrong in Settings isn't finished.

## The themes

| Id | Name | Kind | What it is | On a TV |
|---|---|---|---|---|
| `midnight` | Midnight | Dark | Pley's own: deep night and sky blue. The default, and the one every other theme is measured against. | Midnight |
| `midday` | Midday | Light | Midnight in daylight: Midnight's cool grays turned light, its sky blue deepened until it reads as text. | Midnight |
| `twilight` | Twilight | Dark | A sunset: deep purple surfaces, a pink accent, the sun's last orange light along the foot of the window. | Twilight |
| `starlight` | Starlight | Light | Soft champagne and cream with a bronze-gold accent. | Moonlight |
| `moonlight` | Moonlight | Dark | Carbon blacks and graphite, champagne words, a gold accent and a faint moon at the top right. | Moonlight |
| `nightshade` | Nightshade | Dark | Green-black with a soft mint accent and a faint green light. Success leans teal, so it never reads as the accent. | Nightshade |
| `greenlight` | Greenlight | Light | Nightshade in daylight: soft sage surfaces and a deep green accent. | Nightshade |
| `sunshine` | Sunshine | Light | For children: sunny cream with bubbles of colored light, a sidebar in a soft rainbow, sky and lilac controls, a bright violet, rounder shapes and a toy-box logo. | Midnight |
| `blacklight` | Blacklight | Dark | Black and white: white words on black, and a white accent unless another is picked. States differ by their gray, and by their words and icons as everywhere. | Blacklight |

A device keeps its pick in its own storage (the web app: `pley.theme` in
`localStorage`); the server keeps none. A device that picked none, or whose
stored id it doesn't know, shows Midnight.

Each light theme has a **dark twin**: Midday's is Midnight, Starlight's is
Moonlight, Greenlight's is Nightshade and Sunshine's is Midnight. In a light
theme, the players use their twin's colors, because a light frame would glare
around the picture. On a TV, the twin replaces the light theme everywhere.

## On a TV

TV apps (Android TV, Samsung, LG, Roku, Kodi, Apple TV) draw only dark
themes. For now they draw Midnight; when one offers themes, it follows this.
A TV is watched in a dim room from across it, and a light page would glare.
A TV app offers only the dark themes, keeps its pick on the TV, and shows any
light theme it is handed as its dark twin (the table's last column).

## Accents

An accent can replace a theme's own: Sky, Violet, Miami, Gold, Mint or
Amber, picked under the themes on Account → Appearance. It colors what the
theme's accent colors (buttons, links, focus, what is picked) and the logo's
posters, and nothing else: the surfaces, the words and the logo's tile stay
the theme's. The logo takes the colors of the theme its accent comes from:
Sky's are Midnight's and Midday's, Miami's Twilight's, Gold's Moonlight's
and Starlight's, Mint's Nightshade's and Greenlight's, so an accent that
matches a theme's own leaves that theme's logo as it was. Violet's and
Amber's, and Miami's in the light themes, follow the same steps from light
to deep. Sunshine keeps its toy-box logo under any accent. Like the
theme, it belongs to the device (the web app: `pley.accent` in
`localStorage`), and a device that picked none, or whose stored id it
doesn't know, shows the theme's own. It stays when the theme changes. The
other apps draw the theme's own for now.

Each accent has a color for the dark themes and a deeper one for the light
themes, each with its hover and the words on it. In a light theme the players
take the dark one, as they take the dark twin's colors.

Over Blacklight, an accent takes the white's place on buttons, links, focus
and the logo, and is the only color Blacklight draws: its states stay gray
under any accent.

An accent never reads as a state. Where one comes near a state color, that
state moves aside while it is picked, as Nightshade's success leans teal
beside its mint: with Mint, success turns teal; with Gold, warning turns
orange; with Amber, warning turns yellow.

## How the web app draws a theme

- `apps/web/src/index.css` holds each theme's colors in a
  `[data-theme="<id>"]` block. Midnight's are also `@theme`'s own. The page's
  theme is the `data-theme` attribute on `<html>`, and any element can carry
  its own (the picker's previews do).
- The page's accent is the `data-accent` attribute on `<html>`, none for the
  theme's own. Each `[data-accent="<id>"]` block sets `--accent-dark`,
  `--accent-dark-hover` and `--accent-dark-fg`, the same three for `light`,
  the logo's colors (`--accent-dark-logo…`, `--accent-light-logo…`), and
  `--success-dark`/`--success-light` or `--warning-…` where it moves a
  state. Every theme reads them, its own color the fallback:
  `--color-accent: var(--accent-dark, #38bdf8)`. `[data-accent="own"]` clears
  them, so the picker's swatch of the theme's own shows inside a page with
  another.
- `apps/web/src/theme` holds the ids, shows a theme and an accent, and keeps
  the device's picks in storage. `public/theme.js` puts them on the page
  before it first draws (the CSP runs no script written into the page).
- `apps/web/src/pages/ThemeCard.tsx` is the picker on Account → Appearance.

### Tokens

Every color the app draws comes from these, as Tailwind classes (`bg-surface`,
`text-muted`, `border-border`):

| Token | For |
|---|---|
| `bg` | The page |
| `surface`, `surface-2`, `surface-3` | Cards and the sidebar; inputs and hovered rows; pressed things and placeholders |
| `border`, `border-strong` | Lines between things; outlined buttons |
| `fg`, `muted`, `subtle` | Main text; secondary text; hints |
| `accent`, `accent-hover`, `accent-fg` | Buttons, links, focus; hovered; words on the accent |
| `success`, `danger`, `warning`, `info` | States, as text and as fills |
| `danger-fg` | Words on a danger fill (dark on the dark themes' bright red, white on the light themes' deep red) |
| `brand-light`, `brand`, `brand-deep`, `brand-far`, `brand-tile` | The logo: the front poster's gradient, the middle and back posters, the tile |
| `shade` | Shadows and the dim behind a dialog |

Besides the colors, each theme sets the following:
- `--glow`: a backdrop behind the whole window, or `none`.
- `--sidebar`: a picture for the sidebar, or `none`.
- `--backdrop-opacity`: how strongly a title's picture shows behind its words.
- `--caret`: a select's arrow, in its `muted` color.

### Rules

- **Only the theme's colors.** A wash over a surface is the text color at a
  few percent (`bg-fg/5`, `border-fg/10`), so it works dark or light. White
  and black are only for what sits on a picture or a video. Shadows use
  `shadow-shade/…`, never `shadow-black`.
- **Words on a fill take that fill's words color:** `text-accent-fg` on
  `bg-accent`, `text-danger-fg` on `bg-danger`, `text-bg` on `bg-warning`.
  Never `text-white` on a state color.
- **Pictures fade themselves.** A picture behind words (the home banner, a
  title's backdrop) uses `banner-fade`, `banner-fade-wide` or
  `backdrop-fade`. They mask the picture so the page and its backdrop show
  through with no edge. Never lay `from-bg` gradients over a picture.
- **Players stay dark.** A player's outermost element carries `data-dark`.
  A light theme's selector `[data-theme="<id>"] [data-dark]` belongs in its
  dark twin's block.
- **Every theme sets every variable,** `--glow` and `--sidebar` included
  (`none` when it has none). Otherwise a preview of one theme inside a page in
  another would inherit the page's backdrop.

### Contrast

`apps/web/src/theme/palettes.test.ts` reads `index.css` and holds every
theme to these levels:

| Pair | At least |
|---|---|
| `fg` on `bg`, `surface`, `surface-2` | 7:1 |
| `fg` and `muted` on `surface-3`; `muted` and `subtle` on `bg`, `surface`, `surface-2` | 4.5:1 |
| `accent` on `bg` and every surface; `accent-fg` on `accent` and `accent-hover` | 4.5:1 |
| Each state color on `surface`, and on its own tint up to 20% | 4.5:1 |
| `danger-fg` on `danger`, and on the danger button's 90% fill; `bg` on `warning` | 4.5:1 |
| The logo's front poster on its tile | 4.5:1 |
| The logo's middle poster on its tile | 2:1 |
| The logo's back poster on its tile | 1.3:1 |

It holds every theme to them again under each accent. It also
keeps each accent, and each state an accent moves, apart in hue from the
state colors: at least 0.035 apart in OKLab's a and b, the distance between
Nightshade's mint and its teal success.

The test can't see the pictures. After a change to a backdrop, a fade or a
theme, look at Home's banner and a title's page with a dark picture and a
light one, in a dark theme and a light theme. Every line of text has to stay
readable where the picture is brightest.

## Adding a theme

1. Pick an id: one lowercase word. Add it to `THEMES` in
   `apps/web/src/theme/index.ts`, in the order the picker shows them, and to
   its tests (`apps/web/src/theme/index.test.ts`, the picker's count in
   `ThemeCard.test.tsx`).
2. Add its block to `index.css`, with every token and variable, starting from
   the closest theme. A light theme also goes in its dark twin's selector list
   (`[data-theme="<id>"] [data-dark]`). Its accent, success and warning read
   the accent's variables, as the others' do (`--accent-light…` in a light
   theme), and so do its logo's posters. Every accent's logo has
   to stay readable on its tile.
3. Give it a name and a caption of a few words in every language: under
   `account.theme` in `apps/web/src/i18n/locales/<language>/pages.json`.
4. Describe it in this document's table, with what a TV shows, and add it to
   the server's changelog line.
5. Run `bun run check` in `apps/web`, then look at Home, a title's page, a
   library, Account and Settings in it, and at the picker in every theme.

## Adding an accent

1. Pick an id: one lowercase word. Add it to `ACCENTS` in
   `apps/web/src/theme/index.ts`, in the order the picker shows them, and to
   its test.
2. Add its block to `index.css`, with its colors and its logo's, after the
   others. The dark one has to read on every dark theme's surfaces and the
   light one on every light theme's; `palettes.test.ts` names the theme and
   the surface that falls short. If it comes near a state color, move that
   state in the block, for the dark themes and the light ones.
3. Give it a name in every language, under `account.theme.accent.names` in
   `apps/web/src/i18n/locales/<language>/pages.json`.
4. Name it in this document and in the server's changelog line.
5. Run `bun run check` in `apps/web`, then look at Home, a title's page and
   Settings in it, over a dark theme and a light one, and at the picker.