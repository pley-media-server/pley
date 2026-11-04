import { describe, expect, it } from "vitest";
import type { ClientCaps, MediaStream } from "../../src/api/types";
import { audioPlaysHere } from "../../src/player/plan";
import fixture from "../fixtures/audio-rate.json";

/** The sample rates the TV takes, as the native app answers them too (AudioRateTests). */
const caps = (most: number | null) =>
  ({ audio_codecs: ["aac", "ac3", "eac3"], audio_decode: ["aac", "ac3", "eac3"], audio_passthrough: [], ...(most == null ? {} : { audio_max_sample_rate: most }) }) as unknown as ClientCaps;
const audio = (codec: string, rate: number | null): MediaStream =>
  ({ index: 1, kind: "audio", codec, language: null, title: null, default: true, forced: false, sample_rate: rate ?? undefined }) as MediaStream;

describe("the audio's sample rate", () => {
  it.each(fixture.plays)("plays here: %j", (c) => {
    expect(audioPlaysHere(audio(c.in.codec, c.in.sample_rate), caps(c.in.most))).toBe(c.out);
  });
});
