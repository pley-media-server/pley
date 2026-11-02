import { useEffect, useLayoutEffect, useRef, useState } from "preact/hooks";
import { errorMessage, isApiError, unreachable, unreachableReason } from "../../../api/client";
import type { HouseholdProfile, User } from "../../../api/types";
import { SignedOutError, TokenError } from "../../../auth/tokens";
import { useUi } from "../../context";
import { Pill, TextButton } from "../../kit/buttons";
import { F, G } from "../../kit/focusable";
import { sentence, useApp } from "../common";
import { ApproveContent } from "./link";
import { keyId, NumberPad, PIN_KEYS, ProfileAvatar, type PadKey } from "./kit";

type Step =
  | { kind: "pick" }
  | { kind: "pin"; profile: HouseholdProfile; confirm: boolean; locked: boolean; error: string | null }
  | { kind: "approve"; profile: HouseholdProfile | null };

/** PINs are 4 to 8 digits. */
export const PIN_MIN = 4;
export const PIN_MAX = 8;

const ITEM_W = 100;
const ITEM_GAP = 40;
/** More than this many profiles scroll, the focused one's left edge at x 288. */
const FIT = 6;

/**
 * Why a switch was refused, in a sentence, and what it leads to. `offline`:
 * the server answered nothing, so the PIN wasn't checked and isn't wrong
 * (the pad doesn't shake).
 */
export function switchRefusal(e: unknown): { message: string; locked?: boolean; approve?: boolean; letThrough?: boolean; offline?: boolean } {
  if (unreachable(e)) {
    const reason = unreachableReason(e);
    return { message: `Can't reach your server: ${reason.charAt(0).toLowerCase()}${reason.slice(1)}`, offline: true };
  }
  if (e instanceof TokenError) {
    const reason = e.oauth?.reason;
    if (e.oauth?.error === "invalid_request" && !reason) return { message: "", letThrough: true };
    switch (reason) {
      case "wrong_pin":
        return { message: "Wrong PIN." };
      case "throttled":
        return { message: sentence(e.oauth?.description || "too many wrong PINs: try again in a few minutes") };
      case "pin_locked":
        return {
          message: "This profile's PIN is locked after too many wrong tries. Its owner or an administrator can reset it.",
          locked: true,
        };
      case "browser":
        return { message: "", approve: true };
      case "not_allowed":
        return { message: "You can't switch to this profile." };
      default:
        return { message: e.status >= 500 ? "The server refused: try again." : sentence(e.message) };
    }
  }
  return { message: sentence(errorMessage(e)) };
}

/** Who the app is, as far as "Who's watching?" needs to know. */
type Me = Pick<User, "id" | "has_pin">;

/**
 * The household as "Who's watching?" shows it (ProfilesViewModel's
 * tiles). A saved list may be from before a switch, so `me` says which
 * profile is current. The server says switching to that one needs nothing;
 * it shows its lock when it has a PIN, which a start asks.
 */
export function profileTiles(household: HouseholdProfile[], me: Me | null): HouseholdProfile[] {
  if (!me || !household.some((p) => p.id === me.id)) return household;
  return household.map((p) => (p.id === me.id ? { ...p, current: true, needs: me.has_pin ? "pin" : "none" } : { ...p, current: false }));
}

/**
 * What choosing `p` leads to: `confirm`, the current profile's PIN, checked
 * with the switch-profile grant; `done`, Home as the current profile;
 * `switch`, a switch without a PIN; `pin`, the pad for another profile's;
 * `approve`, approval on a phone; `locked`, why it can't be used. `gate`:
 * a start, where going on as a profile with a PIN asks it.
 */
export function profileChoice(p: HouseholdProfile, me: Me | null, gate: boolean): "confirm" | "done" | "switch" | "pin" | "approve" | "locked" {
  if (p.current) return gate && me?.id === p.id && me.has_pin ? "confirm" : "done";
  switch (p.needs) {
    case "none":
      return "switch";
    case "pin":
      return "pin";
    case "password":
      return "approve";
    default:
      return "locked";
  }
}

/**
 * "Who's watching?" (TvProfilesScreen): the household's profiles, a PIN
 * pad for those with one, and approval on a phone for those that need a
 * password. At a cold start the current profile's PIN is asked too.
 */
export function ProfilesScreen() {
  const app = useApp();
  const ui = useUi();
  const [household, setHousehold] = useState<HouseholdProfile[]>(() => app.savedHousehold());
  const profiles = profileTiles(household, app.me.value);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [step, setStep] = useState<Step>({ kind: "pick" });
  const [busy, setBusy] = useState<number | null>(null);
  const lastFocused = useRef<number | null>(null);
  const canSwitch = (app.tokenStore?.scopes() ?? []).indexOf("profiles") >= 0;
  const alive = useRef(true);
  useEffect(() => () => void (alive.current = false), []);

  const load = () => {
    const api = app.api.value;
    if (!api) return;
    setLoading(true);
    // Who the app is, for the current profile's PIN; alongside the list.
    const me = app.me.value ? Promise.resolve() : app.loadAccount();
    Promise.all([api.profiles(), me]).then(
      ([list]) => {
        if (!alive.current) return;
        app.keepHousehold(list);
        setHousehold(list);
        setLoading(false);
      },
      (e) => {
        if (!alive.current || e instanceof SignedOutError) return;
        setLoading(false);
        // With a list on screen, trouble stays quiet.
        setHousehold((shown) => {
          if (!shown.length) setError(isApiError(e, 403) ? "This sign-in can't switch profiles." : sentence(errorMessage(e)));
          return shown;
        });
      },
    );
  };
  useEffect(load, []);

  const done = () => {
    app.profileGatePassed = true;
    ui.nav.restart({ name: "home" });
  };

  const switchTo = async (p: HouseholdProfile, pin: string | null): Promise<void> => {
    const tokens = app.tokenStore;
    if (!tokens) return;
    await tokens.switchProfile(p.id, pin);
    await app.switched().catch(() => undefined);
  };

  const pick = (p: HouseholdProfile) => {
    if (busy !== null) return;
    setError(null);
    lastFocused.current = p.id;
    switch (profileChoice(p, app.me.value, !app.profileGatePassed)) {
      case "confirm":
        setStep({ kind: "pin", profile: p, confirm: true, locked: false, error: null });
        return;
      case "done":
        done();
        return;
      case "switch":
        setBusy(p.id);
        switchTo(p, null).then(
          () => alive.current && done(),
          (e) => {
            if (!alive.current) return;
            setBusy(null);
            const r = switchRefusal(e);
            if (r.approve) setStep({ kind: "approve", profile: p });
            else setError(r.message);
          },
        );
        return;
      case "pin":
        setStep({ kind: "pin", profile: p, confirm: false, locked: false, error: null });
        return;
      case "approve":
        setStep({ kind: "approve", profile: p });
        return;
      case "locked":
        setStep({ kind: "pin", profile: p, confirm: false, locked: true, error: "This profile can't be used from the app." });
    }
  };

  const backToPick = () => setStep({ kind: "pick" });

  if (step.kind === "approve") {
    return (
      <div class="step-fade" key="approve">
        <ApproveContent
          server={app.server.value ?? ""}
          profile={step.profile ? { id: step.profile.id, name: step.profile.name } : null}
          previousUser={profiles.filter((x) => x.current)[0]?.id ?? app.me.value?.id ?? null}
          onBack={backToPick}
        />
      </div>
    );
  }
  if (step.kind === "pin") {
    return (
      <div class="step-fade plain-bg" key="pin">
        <PinPad
          profile={step.profile}
          locked={step.locked}
          initialError={step.error}
          onBack={backToPick}
          onSubmit={async (pin) => {
            try {
              await switchTo(step.profile, pin);
              if (alive.current) done();
              return null;
            } catch (e) {
              const r = switchRefusal(e);
              if (r.letThrough) {
                done();
                return null;
              }
              if (r.approve) {
                setStep({ kind: "approve", profile: step.profile });
                return null;
              }
              return { message: r.message, locked: !!r.locked, offline: !!r.offline };
            }
          }}
        />
      </div>
    );
  }

  const initialId =
    (lastFocused.current !== null && profiles.some((x) => x.id === lastFocused.current) ? lastFocused.current : null) ??
    profiles.filter((x) => x.current)[0]?.id ??
    profiles[0]?.id ??
    null;
  return (
    <div class="step-fade plain-bg" key="pick">
      <div class="t-page c-fg" style="position:absolute;left:0;right:0;top:110px;text-align:center">
        Who's watching?
      </div>
      <ProfileRow
        profiles={profiles}
        busy={busy}
        initialId={initialId}
        onFocusId={(id) => (lastFocused.current = id)}
        onPick={pick}
      />
      <div class="profiles-bottom">
        {error && (
          <div class="t-body c-danger" style="width:560px;text-align:center">
            {error}
          </div>
        )}
        <G kind="col" class="profiles-buttons">
          {!profiles.length && !loading && (
            <div style="margin-top:12px">
              <Pill label="Retry" icon="refresh-cw" initial onSelect={load} />
            </div>
          )}
          {!canSwitch && (
            <div style="margin-top:12px">
              <TextButton label="Sign in as another profile" onSelect={() => setStep({ kind: "approve", profile: null })} />
            </div>
          )}
        </G>
      </div>
    </div>
  );
}

function ProfileRow(p: { profiles: HouseholdProfile[]; busy: number | null; initialId: number | null; onFocusId: (id: number) => void; onPick: (p: HouseholdProfile) => void }) {
  const n = p.profiles.length;
  const [focusedAt, setFocusedAt] = useState(Math.max(0, p.profiles.findIndex((x) => x.id === p.initialId)));
  const span = n * ITEM_W + Math.max(0, n - 1) * ITEM_GAP;
  let left: number;
  if (n <= FIT) left = (960 - span) / 2;
  else {
    // The focused profile's left edge at x 288, the row never past its ends (48 padding).
    const want = 288 - focusedAt * (ITEM_W + ITEM_GAP);
    left = Math.min(48, Math.max(960 - 48 - span, want));
  }
  return (
    <div class="profile-row">
      <G kind="row" restore class="profile-track" style={`transform:translateX(${left}px)`}>
        {p.profiles.map((x, i) => (
          <F
            key={x.id}
            class={`profile-choice${p.busy === x.id ? " busy" : ""}`}
            style={`left:${i * (ITEM_W + ITEM_GAP)}px`}
            fid={`profile/${x.id}`}
            initial={x.id === p.initialId}
            label={x.needs === "pin" ? `${x.name}, protected by a PIN` : x.name}
            onFocus={() => {
              setFocusedAt(i);
              p.onFocusId(x.id);
            }}
            onSelect={() => p.onPick(x)}
          >
            <div class="profile-ring-box">
              <ProfileAvatar name={x.name} color={x.avatar_color} picture={x.avatar_url} size={100} locked={x.needs === "pin"} />
              <div class="profile-ring" />
            </div>
            <div class="profile-name t-label">{x.name}</div>
          </F>
        ))}
      </G>
    </div>
  );
}

/**
 * The PIN pad (TvPinPad): dots for the digits typed, 4 to 8 of them, sent
 * with ✓; a wrong one clears the dots and shakes them. Locked, the pad is
 * dimmed and "Use another profile" takes focus.
 */
function PinPad(p: {
  profile: HouseholdProfile;
  locked: boolean;
  initialError: string | null;
  onBack: () => void;
  onSubmit: (pin: string) => Promise<{ message: string; locked: boolean; offline?: boolean } | null>;
}) {
  const ui = useUi();
  const [digits, setDigits] = useState("");
  const [error, setError] = useState<string | null>(p.initialError);
  const [locked, setLocked] = useState(p.locked);
  const [shakes, setShakes] = useState(0);
  const [flash, setFlash] = useState<{ key: string; n: number } | null>(null);
  const busy = useRef(false);
  const pressed = useRef(false);
  const digitsRef = useRef(digits);
  digitsRef.current = digits;
  const lockedRef = useRef(locked);
  lockedRef.current = locked;

  const submit = () => {
    const pin = digitsRef.current;
    if (busy.current || lockedRef.current || pin.length < PIN_MIN) return;
    busy.current = true;
    setDigits("");
    setError(null);
    p.onSubmit(pin).then((refused) => {
      busy.current = false;
      if (!refused) return;
      setError(refused.message);
      // A PIN the server couldn't check isn't wrong: the dots clear without shaking.
      if (!refused.offline) setShakes((n) => n + 1);
      if (refused.locked) setLocked(true);
    });
  };
  const press = (k: PadKey) => {
    if (busy.current || lockedRef.current) return;
    if (k.kind === "delete") setDigits((d) => d.slice(0, -1));
    else if (k.kind === "ok") submit();
    else if (/^\d$/.test(k.text)) setDigits((d) => (d.length < PIN_MAX ? d + k.text : d));
  };

  useEffect(
    () =>
      ui.input.push((key, e) => {
        if (key === "back") {
          if (e.type === "down" && !e.repeat) p.onBack();
          return true;
        }
        const digit = key.indexOf("digit") === 0;
        if (!digit && key !== "backspace") {
          if (e.type === "down") pressed.current = true;
          return false;
        }
        if (e.type !== "down") return true;
        pressed.current = true;
        const k: PadKey = digit ? { kind: "text", text: key.slice(5) } : { kind: "delete" };
        setFlash({ key: keyId(k), n: Date.now() });
        const enough = digit && digitsRef.current.length + 1 >= PIN_MIN;
        press(k);
        // A PIN typed with the number keys is sent with OK: ✓ takes focus once it could be sent, rather than 5 typing a 5.
        if (enough && !lockedRef.current) {
          const ok = document.querySelector<HTMLElement>('.pin-pad [data-fid="pin/ok"]');
          if (ok) ui.focus.focus(ok);
        }
        return true;
      }),
    [],
  );

  // Focus on "5" again once the step has faded in, unless a key was pressed; locked, on "Use another profile".
  const anotherRef = useRef<HTMLElement | null>(null);
  useLayoutEffect(() => {
    if (locked) {
      const el = anotherRef.current?.querySelector<HTMLElement>("[data-f]");
      if (el) ui.focus.focus(el);
      return;
    }
    const t = setTimeout(
      () => {
        if (pressed.current) return;
        const five = document.querySelector<HTMLElement>('.pin-pad [data-fid="pin/5"]');
        if (five) ui.focus.focus(five);
      },
      ui.reducedMotion ? 0 : 250,
    );
    return () => clearTimeout(t);
  }, [locked]);

  const dots = Math.max(PIN_MIN, digits.length);
  const dotEls = [];
  for (let i = 0; i < dots; i++) dotEls.push(<span key={i} class={`pin-dot${i < digits.length ? " on" : ""}`} />);
  return (
    <div class="pin-pad">
      <ProfileAvatar name={p.profile.name} color={p.profile.avatar_color} picture={p.profile.avatar_url} size={56} />
      <div class="t-title c-fg" style="margin-top:14px">
        Enter {p.profile.name}'s PIN
      </div>
      <div
        key={shakes}
        class={`pin-dots${shakes && !ui.reducedMotion ? " shake" : ""}`}
        aria-live="polite"
        aria-label={`${digits.length} ${digits.length === 1 ? "digit" : "digits"} entered`}
      >
        {dotEls}
      </div>
      <div class="pin-error t-body c-danger one-line" aria-live="assertive">
        {error ?? ""}
      </div>
      <NumberPad rows={PIN_KEYS} onKey={press} enabled={!locked} flash={flash} fid="pin" />
      <div style="margin-top:16px" ref={(el) => {
          anotherRef.current = el;
        }}>
        {locked ? (
          <G kind="row">
            <TextButton label="Use another profile" onSelect={p.onBack} />
          </G>
        ) : (
          <div class="t-caption c-muted">You can also type with the number keys on your remote.</div>
        )}
      </div>
    </div>
  );
}
