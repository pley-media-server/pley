import { isApiError } from "../../api/client";
import type { CreditRole, ItemSummary, PersonDetail } from "../../api/types";
import { pluralize } from "../../format/format";
import { t } from "../../i18n";
import { useUi } from "../context";
import { Row, rowHeight, SectionPage, type Section } from "../kit/row";
import { Tile } from "../kit/tile";
import { Art } from "../kit/art";
import type { Entry } from "../shell/navigator";
import { failedTitle, Message, problem, useLoad } from "./common";
import { pageOf } from "./item-actions";

/** The rows, in order, each titled with what the person did ("Acting"). */
const ROLES: CreditRole[] = ["actor", "director", "creator", "writer", "producer"];

const ROWS_TOP = 240;

/** "2019 · as Neo", "Guest star (3 episodes)", "Director". */
export function creditCaption(item: ItemSummary, roles: { role: CreditRole; character?: string; episodes?: number }[]): string {
  const words = roles.map((r) => {
    let w: string;
    if (r.role === "actor") w = r.character ? t("browse.person.as", { character: r.character }) : r.episodes ? t("browse.person.guestStar") : t("browse.person.roles.actor");
    else w = t(`browse.person.roles.${r.role}`);
    return r.episodes ? t("browse.person.withEpisodes", { role: w, count: r.episodes }) : w;
  });
  return [item.year ? String(item.year) : "", words.join(", ")].filter(Boolean).join(" · ");
}

/** "3 movies · 2 shows in your libraries". */
function countsLine(p: PersonDetail): string {
  const movies = p.credits.filter((c) => c.item.kind === "movie").length;
  const shows = p.credits.filter((c) => c.item.kind === "show").length;
  const parts = [movies ? pluralize(movies, "movie") : "", shows ? pluralize(shows, "show") : ""].filter(Boolean);
  return parts.length ? t("browse.person.inLibraries", { counts: parts.join(" · ") }) : "";
}

function initials(name: string): string {
  return name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w.charAt(0).toUpperCase())
    .join("");
}

/** A person (TvPersonScreen): their photo and name, then a row per role. */
export function PersonScreen(p: { entry: Entry; id: number }) {
  const ui = useUi();
  const loaded = useLoad((c) => ui.api.person(p.id, c), [p.id]);
  const person = loaded.data;
  if (!person) {
    if (loaded.error && isApiError(loaded.error, 404)) {
      return <Message icon="user-round" title={t("browse.person.notFound")} body={t("browse.person.notFoundHint")} action={{ label: t("common.actions.back"), onSelect: () => ui.nav.pop() }} />;
    }
    if (loaded.error) return <Message title={failedTitle(loaded.error, t("browse.person.cantLoad"))} body={problem(loaded.error)} action={{ label: t("common.actions.retry"), icon: "refresh-cw", onSelect: loaded.reload }} />;
    return (
      <div class="person-page" role="status" aria-label={t("common.states.loading")}>
        <div class="person-photo" />
        <div class="skeleton" style="position:absolute;left:288px;top:110px;width:280px;height:38px" />
        <div class="skeleton" style="position:absolute;left:288px;top:156px;width:200px;height:20px" />
      </div>
    );
  }
  // Each row's tile focused last, so Back from a title comes to it, the rows drawn again.
  const rowIndex = (p.entry.state.rowIndex as Record<string, number> | undefined) ?? {};
  p.entry.state.rowIndex = rowIndex;
  const sections: Section[] = [];
  let first = true;
  for (const role of ROLES) {
    const credits = person.credits.filter((c) => c.roles.some((x) => x.role === role));
    if (!credits.length) continue;
    const initialRow = first;
    first = false;
    sections.push({
      key: role,
      height: rowHeight("poster"),
      anchor: ROWS_TOP,
      render: (onFocus) => (
        <Row
          title={t(`browse.person.rows.${role}`)}
          kind="poster"
          items={credits}
          keyOf={(c) => c.item.id}
          enterAt={rowIndex[role]}
          onLeftEdge={ui.toRail}
          onFocusIndex={(i) => {
            rowIndex[role] = i;
            onFocus();
          }}
          tile={(c, i, place) => (
            <Tile
              item={c.item}
              kind="poster"
              subtitle={creditCaption(c.item, c.roles.filter((x) => x.role === role))}
              fid={`${role}/${c.item.id}`}
              initial={initialRow && i === 0}
              priority={place.priority}
              onFocus={place.onFocus}
              onSelect={() => ui.nav.open(pageOf(c.item))}
              onActions={() => ui.nav.open({ name: "actions", id: c.item.id })}
            />
          )}
        />
      ),
    });
  }
  const counts = countsLine(person);
  return (
    <div class="person-page">
      <div class="person-header">
        <div class="person-photo">
          {person.photo_url ? (
            <Art candidates={[person.photo_url]} width={160} title={person.name} class="tile-clip" />
          ) : (
            <span class="t-page" style="color:rgba(233,235,239,0.8)">
              {initials(person.name)}
            </span>
          )}
        </div>
        <div style="margin-left:32px;min-width:0">
          <div class="t-page c-fg lines-2">{person.name}</div>
          {counts && (
            <div class="t-body c-muted" style="margin-top:8px">
              {counts}
            </div>
          )}
        </div>
      </div>
      <SectionPage
        sections={sections}
        top={ROWS_TOP}
        clipAbove={ROWS_TOP - 12}
        focusedKey={p.entry.state.section as string | undefined}
        onFocusSection={(k) => (p.entry.state.section = k)}
      />
      <div class="rail-fade" />
    </div>
  );
}
