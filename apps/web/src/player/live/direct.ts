import type { SessionPlan } from "@/api/livetv";
import { playsStreamTrack } from "@/lib/caps";

/**
 * What mpegts.js found in a stream (its `mediaInfo`), each track by the
 * codec it wrote for it: "avc1.640028", "hvc1.2.4.L153.B0", "mp4a.40.2",
 * "ac-3". It says so once it has read a stream's first picture and sound,
 * and never of a stream with only one of them.
 */
export interface StreamTracks {
  hasVideo?: boolean | null;
  hasAudio?: boolean | null;
  videoCodec?: string | null;
  audioCodec?: string | null;
}

/**
 * Whether a channel plays from its stream, by the tracks mpegts.js found
 * in it. H.264 and AAC play wherever mpegts.js does, others when the
 * browser plays each as the stream has it. `plan`: what the server said the channel is, when it did:
 * with a track of the plan's missing (AC-3 as DVB carries it, which
 * mpegts.js doesn't read), the picture would play without its sound.
 */
export function tracksPlay(tracks: StreamTracks | null | undefined, plan: SessionPlan | null): boolean {
  if (!tracks) return false;
  const video = tracks.hasVideo ? (tracks.videoCodec ?? "") : null;
  const audio = tracks.hasAudio ? (tracks.audioCodec ?? "") : null;
  if (video === null && audio === null) return false;
  if (plan && ((plan.video !== "none" && video === null) || (plan.audio !== "none" && audio === null))) return false;
  if ((video === null || video.startsWith("avc")) && (audio === null || audio.startsWith("mp4a"))) return true;
  return (
    (video === null || playsStreamTrack("video", video)) &&
    (audio === null || playsStreamTrack("audio", audio, plan?.audio_codec))
  );
}
