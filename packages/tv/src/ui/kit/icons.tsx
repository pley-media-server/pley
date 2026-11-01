import {
  ArrowDownWideNarrow,
  ArrowLeft,
  ArrowRight,
  ArrowUpDown,
  ArrowUpNarrowWide,
  AudioLines,
  Ban,
  Bell,
  BellOff,
  BookmarkCheck,
  BookmarkPlus,
  Calendar,
  Captions,
  Check,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ChevronsRight,
  ChevronUp,
  CircleAlert,
  CircleCheck,
  CircleX,
  Clapperboard,
  Clock,
  Compass,
  Delete,
  Disc3,
  Download,
  Ellipsis,
  EyeOff,
  FastForward,
  Film,
  Funnel,
  Gauge,
  GripVertical,
  History,
  House,
  Inbox,
  Info,
  Languages,
  Layers,
  List,
  ListMusic,
  ListOrdered,
  ListPlus,
  ListVideo,
  Lock,
  LogOut,
  Monitor,
  Moon,
  Music,
  Pause,
  Pencil,
  Play,
  Plus,
  Radio,
  RefreshCw,
  Repeat,
  Repeat1,
  RotateCcw,
  RotateCw,
  Save,
  Search,
  Server,
  ServerOff,
  Settings,
  Shuffle,
  SkipBack,
  SkipForward,
  SlidersHorizontal,
  Star,
  Tag,
  Timer,
  Trash2,
  Tv,
  TvMinimal,
  UserRound,
  Users,
  WifiOff,
  X,
  type IconNode,
} from "lucide";

const ICONS = {
  "arrow-down-wide-narrow": ArrowDownWideNarrow,
  "arrow-left": ArrowLeft,
  "arrow-right": ArrowRight,
  "arrow-up-down": ArrowUpDown,
  "arrow-up-narrow-wide": ArrowUpNarrowWide,
  "audio-lines": AudioLines,
  ban: Ban,
  bell: Bell,
  "bell-off": BellOff,
  "bookmark-check": BookmarkCheck,
  "bookmark-plus": BookmarkPlus,
  calendar: Calendar,
  captions: Captions,
  check: Check,
  "chevron-down": ChevronDown,
  "chevron-left": ChevronLeft,
  "chevron-right": ChevronRight,
  "chevron-up": ChevronUp,
  "chevrons-right": ChevronsRight,
  "circle-alert": CircleAlert,
  "circle-check": CircleCheck,
  "circle-x": CircleX,
  clapperboard: Clapperboard,
  clock: Clock,
  compass: Compass,
  delete: Delete,
  "disc-3": Disc3,
  download: Download,
  ellipsis: Ellipsis,
  "eye-off": EyeOff,
  "fast-forward": FastForward,
  film: Film,
  funnel: Funnel,
  gauge: Gauge,
  "grip-vertical": GripVertical,
  history: History,
  house: House,
  inbox: Inbox,
  info: Info,
  languages: Languages,
  layers: Layers,
  list: List,
  "list-music": ListMusic,
  "list-ordered": ListOrdered,
  "list-plus": ListPlus,
  "list-video": ListVideo,
  lock: Lock,
  "log-out": LogOut,
  monitor: Monitor,
  moon: Moon,
  music: Music,
  pause: Pause,
  pencil: Pencil,
  play: Play,
  plus: Plus,
  radio: Radio,
  "refresh-cw": RefreshCw,
  repeat: Repeat,
  "repeat-1": Repeat1,
  "rotate-ccw": RotateCcw,
  "rotate-cw": RotateCw,
  save: Save,
  search: Search,
  server: Server,
  "server-off": ServerOff,
  settings: Settings,
  shuffle: Shuffle,
  "skip-back": SkipBack,
  "skip-forward": SkipForward,
  "sliders-horizontal": SlidersHorizontal,
  star: Star,
  /** The star unfilled: not a favourite yet. */
  "star-outline": Star,
  tag: Tag,
  timer: Timer,
  "trash-2": Trash2,
  tv: Tv,
  "tv-minimal": TvMinimal,
  "user-round": UserRound,
  users: Users,
  "wifi-off": WifiOff,
  x: X,
} satisfies Record<string, IconNode>;

export type IconName = keyof typeof ICONS;

/** Drawn filled as well as stroked (the Android app's PleyIcons). */
const FILLED: IconName[] = ["play", "pause", "skip-back", "skip-forward", "star"];

/**
 * A Lucide icon on its 24 grid, stroked with round caps and joins, the
 * stroke thinner as the icon grows: 2 + (24 − size)/16 units, within 1.5–2.5.
 */
export function Icon(props: { name: IconName; size?: number; class?: string; color?: string }) {
  const size = props.size ?? 20;
  const stroke = Math.max(1.5, Math.min(2.5, 2 + (24 - size) / 16));
  const node = ICONS[props.name];
  const filled = FILLED.indexOf(props.name) >= 0;
  return (
    <svg
      class={`icon ${props.class ?? ""}`}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill={filled ? "currentColor" : "none"}
      stroke="currentColor"
      stroke-width={stroke}
      stroke-linecap="round"
      stroke-linejoin="round"
      style={props.color ? { color: props.color } : undefined}
      aria-hidden="true"
    >
      {node.map(([tag, attrs], i) => {
        const Tag = tag as "path";
        return <Tag key={i} {...(attrs as Record<string, string>)} />;
      })}
    </svg>
  );
}
