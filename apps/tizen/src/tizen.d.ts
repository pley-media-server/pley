/**
 * The parts of Samsung's TV APIs pley uses: the Tizen Web Device API
 * (`tizen`) and Samsung's Product API (`webapis`, loaded from
 * $WEBAPIS/webapis/webapis.js). Written from Samsung's references; the
 * TV is the source of truth, so every call is guarded.
 */

interface AvplayTrack {
  index: number;
  type: "VIDEO" | "AUDIO" | "TEXT";
  /** JSON as a string: {"language":"eng","channels":"6","fourCC":"ac-3",…}. */
  extra_info: string;
}

interface AvplayListener {
  onbufferingstart?: () => void;
  onbufferingprogress?: (percent: number) => void;
  onbufferingcomplete?: () => void;
  oncurrentplaytime?: (ms: number) => void;
  onstreamcompleted?: () => void;
  onevent?: (event: string, data: string) => void;
  onerror?: (error: string) => void;
  onerrormsg?: (id: string, message: string) => void;
  onsubtitlechange?: (duration: number, text: string, type: string, attributes: unknown) => void;
  ondrmevent?: (type: string, data: unknown) => void;
}

interface Avplay {
  open(url: string): void;
  close(): void;
  prepare(): void;
  prepareAsync(ok: () => void, err: (e: unknown) => void): void;
  play(): void;
  pause(): void;
  stop(): void;
  seekTo(ms: number, ok?: () => void, err?: (e: unknown) => void): void;
  getCurrentTime(): number;
  getDuration(): number;
  getState(): "NONE" | "IDLE" | "READY" | "PLAYING" | "PAUSED";
  setListener(listener: AvplayListener): void;
  setDisplayRect(x: number, y: number, w: number, h: number): void;
  setDisplayMethod(method: string): void;
  setStreamingProperty(type: string, value: string): void;
  getStreamingProperty(type: string): string;
  setBufferingParam(option: string, unit: string, amount: number): void;
  getTotalTrackInfo(): AvplayTrack[];
  getCurrentStreamInfo(): AvplayTrack[];
  setSelectTrack(type: "AUDIO" | "TEXT", index: number): void;
  setSilentSubtitle(silent: boolean): void;
  suspend(): void;
  restore(url?: string, resumeTimeMs?: number, prepare?: boolean): void;
}

interface Webapis {
  avplay: Avplay;
  productinfo?: {
    getVersion(): string;
    getFirmware(): string;
    getModel(): string;
    getRealModel(): string;
    getModelCode(): string;
    isUdPanelSupported(): boolean;
    is8KPanelSupported?(): boolean;
  };
  avinfo?: { isHdrTvSupport(): boolean };
  systeminfo?: {
    isSupportedVideoCodec?(name: string): boolean;
    isSupportedAudioCodec?(name: string): boolean;
  };
  appcommon?: {
    setScreenSaver(state: number, ok?: () => void, err?: (e: unknown) => void): void;
    AppCommonScreenSaverState: { SCREEN_SAVER_OFF: number; SCREEN_SAVER_ON: number };
  };
  network?: { getIp(): string };
  tvinfo?: {
    TvInfoMenuKey: { VOICE_GUIDE_KEY: number };
    TvInfoMenuValue: { OFF: number; ON: number };
    getMenuValue(key: number): number | string | boolean;
    addCaptionChangeListener(key: number, listener: () => void): number;
  };
}

interface TizenKey {
  name: string;
  code: number;
}

/** The TV's menu language and region ("fr_FR"). */
interface TizenLocale {
  language: string;
  country: string;
}

interface Tizen {
  systeminfo: {
    getCapability(key: string): unknown;
    getPropertyValue(property: "LOCALE", ok: (locale: TizenLocale) => void, error?: (e: unknown) => void): void;
  };
  tvinputdevice: {
    getSupportedKeys(): TizenKey[];
    registerKey(name: string): void;
    registerKeyBatch?(names: string[], ok?: () => void, err?: (e: unknown) => void): void;
  };
  application: { getCurrentApplication(): { exit(): void; hide?(): void } };
  time?: { getTimeFormat(): string };
  tvaudiocontrol?: { getOutputMode(): string };
}

declare const webapis: Webapis | undefined;
declare const tizen: Tizen | undefined;
