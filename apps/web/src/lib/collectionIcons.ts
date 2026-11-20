// The icons and colors a collection's poster can carry, as the server has
// them (apps/server/src/collection_art.rs): the server draws the poster, the
// web app draws its picker.

import {
  Award,
  Baby,
  BookHeadphones,
  BookOpen,
  Calendar,
  Clapperboard,
  Clock,
  Dog,
  Drama,
  EyeOff,
  FaceGrinning,
  Fingerprint,
  Flame,
  Ghost,
  Globe,
  Heart,
  Landmark,
  Layers,
  Leaf,
  MountainSnow,
  Music,
  Popcorn,
  Rocket,
  Snowflake,
  Sparkles,
  Star,
  Sun,
  Swords,
  TreePine,
  Trophy,
  Tv,
  Users,
  Video,
  WandSparkles,
  Zap,
  type LucideIcon,
} from "lucide-react";

export type CollectionIconName =
  "star"
  | "clock"
  | "calendar"
  | "eye-off"
  | "heart"
  | "flame"
  | "trophy"
  | "award"
  | "sparkles"
  | "ghost"
  | "rocket"
  | "wand-sparkles"
  | "drama"
  | "face-grinning"
  | "swords"
  | "zap"
  | "fingerprint"
  | "mountain-snow"
  | "globe"
  | "landmark"
  | "video"
  | "dog"
  | "baby"
  | "users"
  | "popcorn"
  | "book-open"
  | "snowflake"
  | "sun"
  | "leaf"
  | "tree-pine"
  | "clapperboard"
  | "tv"
  | "music"
  | "book-headphones"
  | "layers";

export type CollectionColorName = "red" | "orange" | "amber" | "green" | "teal" | "blue" | "violet" | "pink" | "slate";

/** Each icon by its name (Lucide's), in the server's order. */
export const COLLECTION_ICONS: [CollectionIconName, LucideIcon][] = [
  ["star", Star],
  ["clock", Clock],
  ["calendar", Calendar],
  ["eye-off", EyeOff],
  ["heart", Heart],
  ["flame", Flame],
  ["trophy", Trophy],
  ["award", Award],
  ["sparkles", Sparkles],
  ["ghost", Ghost],
  ["rocket", Rocket],
  ["wand-sparkles", WandSparkles],
  ["drama", Drama],
  ["face-grinning", FaceGrinning],
  ["swords", Swords],
  ["zap", Zap],
  ["fingerprint", Fingerprint],
  ["mountain-snow", MountainSnow],
  ["globe", Globe],
  ["landmark", Landmark],
  ["video", Video],
  ["dog", Dog],
  ["baby", Baby],
  ["users", Users],
  ["popcorn", Popcorn],
  ["book-open", BookOpen],
  ["snowflake", Snowflake],
  ["sun", Sun],
  ["leaf", Leaf],
  ["tree-pine", TreePine],
  ["clapperboard", Clapperboard],
  ["tv", Tv],
  ["music", Music],
  ["book-headphones", BookHeadphones],
  ["layers", Layers],
];

/** Each color by its name, with the server's RGB. */
export const COLLECTION_COLORS: [CollectionColorName, string][] = [
  ["red", "rgb(214 69 69)"],
  ["orange", "rgb(222 110 48)"],
  ["amber", "rgb(196 136 26)"],
  ["green", "rgb(76 150 58)"],
  ["teal", "rgb(29 158 117)"],
  ["blue", "rgb(55 138 221)"],
  ["violet", "rgb(118 104 221)"],
  ["pink", "rgb(212 83 126)"],
  ["slate", "rgb(125 138 158)"],
];

export const DEFAULT_COLLECTION_COLOR: CollectionColorName = "blue";

/** The color the server draws a collection with no posters and no icon on. */
export const EMPTY_COLLECTION_COLOR: CollectionColorName = "slate";

/** The icon a poster on its color alone starts with: its library's, as
 * the server picks it (layers for a kind it adds later). */
export function defaultCollectionIcon(kind: string | undefined): CollectionIconName {
  switch (kind) {
    case "movies":
      return "clapperboard";
    case "shows":
      return "tv";
    case "music":
      return "music";
    case "audiobooks":
      return "book-headphones";
    default:
      return "layers";
  }
}

export function collectionIcon(name: string | null): LucideIcon | undefined {
  return COLLECTION_ICONS.find(([n]) => n === name)?.[1];
}

export function collectionColor(name: string | null): string {
  return COLLECTION_COLORS.find(([n]) => n === name)?.[1] ?? "rgb(55 138 221)";
}

export function isCollectionIcon(name: string | null): name is CollectionIconName {
  return COLLECTION_ICONS.some(([n]) => n === name);
}

export function isCollectionColor(name: string | null): name is CollectionColorName {
  return COLLECTION_COLORS.some(([n]) => n === name);
}
