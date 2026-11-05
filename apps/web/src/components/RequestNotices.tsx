import { useCallback, useEffect, useRef } from "react";
import { useLocation, useNavigate } from "react-router";
import { useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api, qs } from "@/api/client";
import { onServerEvent } from "@/api/events";
import { requestKeys, requestQueries, useRequestSummary, type RequestList, type RequestStatus } from "@/api/requests";
import { useAuth } from "@/auth";
import { eventsLive } from "@/lib/connection";
import { getStored, writeStorage } from "@/lib/hooks";
import {
  createdText,
  decidedText,
  failedText,
  freshNews,
  markAnnounced,
  markTold,
  newsCountText,
  newsText,
  NO_NEWS,
  readyText,
  type NewsMark,
} from "@/lib/requests";
import type { Id } from "@/api/types";
import { itemUrl, requestsUrl, titleUrl } from "@/lib/urls";
import { useToast } from "./Toast";

interface Notice {
  text: string;
  tone: "success" | "info" | "error";
  /** Where Open goes. */
  to: string;
}

/** How long a request's notice stays. */
const NOTICE_MS = 10_000;
/** Notices kept while a video plays, to show once it stops. */
const HELD = 3;
/** An event explains a rise in the unseen count for this long (the summary follows it a second later). */
const EXPLAINS_MS = 10_000;

/** A video plays: the player, or a Live TV channel (`/live/:channelId`, `/live/:sourceUid/:channelId`), not Live TV's page. */
const PLAYING = /^\/(play|live)\/[^/]/;

/** Where this profile's notices keep what they told, on this server (read each time: other tabs tell too). */
const markKey = (userId: number) => `pley.requestNews.${userId}`;

function readMark(userId: number): NewsMark {
  const stored = getStored<Partial<NewsMark> | null>(markKey(userId), null);
  return {
    at: typeof stored?.at === "number" ? stored.at : NO_NEWS.at,
    told: stored?.told && typeof stored.told === "object" ? stored.told : {},
  };
}

/**
 * Notices of requests' news, app-wide: ready to watch, approved or
 * declined, and for managers new requests waiting and those that failed.
 * At start, and when the summary's unseen count rises with no event to
 * explain it, one notice tells what came meanwhile (never a request's
 * news twice). None shows over a playing video: they wait for it to stop,
 * the last three kept. Without the event stream, a change of the
 * summary's `changed_at` refreshes what shows requests.
 */
export function RequestNotices() {
  const { t } = useTranslation("components");
  const me = useAuth();
  const toast = useToast();
  const navigate = useNavigate();
  const location = useLocation();
  const qc = useQueryClient();
  const summary = useRequestSummary();
  const playing = PLAYING.test(location.pathname);
  const playingRef = useRef(playing);
  const held = useRef<Notice[]>([]);
  const explainedAt = useRef(0);
  const userId = me.id;
  const told = useCallback(
    (requestId: Id, status: RequestStatus, seasons: number[]) => writeStorage(markKey(userId), markTold(readMark(userId), requestId, status, seasons)),
    [userId],
  );

  const toastNotice = useCallback(
    (notice: Notice) => {
      const message = (
        <span>
          {notice.text}
          <button type="button" onClick={() => void navigate(notice.to)} className="ml-2 font-medium text-accent hover:underline">
            {t("requestNotices.open")}
          </button>
        </span>
      );
      if (notice.tone === "error") toast.error(message, { duration: NOTICE_MS });
      else if (notice.tone === "success") toast.success(message, { duration: NOTICE_MS });
      else toast.info(message, { duration: NOTICE_MS });
    },
    [toast, navigate, t],
  );

  const show = useCallback(
    (notice: Notice) => {
      if (playingRef.current) held.current = [...held.current, notice].slice(-HELD);
      else toastNotice(notice);
    },
    [toastNotice],
  );

  // The video stopped: what waited shows now.
  useEffect(() => {
    playingRef.current = playing;
    if (playing || !held.current.length) return;
    const waiting = held.current;
    held.current = [];
    waiting.forEach(toastNotice);
  }, [playing, toastNotice]);

  // Live: each event as it comes.
  const manage = summary.data?.manage ?? false;
  useEffect(
    () =>
      onServerEvent((ev) => {
        switch (ev.type) {
          case "request_available":
            explainedAt.current = Date.now();
            if (ev.complete) told(ev.request_id, "available", ev.seasons ?? []);
            else {
              told(ev.request_id, "approved", ev.seasons ?? []);
              // The event names the seasons newly ready: those ready before them come with the request, so
              // that a catch-up tells only of seasons ready after these.
              void qc.fetchQuery(requestQueries.one(ev.request_id)).then(
                (request) => told(request.id, request.status, request.available_seasons),
                () => {},
              );
            }
            show({
              text: readyText(ev.title, ev.seasons, ev.complete),
              tone: "success",
              to: ev.item_id !== null ? itemUrl(ev.item_id) : titleUrl(ev.kind, ev.tmdb_id),
            });
            break;
          case "request_decided":
            explainedAt.current = Date.now();
            told(ev.request_id, ev.status, []);
            show({ text: decidedText(ev.title, ev.status, ev.reason), tone: ev.status === "declined" ? "info" : "success", to: titleUrl(ev.kind, ev.tmdb_id) });
            break;
          case "request_created":
            // Managers hear of those waiting for them, not those approved as they were made.
            if (manage && ev.status === "pending") show({ text: createdText(ev.requested_by.name, ev.title, ev.seasons), tone: "info", to: requestsUrl });
            break;
          case "request_failed":
            if (manage) show({ text: failedText(ev.title, ev.kind, ev.reason), tone: "error", to: requestsUrl });
            break;
        }
      }),
    [show, told, manage, qc],
  );

  // Catching up: what came while this page wasn't listening, in one notice.
  const catchUp = useCallback(async () => {
    let list: RequestList;
    try {
      list = await api.get<RequestList>(`/api/requests${qs({ unseen: 1, limit: 20 })}`);
    } catch {
      return;
    }
    const mark = readMark(userId);
    const fresh = freshNews(list.requests, mark);
    if (!fresh.length) return;
    writeStorage(markKey(userId), markAnnounced(mark, fresh));
    const [only] = fresh;
    const unfetched = Math.max(0, list.total - list.requests.length);
    const single = only && fresh.length === 1 && !unfetched ? newsText(only) : null;
    if (single && only) {
      const to = only.item_id !== null && only.status !== "declined" ? itemUrl(only.item_id) : titleUrl(only.kind, only.tmdb_id);
      show({ text: single, tone: only.status === "declined" ? "info" : "success", to });
    } else {
      show({ text: newsCountText(fresh.length + unfetched), tone: "info", to: requestsUrl });
    }
  }, [userId, show]);

  const unseen = summary.data?.enabled ? summary.data.unseen : null;
  const lastUnseen = useRef<number | null>(null);
  useEffect(() => {
    if (unseen === null) return;
    const before = lastUnseen.current;
    lastUnseen.current = unseen;
    const rose = before === null ? unseen > 0 : unseen > before;
    if (rose && Date.now() - explainedAt.current > EXPLAINS_MS) void catchUp();
  }, [unseen, catchUp]);

  // Without the event stream, what shows requests follows the summary.
  const changedAt = summary.data?.changed_at ?? null;
  const lastChanged = useRef<number | null>(null);
  useEffect(() => {
    const before = lastChanged.current;
    lastChanged.current = changedAt;
    if (before === null || changedAt === before || eventsLive()) return;
    for (const queryKey of [requestKeys.lists, requestKeys.titles, requestKeys.items, requestKeys.ones]) {
      void qc.invalidateQueries({ queryKey });
    }
  }, [changedAt, qc]);

  return null;
}
