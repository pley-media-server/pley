import { useEffect, useLayoutEffect, useRef, useState } from "preact/hooks";
import { errorMessage } from "../../../api/client";
import { candidates, origin, unencrypted } from "../../../auth/addresses";
import { check, connect, ConnectError, hostPort, problemText, subnetOf, sweep, type ConnectProblem, type ServerInfo } from "../../../auth/servers";
import { useUi } from "../../context";
import { nudge } from "../../focus";
import { Pill, TextButton } from "../../kit/buttons";
import { F, G } from "../../kit/focusable";
import { TextField } from "../../kit/field";
import { Icon } from "../../kit/icons";
import { Dialog, sentence, useApp } from "../common";
import { BrandGlow, PleyLogo, StatusLine } from "./kit";

/** A row of the server list. */
interface Row {
  url: string;
  name: string;
  saved: boolean;
  /** It answered discovery or the sweep (typed servers never count). */
  found: boolean;
  unreachable: boolean;
}

type Problem = ConnectProblem | { kind: "unencrypted"; address: string } | { kind: "failed"; message: string };

function textOf(p: Problem): string {
  if (p.kind === "unencrypted") {
    return `${p.address} isn't encrypted: what you watch could be read on the way. Use its https address if it has one, or press Connect again to go ahead anyway.`;
  }
  if (p.kind === "failed") return p.message;
  return problemText(p);
}

const ROW_H = 64;
const ROW_GAP = 4;
const LIST_MAX = 300;

/**
 * "Choose your server" (TvServersScreen): the servers this TV used, then
 * those that answer on the home network; an address typed on the TV's
 * keyboard; "Search this network" asks every address of the TV's /24.
 * Choosing one opens sign-in, or Home when the TV is signed in there
 * already.
 */
export function ServersScreen() {
  const app = useApp();
  const ui = useUi();
  const [rows, setRows] = useState<Row[]>(() =>
    app.savedServers().map((s) => ({ url: s.url, name: s.name, saved: true, found: false, unreachable: false })),
  );
  const [busy, setBusy] = useState<string | null>(null);
  const [problem, setProblem] = useState<Problem | null>(null);
  const [searching, setSearching] = useState(false);
  const [searched, setSearched] = useState(false);
  const [sweeping, setSweeping] = useState(false);
  const [subnet, setSubnet] = useState<string | null>(null);
  const [entering, setEntering] = useState(false);
  const [forgetting, setForgetting] = useState<Row | null>(null);
  const [shift, setShift] = useState(0);
  const warned = useRef<string | null>(null);
  const touched = useRef(false);
  const alive = useRef(true);
  useEffect(() => () => void (alive.current = false), []);

  const add = (info: ServerInfo, found: boolean) => {
    const url = origin(info.url);
    if (!url) return;
    setRows((list) => {
      const at = list.findIndex((r) => r.url === url);
      if (at >= 0) {
        const r = list[at]!;
        const next = list.slice();
        next[at] = { ...r, found: r.found || found, unreachable: false, name: r.saved ? r.name : info.name };
        return next;
      }
      return list.concat([{ url, name: info.name, saved: false, found, unreachable: false }]);
    });
  };

  // Looking on the network as the screen opens; the saved servers that didn't answer are asked directly.
  useEffect(() => {
    let stopped = false;
    const answered = new Set<string>();
    const found = (info: ServerInfo) => {
      const url = origin(info.url);
      if (url) answered.add(url);
      if (!stopped) add(info, true);
    };
    const run = async () => {
      const local = app.platform.localAddress ? await app.platform.localAddress().catch(() => null) : null;
      const net = subnetOf(local);
      if (!stopped) setSubnet(net);
      setSearching(true);
      if (app.platform.discover) {
        await app.platform
          // A discovery answer doesn't say: signing in asks the server itself.
          .discover((s) => found({ url: s.url, name: s.name, version: s.version, twoStepRequired: false }), () => stopped)
          .catch(() => undefined);
      } else if (net && !app.savedServers().length) {
        // No UDP on this TV: a first launch sweeps the network at once.
        setSweeping(true);
        await sweep(app.http, net, found, () => stopped);
        if (!stopped) setSweeping(false);
      }
      if (stopped) return;
      setSearching(false);
      setSearched(true);
      for (const s of app.savedServers()) {
        if (answered.has(s.url)) continue;
        check(app.http, s.url).then(
          (info) => !stopped && add(info, false),
          () => !stopped && setRows((list) => list.map((r) => (r.url === s.url ? { ...r, unreachable: true } : r))),
        );
      }
    };
    void run();
    return () => {
      stopped = true;
    };
  }, []);

  const go = (info: ServerInfo) => {
    const url = origin(info.url)!;
    app.rememberServer(url, info.name);
    app.use(url);
    if (!app.hasTokens(url)) ui.nav.restart({ name: "link", server: url });
    else ui.nav.restart(app.householdSize() > 1 ? { name: "profiles" } : { name: "home" });
  };

  const choose = (url: string) => {
    if (busy) return;
    setProblem(null);
    if (unencrypted(url) && warned.current !== url) {
      warned.current = url;
      setProblem({ kind: "unencrypted", address: hostPort(url) });
      return;
    }
    setBusy(url);
    check(app.http, url).then(
      (info) => {
        if (!alive.current) return;
        setBusy(null);
        go(info);
      },
      (e) => {
        if (!alive.current) return;
        setBusy(null);
        if (e instanceof ConnectError) {
          if (e.problem.kind === "unreachable") setRows((list) => list.map((r) => (r.url === url ? { ...r, unreachable: true } : r)));
          setProblem(e.problem);
        } else setProblem({ kind: "failed", message: sentence(errorMessage(e)) });
      },
    );
  };

  const runSweep = () => {
    if (sweeping || !subnet) return;
    setSweeping(true);
    let stopped = false;
    sweep(app.http, subnet, (info) => !stopped && alive.current && add(info, true), () => stopped || !alive.current).then(() => {
      if (alive.current) {
        setSweeping(false);
        setSearched(true);
      }
    });
    return () => {
      stopped = true;
    };
  };

  // Any key on the screen ends the first-row rule below.
  useEffect(
    () =>
      ui.input.push((_key, e) => {
        if (e.type === "down") touched.current = true;
        return false;
      }),
    [],
  );

  const listRef = useRef<HTMLDivElement>(null);
  const enterRef = useRef<HTMLElement | null>(null);
  // The first row takes focus when one appears, until a key was pressed.
  useLayoutEffect(() => {
    if (touched.current || entering || forgetting) return;
    const first = listRef.current?.querySelector<HTMLElement>("[data-f]");
    if (first) ui.focus.focus(first);
    else if (searched && enterRef.current && !ui.focus.current?.isConnected) ui.focus.focus(enterRef.current);
  }, [rows.length > 0, searched, entering]);

  if (entering) {
    return (
      <AddressEntry
        problem={problem}
        onEdited={() => setProblem(null)}
        onBack={() => {
          touched.current = false;
          setEntering(false);
        }}
        onConnect={(text, setChecking) => {
          setProblem(null);
          const list = candidates(text);
          if (!list.length) {
            setProblem({ kind: "bad_address" });
            return;
          }
          // Plain http beyond the home network: warned once, then it goes ahead.
          const only = list.length === 1 ? list[0]! : null;
          if (only && unencrypted(only) && warned.current !== only) {
            warned.current = only;
            setProblem({ kind: "unencrypted", address: hostPort(only) });
            return;
          }
          setChecking(true);
          connect(app.http, text).then(
            (info) => {
              if (!alive.current) return;
              setChecking(false);
              add(info, false);
              go(info);
            },
            (e) => {
              if (!alive.current) return;
              setChecking(false);
              setProblem(e instanceof ConnectError ? e.problem : { kind: "failed", message: sentence(errorMessage(e)) });
            },
          );
        }}
      />
    );
  }

  const listHeight = rows.length ? Math.min(LIST_MAX, rows.length * ROW_H + (rows.length - 1) * ROW_GAP) : 0;
  const contentHeight = rows.length * ROW_H + Math.max(0, rows.length - 1) * ROW_GAP;
  const anySaved = rows.some((r) => r.saved);
  const savedNames = rows.filter((r) => r.saved).map((r) => r.name.trim().toLowerCase());

  return (
    <BrandGlow>
      <div class="servers">
        <div class="servers-header">
          <div class="t-page c-fg one-line" style="flex:1">
            Choose your server
          </div>
          <PleyLogo height={32} />
        </div>
        {rows.length > 0 && (
          <div class="server-list" style={`height:${listHeight}px`}>
            <div ref={listRef} class="scroll-y" style={`transform:translateY(${-shift}px)`}>
              <G kind="col">
                {rows.map((r, i) => {
                  const lookalike = r.found && !r.saved && savedNames.indexOf(r.name.trim().toLowerCase()) >= 0;
                  return (
                    <ServerRow
                      key={r.url}
                      row={r}
                      busy={busy === r.url}
                      lookalike={lookalike}
                      onFocus={() => {
                        const top = i * (ROW_H + ROW_GAP);
                        const max = Math.max(0, contentHeight - LIST_MAX);
                        if (ui.focus.byPointer) setShift((s) => nudge(s, top, top + ROW_H, 0, listHeight, 0, max));
                        else setShift(Math.max(0, Math.min(top - 90, max)));
                      }}
                      onSelect={() => choose(r.url)}
                      onLongSelect={r.saved ? () => setForgetting(r) : undefined}
                    />
                  );
                })}
              </G>
            </div>
          </div>
        )}
        <G kind="row" class="server-buttons" style={`margin-top:${rows.length ? 16 : 0}px`}>
          <F class="text-btn t-label" label="Enter an address" elRef={(el) => (enterRef.current = el)} initial={!rows.length} onSelect={() => setEntering(true)}>
            <span>Enter an address</span>
          </F>
          <TextButton label="Search this network" disabled={sweeping || !subnet} onSelect={runSweep} />
        </G>
        <div class="server-status" aria-live="polite">
          {sweeping ? (
            <>
              <StatusLine text={subnet ? `Checking ${subnet}1-254…` : "Checking this network…"} class="t-caption c-muted" />
              <div class="sweep-bar">
                <div />
              </div>
            </>
          ) : searching ? (
            <StatusLine text="Looking for Pley on your network…" waiting />
          ) : searched && !rows.length && !problem ? (
            <div class="t-body c-muted">No servers found. Make sure this TV and the server are on the same network.</div>
          ) : null}
          {problem && <div class="t-body c-danger">{textOf(problem)}</div>}
        </div>
        {anySaved && <div class="forget-hint t-caption c-subtle">Hold OK on a saved server to forget it.</div>}
      </div>
      {forgetting && (
        <Dialog
          title={`Forget ${forgetting.name}?`}
          body="Pley signs this TV out of it and deletes what it saved from it."
          onDismiss={() => setForgetting(null)}
          buttons={[
            { label: "Cancel", initial: true, onSelect: () => setForgetting(null) },
            {
              label: "Forget",
              color: "var(--danger-text)",
              onSelect: () => {
                const r = forgetting;
                setForgetting(null);
                void app.forget(r.url);
                setRows((list) => list.filter((x) => x.url !== r.url || x.found).map((x) => (x.url === r.url ? { ...x, saved: false } : x)));
              },
            },
          ]}
        />
      )}
    </BrandGlow>
  );
}

function ServerRow(p: { row: Row; busy: boolean; lookalike: boolean; onFocus: () => void; onSelect: () => void; onLongSelect?: () => void }) {
  const r = p.row;
  const line2 = p.busy ? `Connecting to ${hostPort(r.url)}…` : r.unreachable && !r.found ? "Can't reach it" : hostPort(r.url);
  const danger = !p.busy && r.unreachable && !r.found;
  const tag = p.lookalike
    ? { text: "Not your saved server", danger: true }
    : unencrypted(r.url)
      ? { text: "Not encrypted", danger: true }
      : r.found
        ? { text: "On this network", danger: false }
        : null;
  return (
    <F class="server-row" fid={`server/${r.url}`} label={`${r.name}, ${line2}`} onFocus={p.onFocus} onSelect={p.onSelect} onLongSelect={p.onLongSelect}>
      <div class="server-icon">
        <Icon name="monitor" size={22} />
      </div>
      <div class="server-text">
        <div class="t-title one-line">{r.name}</div>
        <div class={`t-caption one-line ${danger ? "c-danger" : "c-muted"} sub`}>{line2}</div>
      </div>
      {tag && (
        <div class={`server-tag t-caption ${tag.danger ? "c-danger" : "c-muted"} sub`}>
          {!tag.danger && <span class="found-dot" />}
          {tag.text}
        </div>
      )}
    </F>
  );
}

/**
 * "Enter your server's address" (TvAddressEntry): a field and Connect under
 * it. The TV's keyboard opens with the screen, so a name or a whole https
 * address types as easily as an IP; once it is closed, the remote's digit
 * keys type too.
 */
function AddressEntry(p: {
  problem: Problem | null;
  onEdited: () => void;
  onBack: () => void;
  onConnect: (text: string, setChecking: (checking: boolean) => void) => void;
}) {
  const ui = useUi();
  const [text, setText] = useState("");
  const [checking, setChecking] = useState(false);
  const textRef = useRef(text);
  textRef.current = text;
  const edit = (next: string) => {
    // Kept at once: the remote's keys can come faster than the screen redraws.
    textRef.current = next;
    p.onEdited();
    setText(next);
  };
  const connectNow = () => {
    if (checking) return;
    const t = textRef.current.trim();
    if (t) p.onConnect(t, setChecking);
  };
  // The remote's own digits, "." and Delete while the keyboard is closed; Back to the list.
  useEffect(
    () =>
      ui.input.push((key, e) => {
        if (key === "back") {
          if (e.type === "down" && !e.repeat) p.onBack();
          return true;
        }
        if (e.type !== "down") return key.indexOf("digit") === 0 || key === "period" || key === "backspace";
        if (key.indexOf("digit") === 0) edit(textRef.current + key.slice(5));
        else if (key === "period") edit(textRef.current + ".");
        else if (key === "backspace") {
          if (textRef.current) edit(textRef.current.slice(0, -1));
        } else return false;
        return true;
      }),
    [],
  );
  return (
    <BrandGlow>
      <div class="address-entry">
        <div class="t-title c-fg">Enter your server's address</div>
        <G kind="col" class="address-col">
          <div style="margin-top:8px">
            <TextField
              value={text}
              placeholder="192.168.1.20 or pley.example.com"
              type="url"
              width={480}
              initial
              typeNow
              onInput={(v) => edit(v)}
              onSubmit={() => setTimeout(connectNow, 0)}
            />
          </div>
          <div style="margin-top:14px">
            <Pill label={checking ? "Checking…" : "Connect"} onSelect={connectNow} />
          </div>
        </G>
        <div class="address-info" aria-live="polite">
          <div class="t-caption c-muted">Port 7539 is added when none is typed.</div>
          {p.problem && <div class="t-body c-danger">{textOf(p.problem)}</div>}
        </div>
      </div>
    </BrandGlow>
  );
}
