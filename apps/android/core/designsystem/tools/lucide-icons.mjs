// Generates PleyIcons.kt from lucide-react's icon data: every icon as SVG path strings.
// In apps/android/core/designsystem, with the web app's packages installed:
//   node tools/lucide-icons.mjs ../../../../node_modules/lucide-react src/main/kotlin/app/pley/android/designsystem/icon/PleyIcons.kt
import { readFileSync, writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const [, , lucideDir, out] = process.argv;
const version = JSON.parse(readFileSync(`${lucideDir}/package.json`, "utf8")).version;

// [Kotlin name, lucide file, filled, why]
const icons = [
  // Navigation and the rail
  ["Search", "search"], ["House", "house"], ["Clapperboard", "clapperboard"], ["Tv", "tv"], ["Music", "music"],
  ["Image", "image"], ["Images", "images"], ["Film", "film"], ["Layers", "layers"], ["ListMusic", "list-music"],
  ["ListVideo", "list-video"], ["LibraryBig", "library-big"], ["Settings", "settings"], ["Settings2", "settings-2"],
  ["SlidersHorizontal", "sliders-horizontal"], ["UserRound", "user-round"], ["Users", "users"], ["UserPlus", "user-plus"],
  ["Compass", "compass"], ["Menu", "menu"],
  // Playback
  ["Play", "play", true], ["Pause", "pause", true], ["SkipBack", "skip-back", true], ["SkipForward", "skip-forward", true],
  ["Rewind", "rewind"], ["FastForward", "fast-forward"], ["RotateCcw", "rotate-ccw"], ["RotateCw", "rotate-cw"],
  ["Captions", "captions"], ["AudioLines", "audio-lines"], ["Gauge", "gauge"], ["PictureInPicture2", "picture-in-picture-2"],
  ["Volume2", "volume-2"], ["Sun", "sun"], ["Maximize", "maximize"], ["Minimize", "minimize"], ["Shuffle", "shuffle"],
  ["Repeat", "repeat"], ["Repeat1", "repeat-1"], ["MonitorPlay", "monitor-play"],
  // Actions and state
  ["Check", "check"], ["CircleCheck", "circle-check"], ["Star", "star", true], ["Eye", "eye"], ["EyeOff", "eye-off"],
  ["Plus", "plus"], ["ListPlus", "list-plus"], ["Ellipsis", "ellipsis"], ["EllipsisVertical", "ellipsis-vertical"],
  ["Info", "info"], ["X", "x"], ["Delete", "delete"], ["Trash2", "trash-2"], ["Pencil", "pencil"], ["Copy", "copy"],
  ["Save", "save"], ["Download", "download"], ["RefreshCw", "refresh-cw"], ["History", "history"], ["Lock", "lock"],
  ["LockOpen", "lock-open"], ["LogOut", "log-out"], ["Link2", "link-2"], ["Sparkles", "sparkles"],
  ["GripVertical", "grip-vertical"], ["List", "list"], ["ListOrdered", "list-ordered"], ["Square", "square"],
  ["LoaderCircle", "loader-circle"], ["CircleAlert", "circle-alert"], ["TriangleAlert", "triangle-alert"],
  ["ShieldCheck", "shield-check"], ["TicketX", "ticket-x"], ["MailPlus", "mail-plus"], ["Activity", "activity"],
  // My List
  ["BookmarkPlus", "bookmark-plus"], ["BookmarkCheck", "bookmark-check"], ["BookmarkMinus", "bookmark-minus"],
  // Arrows
  ["ArrowLeft", "arrow-left"], ["ArrowRight", "arrow-right"], ["ArrowUp", "arrow-up"], ["ArrowUpDown", "arrow-up-down"],
  ["ArrowDownWideNarrow", "arrow-down-wide-narrow"], ["ArrowUpNarrowWide", "arrow-up-narrow-wide"],
  ["ChevronDown", "chevron-down"], ["ChevronLeft", "chevron-left"], ["ChevronRight", "chevron-right"],
  ["ChevronUp", "chevron-up"], ["ChevronsRight", "chevrons-right"],
  // Devices, servers, files
  ["Server", "server"], ["ServerOff", "server-off"], ["Smartphone", "smartphone"], ["Laptop", "laptop"], ["Monitor", "monitor"], ["Wifi", "wifi"],
  ["HardDrive", "hard-drive"], ["Cpu", "cpu"], ["AppWindow", "app-window"], ["LayoutDashboard", "layout-dashboard"],
  ["Folder", "folder"], ["FolderOpen", "folder-open"], ["FolderPlus", "folder-plus"], ["FolderSearch", "folder-search"],
  ["ImageUp", "image-up"], ["ScanSearch", "scan-search"], ["DatabaseBackup", "database-backup"],
  // A USB drive's page (TVs): the rail's entry, a video's line, "Up a folder"
  ["Usb", "usb"], ["FilePlay", "file-play"], ["CornerLeftUp", "corner-left-up"],
  // Live TV
  ["TvMinimal", "tv-minimal"], ["CalendarDays", "calendar-days"], ["LayoutGrid", "layout-grid"], ["Radio", "radio"],
  ["Clock", "clock"], ["StarOutline", "star"],
];

const n = (v) => Number(v);
const fmt = (v) => String(+n(v).toFixed(4));

function toPath(tag, a) {
  switch (tag) {
    case "path":
      return a.d;
    case "circle": {
      const cx = n(a.cx), cy = n(a.cy), r = n(a.r);
      return `M${fmt(cx - r)} ${fmt(cy)}a${fmt(r)} ${fmt(r)} 0 1 0 ${fmt(2 * r)} 0a${fmt(r)} ${fmt(r)} 0 1 0 ${fmt(-2 * r)} 0z`;
    }
    case "ellipse": {
      const cx = n(a.cx), cy = n(a.cy), rx = n(a.rx), ry = n(a.ry);
      return `M${fmt(cx - rx)} ${fmt(cy)}a${fmt(rx)} ${fmt(ry)} 0 1 0 ${fmt(2 * rx)} 0a${fmt(rx)} ${fmt(ry)} 0 1 0 ${fmt(-2 * rx)} 0z`;
    }
    case "rect": {
      const x = n(a.x ?? 0), y = n(a.y ?? 0), w = n(a.width), h = n(a.height);
      const rx = Math.min(n(a.rx ?? a.ry ?? 0), w / 2), ry = Math.min(n(a.ry ?? a.rx ?? 0), h / 2);
      if (!rx) return `M${fmt(x)} ${fmt(y)}h${fmt(w)}v${fmt(h)}h${fmt(-w)}z`;
      return (
        `M${fmt(x + rx)} ${fmt(y)}h${fmt(w - 2 * rx)}a${fmt(rx)} ${fmt(ry)} 0 0 1 ${fmt(rx)} ${fmt(ry)}` +
        `v${fmt(h - 2 * ry)}a${fmt(rx)} ${fmt(ry)} 0 0 1 ${fmt(-rx)} ${fmt(ry)}h${fmt(-(w - 2 * rx))}` +
        `a${fmt(rx)} ${fmt(ry)} 0 0 1 ${fmt(-rx)} ${fmt(-ry)}v${fmt(-(h - 2 * ry))}a${fmt(rx)} ${fmt(ry)} 0 0 1 ${fmt(rx)} ${fmt(-ry)}z`
      );
    }
    case "line":
      return `M${fmt(a.x1)} ${fmt(a.y1)}L${fmt(a.x2)} ${fmt(a.y2)}`;
    case "polyline":
    case "polygon": {
      const pts = a.points.trim().split(/[\s,]+/).map(n);
      let d = "";
      for (let i = 0; i < pts.length; i += 2) d += `${i ? "L" : "M"}${fmt(pts[i])} ${fmt(pts[i + 1])}`;
      return tag === "polygon" ? d + "z" : d;
    }
    default:
      throw new Error(`unsupported ${tag}`);
  }
}

// An old name is a file that re-exports its new one ("music" is "eighth-notes").
function target(file) {
  const text = readFileSync(`${lucideDir}/dist/esm/icons/${file}.mjs`, "utf8");
  const alias = text.match(/export \{ default \} from '\.\/([a-z0-9-]+)\.mjs'/);
  return alias ? target(alias[1]) : file;
}

const lines = [];
for (const [name, file, filled] of icons) {
  const mod = await import(pathToFileURL(`${lucideDir}/dist/esm/icons/${target(file)}.mjs`).href);
  if (!mod.__iconData) throw new Error(`no icon data in ${file}`);
  const paths = mod.__iconData.node.map(([tag, attrs]) => toPath(tag, attrs));
  const args = paths.map((p) => `        "${p}",`).join("\n");
  lines.push(`    /** Lucide \`${file}\`${filled ? ", filled" : ""}. */\n    val ${name} = PleyIcon(\n        "${file}",\n        ${filled ? "true" : "false"},\n${args}\n    )\n`);
}

const kt = `// Generated from lucide-react ${version} (ISC; see assets/licenses/lucide.txt) by
// tools/lucide-icons.mjs. Edit the list there and regenerate, never by hand.
package app.pley.android.designsystem.icon

/**
 * The Lucide icons pley draws, as on the web: 24 by 24, stroked with round
 * ends, a few filled (Play, Pause, Previous, Next, the star). Built into
 * vectors on first use, so unused ones cost a string each.
 */
@Suppress("LargeClass")
object PleyIcons {
${lines.join("\n")}}
`;
writeFileSync(out, kt);
console.log(`${icons.length} icons from lucide-react ${version}`);
