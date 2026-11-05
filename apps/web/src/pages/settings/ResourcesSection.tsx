import { useState, type KeyboardEvent, type PointerEvent, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Gauge } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useResources } from "@/api/hooks";
import type { ResourceSample, Resources } from "@/api/types";
import { SectionTitle, Skeleton } from "@/components/ui";
import { currentLocale } from "@/i18n";
import { cn } from "@/lib/cn";
import { formatBytes } from "@/lib/format";

/** What the charts span, as the server keeps it. */
const WINDOW_MS = 5 * 60_000;
/** The drawing's own units; it stretches to the card's width. */
const W = 600;
const H = 120;

/** pley in the brand's light blue; the whole machine, behind it, in gray. */
const PLEY = "var(--color-accent)";
const SYSTEM = "#7a8292";

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
function Chart({ title, detail, data, series }: { title: string; detail: ReactNode; data: Resources; series: Series[] }) {
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
          {percent(100)}
        </span>
        <span className="pointer-events-none absolute top-1/2 right-0 -translate-y-full pb-0.5 text-[10px] leading-none tabular-nums text-subtle">
          {percent(50)}
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

/**
 * How busy the machine is and how much of it pley takes, as Plex's
 * dashboard shows it: the processors and the memory over five minutes.
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
