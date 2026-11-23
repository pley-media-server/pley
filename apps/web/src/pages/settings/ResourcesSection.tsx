import { useState, type KeyboardEvent, type PointerEvent, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Gauge, Monitor } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useResources } from "@/api/hooks";
import type { ResourceClient, ResourceSample, Resources } from "@/api/types";
import { SectionTitle, Skeleton } from "@/components/ui";
import { currentLocale, t } from "@/i18n";
import { cn } from "@/lib/cn";
import { formatBitrate, formatBytes } from "@/lib/format";

/** What the charts span, as the server keeps it. */
const WINDOW_MS = 5 * 60_000;
/** The drawing's own units; it stretches to the card's width. */
const W = 600;
const H = 120;

/** pley in the brand's light blue; the whole machine, behind it, in gray. */
const PLEY = "var(--color-accent)";
const SYSTEM = "#7a8292";
/** What the machine receives, apart from what it sends. */
const RECEIVED = "#c98a3d";

interface Series {
  label: string;
  color: string;
  /** 0 to 1 of the chart's height. */
  value: (s: ResourceSample) => number;
  /** As the legend and the readout say it. */
  text: (s: ResourceSample) => string;
  title?: string;
}

/** A share in the language shown: one decimal below 10%, "12%" above. */
const percent = (n: number) => {
  const digits = n === 0 || n >= 10 ? 0 : 1;
  return new Intl.NumberFormat(currentLocale(), { style: "percent", minimumFractionDigits: digits, maximumFractionDigits: digits }).format(n / 100);
};

/** Bytes a second, in bits as network speeds are given: "8.2 Mbps", "0 kbps". */
const speed = (bytes: number) => formatBitrate(bytes * 8) || t("format.kbps", { n: 0 });

/** The network chart's top, in bytes a second: the five minutes' fastest, rounded up to 1, 2 or 5 times a power of ten in bits, 1 Mbps at least. */
export function networkScale(samples: ResourceSample[]): number {
  const peak = Math.max(1_000_000, ...samples.map((s) => Math.max(s.net_in, s.net_out, s.pley_out) * 8));
  const power = 10 ** Math.floor(Math.log10(peak));
  const step = [1, 2, 5, 10].find((m) => m * power >= peak) ?? 10;
  return (step * power) / 8;
}

/** The path through `points`, broken where samples are missing (the server was busy or restarted). */
function linePath(points: [number, number, number][], gapMs: number): string {
  let d = "";
  points.forEach(([x, y, at], i) => {
    const before = points[i - 1];
    const joined = before != null && at - before[2] <= gapMs;
    d += `${joined ? "L" : "M"}${x.toFixed(1)},${y.toFixed(1)}`;
  });
  return d;
}

function areaPath(points: [number, number, number][], gapMs: number): string {
  const runs: [number, number, number][][] = [];
  points.forEach((p, i) => {
    const before = points[i - 1];
    if (!before || p[2] - before[2] > gapMs) runs.push([]);
    runs[runs.length - 1]?.push(p);
  });
  return runs
    .filter((run) => run.length > 1)
    .map((run) => `${linePath(run, Infinity)}L${run.at(-1)?.[0].toFixed(1)},${H}L${run[0]?.[0].toFixed(1)},${H}Z`)
    .join("");
}

function LineKey({ color }: { color: string }) {
  return <span aria-hidden className="inline-block h-0.5 w-3 shrink-0 rounded-full" style={{ background: color }} />;
}

/** One measure over the last five minutes: pley's line in front of the machine's. */
function Chart({
  title,
  detail,
  data,
  series,
  marks = [percent(100), percent(50)],
}: {
  title: string;
  detail: ReactNode;
  data: Resources;
  series: Series[];
  /** What the top and middle lines stand for. */
  marks?: [string, string];
}) {
  const { t } = useTranslation("settings");
  // The sample pointed at, by its time: new samples move the others along.
  const [hoverAt, setHoverAt] = useState<number | null>(null);
  const samples = data.samples;
  const found = hoverAt == null ? -1 : samples.findIndex((s) => s.at === hoverAt);
  const hover = found < 0 ? null : found;
  const setHover = (i: number | null) => setHoverAt(i == null ? null : (samples[i]?.at ?? null));
  const latest = samples.at(-1);
  const end = latest?.at ?? 0;
  const start = end - WINDOW_MS;
  const gapMs = Math.max(data.interval_ms, 1_000) * 3;
  const x = (at: number) => ((at - start) / WINDOW_MS) * W;
  const y = (v: number) => H - Math.min(Math.max(v, 0), 1) * H;
  const shown = hover != null ? samples[hover] : latest;

  const pick = (e: PointerEvent<HTMLDivElement>) => {
    if (!samples.length) return;
    const box = e.currentTarget.getBoundingClientRect();
    const at = start + ((e.clientX - box.left) / box.width) * WINDOW_MS;
    let best = 0;
    let gap = Infinity;
    samples.forEach((s, i) => {
      if (Math.abs(s.at - at) < gap) [best, gap] = [i, Math.abs(s.at - at)];
    });
    setHover(best);
  };
  const step = (e: KeyboardEvent<HTMLDivElement>) => {
    if (!samples.length) return;
    const last = samples.length - 1;
    const now = hover ?? last;
    const next =
      e.key === "ArrowLeft" ? Math.max(0, now - 1) : e.key === "ArrowRight" ? Math.min(last, now + 1) : e.key === "Home" ? 0 : e.key === "End" ? last : null;
    if (e.key === "Escape") setHover(null);
    if (next == null) return;
    e.preventDefault();
    setHover(next);
  };

  const summary = latest
    ? t("resources.summary", { title, values: series.map((s) => `${s.label} ${s.text(latest)}`).join(", ") })
    : t("resources.noReadings", { title });

  return (
    <div className="rounded-lg border border-border bg-surface p-4">
      <div className="flex items-baseline justify-between gap-3">
        <h3 className="text-base font-semibold">{title}</h3>
        <span className="text-xs text-subtle">{detail}</span>
      </div>
      <ul className="mt-2 flex flex-wrap gap-x-5 gap-y-1 text-sm" aria-label={t("resources.now", { title })}>
        {series.map((s) => (
          <li key={s.label} className="flex items-center gap-2" title={s.title}>
            <LineKey color={s.color} />
            <span className="text-muted">{s.label}</span>
            <span className="font-semibold tabular-nums text-fg">{latest ? s.text(latest) : "—"}</span>
          </li>
        ))}
      </ul>
      <div
        className="relative mt-5 h-32 touch-none rounded-sm outline-offset-4"
        role="img"
        aria-label={summary}
        tabIndex={0}
        onPointerMove={pick}
        onPointerDown={pick}
        onPointerLeave={() => setHover(null)}
        onKeyDown={step}
        onFocus={() => setHover(hover ?? (samples.length ? samples.length - 1 : null))}
        onBlur={() => setHover(null)}
      >
        <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" className="absolute inset-0 size-full overflow-visible" aria-hidden>
          {[0, 0.5, 1].map((v) => (
            <line key={v} x1={0} x2={W} y1={y(v)} y2={y(v)} stroke="var(--color-border)" strokeWidth={1} vectorEffect="non-scaling-stroke" />
          ))}
          {series
            .slice()
            .reverse()
            .map((s) => {
              const points = samples.map((p): [number, number, number] => [x(p.at), y(s.value(p)), p.at]);
              return (
                <g key={s.label}>
                  <path d={areaPath(points, gapMs)} fill={s.color} fillOpacity={0.1} />
                  <path
                    d={linePath(points, gapMs)}
                    fill="none"
                    stroke={s.color}
                    strokeWidth={2}
                    strokeLinejoin="round"
                    strokeLinecap="round"
                    vectorEffect="non-scaling-stroke"
                  />
                </g>
              );
            })}
          {hover != null && shown ? (
            <line x1={x(shown.at)} x2={x(shown.at)} y1={0} y2={H} stroke="var(--color-muted)" strokeWidth={1} vectorEffect="non-scaling-stroke" />
          ) : null}
        </svg>
        {/* Each just above its line, out of the lines' way. */}
        <span className="pointer-events-none absolute top-0 right-0 -translate-y-full pb-0.5 text-[10px] leading-none tabular-nums text-subtle">
          {marks[0]}
        </span>
        <span className="pointer-events-none absolute top-1/2 right-0 -translate-y-full pb-0.5 text-[10px] leading-none tabular-nums text-subtle">
          {marks[1]}
        </span>
        {hover != null && shown ? (
          <>
            {series.map((s) => (
              <span
                key={s.label}
                aria-hidden
                className="pointer-events-none absolute size-2.5 -translate-x-1/2 -translate-y-1/2 rounded-full ring-2 ring-surface"
                style={{ left: `${(x(shown.at) / W) * 100}%`, top: `${(y(s.value(shown)) / H) * 100}%`, background: s.color }}
              />
            ))}
            <div
              className={cn(
                "pointer-events-none absolute top-1 z-10 min-w-36 rounded-md border border-border-strong bg-surface-2/95 px-3 py-2 text-xs shadow-lg backdrop-blur",
                x(shown.at) > W / 2 ? "-translate-x-[calc(100%+0.75rem)]" : "translate-x-3",
              )}
              style={{ left: `${(x(shown.at) / W) * 100}%` }}
            >
              <div className="mb-1 text-subtle tabular-nums">{new Date(shown.at).toLocaleTimeString(currentLocale())}</div>
              {series.map((s) => (
                <div key={s.label} className="flex items-center gap-2">
                  <LineKey color={s.color} />
                  <span className="font-semibold tabular-nums text-fg">{s.text(shown)}</span>
                  <span className="text-muted">{s.label}</span>
                </div>
              ))}
            </div>
          </>
        ) : null}
      </div>
      <div className="mt-1.5 flex justify-between text-[10px] text-subtle">
        <span>{t("resources.fiveMinutesAgo")}</span>
        <span>{t("resources.rightNow")}</span>
      </div>
    </div>
  );
}

/** Who Pley is sending to, the busiest first, each with a bar of its share. */
function Clients({ clients }: { clients: ResourceClient[] }) {
  const { t } = useTranslation("settings");
  const total = clients.reduce((sum, c) => sum + c.sent, 0) || 1;
  return (
    <div className="rounded-lg border border-border bg-surface p-4">
      <div className="flex items-baseline justify-between gap-3">
        <h3 className="text-base font-semibold">{t("resources.clients")}</h3>
        <span className="text-xs text-subtle">{t("resources.clientCount", { count: clients.length })}</span>
      </div>
      {clients.length === 0 ? (
        <p className="mt-6 text-center text-sm text-subtle">{t("resources.noClients")}</p>
      ) : (
        <ul className="mt-3 max-h-56 space-y-2.5 overflow-y-auto pr-1" aria-label={t("resources.clients")}>
          {clients.map((c) => {
            // The account first, then the device and address it is on; with no
            // sign-in, the device or address in its place.
            const [name = t("resources.others"), ...rest] = [c.user, c.device, c.address].filter((part): part is string => !!part);
            const about = rest.join(" · ");
            return (
              <li key={`${c.user}|${c.device}|${c.address}`} className="flex items-center gap-3 text-sm">
                <Monitor aria-hidden className="size-4 shrink-0 text-subtle" />
                <div className="min-w-0 flex-1">
                  <div className="flex items-baseline justify-between gap-3">
                    <span className="truncate font-bold text-fg" title={name}>
                      {name}
                    </span>
                    <span className="shrink-0 font-semibold tabular-nums text-fg">{speed(c.sent)}</span>
                  </div>
                  {about ? <div className="truncate text-xs text-subtle">{about}</div> : null}
                  <div aria-hidden className="mt-1 h-1 overflow-hidden rounded-full bg-surface-2">
                    <div className="h-full rounded-full" style={{ width: `${(c.sent / total) * 100}%`, background: PLEY }} />
                  </div>
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

/**
 * How busy the machine is and how much of it pley takes, as Plex's
 * dashboard shows it: the processors, the memory and the network over five
 * minutes, and who Pley is sending to now.
 */
export function ResourcesSection() {
  const { t } = useTranslation("settings");
  const resources = useResources();
  const data = resources.data;

  let body: ReactNode;
  if (resources.isError) {
    body = (
      <p className="rounded-lg border border-danger/40 bg-danger/10 px-4 py-3 text-sm text-danger">
        {t("resources.couldntRead", { error: errorMessage(resources.error) })}
      </p>
    );
  } else if (!data) {
    body = (
      <div className="grid gap-3 lg:grid-cols-2">
        <Skeleton className="h-60" />
        <Skeleton className="h-60" />
        <Skeleton className="h-60" />
        <Skeleton className="h-60" />
      </div>
    );
  } else if (!data.available) {
    body = (
      <p className="rounded-lg border border-dashed border-border px-4 py-6 text-center text-sm text-subtle">
        {t("resources.unmeasurable")}
      </p>
    );
  } else {
    const total = data.memory_total || 1;
    const pleyHint = t("resources.pleyHint");
    const system = t("resources.system");
    const top = networkScale(data.samples);
    const peak = Math.max(0, ...data.samples.map((s) => Math.max(s.net_in, s.net_out, s.pley_out)));
    body = (
      <div className="grid gap-3 lg:grid-cols-2">
        <Chart
          title={t("resources.cpu")}
          detail={t("resources.processors", { count: data.cpus })}
          data={data}
          series={[
            { label: "Pley", color: PLEY, value: (s) => s.pley_cpu / 100, text: (s) => percent(s.pley_cpu), title: pleyHint },
            { label: system, color: SYSTEM, value: (s) => s.cpu / 100, text: (s) => percent(s.cpu) },
          ]}
        />
        <Chart
          title={t("resources.memory")}
          detail={t("resources.inAll", { size: formatBytes(data.memory_total) })}
          data={data}
          series={[
            { label: "Pley", color: PLEY, value: (s) => s.pley_memory / total, text: (s) => formatBytes(s.pley_memory), title: pleyHint },
            { label: system, color: SYSTEM, value: (s) => s.memory / total, text: (s) => formatBytes(s.memory) },
          ]}
        />
        <Chart
          title={t("resources.network")}
          detail={t("resources.fastest", { speed: speed(peak) })}
          data={data}
          marks={[speed(top), speed(top / 2)]}
          series={[
            { label: "Pley", color: PLEY, value: (s) => s.pley_out / top, text: (s) => speed(s.pley_out), title: t("resources.pleyNetworkHint") },
            { label: t("resources.upload"), color: SYSTEM, value: (s) => s.net_out / top, text: (s) => speed(s.net_out) },
            { label: t("resources.download"), color: RECEIVED, value: (s) => s.net_in / top, text: (s) => speed(s.net_in) },
          ]}
        />
        <Clients clients={data.clients} />
      </div>
    );
  }

  return (
    <section>
      <SectionTitle>
        <span className="flex items-center gap-2">
          <Gauge className="size-5 text-accent" /> {t("resources.title")}
        </span>
      </SectionTitle>
      {body}
    </section>
  );
}
