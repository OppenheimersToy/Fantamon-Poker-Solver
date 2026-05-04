import React, { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type {
  BoardState,
  Card,
  CalculateResponse,
  SkillsState,
  SolverProgressPayload,
} from "../solver/types";
import { fullDeck54, removeKnownCards, shuffleInPlace } from "../solver/deck";
import { validPlacements } from "../solver/rules";
import { cardLabel, DEAL_CARD_FORMAT_HINT, formatDealtCardInput, parseCard } from "./cards";

const emptyBoard = (): BoardState => ({ cells: Array.from({ length: 25 }, () => null) });

function idx(x: number, y: number) {
  return y * 5 + x;
}

type WorkerMessage =
  | { id: string; type: "solver_progress"; payload: SolverProgressPayload }
  | { id: string; type: "solver_result"; result: CalculateResponse }
  | { id: string; type: "solver_error"; error: string };

type ProgressPoint = { x: number; run: number; best: number; second: number };

const MAX_PROGRESS_POINTS = 4000;

function SolverProgressChart({ points }: { points: ProgressPoint[] }) {
  const wrapRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [w, setW] = useState(320);

  useLayoutEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      setW(Math.max(200, Math.floor(el.clientWidth)));
    });
    ro.observe(el);
    setW(Math.max(200, Math.floor(el.clientWidth)));
    return () => ro.disconnect();
  }, []);

  useLayoutEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || points.length === 0) return;
    const dpr = Math.min(2, window.devicePixelRatio || 1);
    const h = 140;
    canvas.style.width = `${w}px`;
    canvas.style.height = `${h}px`;
    canvas.width = Math.floor(w * dpr);
    canvas.height = Math.floor(h * dpr);
    const ctxRaw = canvas.getContext("2d");
    if (!ctxRaw) return;
    const ctx = ctxRaw;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const pad = { l: 44, r: 10, t: 12, b: 22 };
    const innerW = w - pad.l - pad.r;
    const innerH = h - pad.t - pad.b;
    const xs = points.map((p) => p.x);
    const xMin = xs[0] ?? 0;
    const xMax = Math.max(xMin + 1, xs[xs.length - 1] ?? 1);
    const vals: number[] = [];
    for (const p of points) {
      if (Number.isFinite(p.run)) vals.push(p.run);
      if (Number.isFinite(p.best)) vals.push(p.best);
      if (Number.isFinite(p.second)) vals.push(p.second);
    }
    let yMin = vals.length ? Math.min(...vals) : 0;
    let yMax = vals.length ? Math.max(...vals) : 1;
    if (!Number.isFinite(yMin) || !Number.isFinite(yMax)) {
      yMin = 0;
      yMax = 1;
    }
    if (yMax - yMin < 1e-3) {
      yMin -= 0.5;
      yMax += 0.5;
    }
    const yPad = (yMax - yMin) * 0.08;
    yMin -= yPad;
    yMax += yPad;
    const sx = (xv: number) => pad.l + ((xv - xMin) / (xMax - xMin)) * innerW;
    const sy = (yv: number) => pad.t + innerH - ((yv - yMin) / (yMax - yMin)) * innerH;
    ctx.fillStyle = "rgba(255,255,255,0.03)";
    ctx.fillRect(pad.l, pad.t, innerW, innerH);
    ctx.strokeStyle = "rgba(231,236,255,0.12)";
    ctx.lineWidth = 1;
    ctx.strokeRect(pad.l, pad.t, innerW, innerH);
    function lineSeries(
      c: CanvasRenderingContext2D,
      key: keyof Pick<ProgressPoint, "run" | "best" | "second">,
      color: string,
      lw: number,
    ) {
      c.beginPath();
      let moved = false;
      for (const p of points) {
        const yv = p[key];
        if (!Number.isFinite(yv)) continue;
        const x = sx(p.x);
        const y = sy(yv);
        if (!moved) {
          c.moveTo(x, y);
          moved = true;
        } else c.lineTo(x, y);
      }
      c.strokeStyle = color;
      c.lineWidth = lw;
      c.stroke();
    }
    lineSeries(ctx, "second", "rgba(169,180,230,0.4)", 1);
    lineSeries(ctx, "best", "var(--good)", 1.75);
    lineSeries(ctx, "run", "var(--accent)", 1.35);
    ctx.fillStyle = "var(--muted)";
    ctx.font = "11px ui-sans-serif,system-ui,sans-serif";
    ctx.fillText(yMax.toFixed(1), 4, pad.t + 10);
    ctx.fillText(yMin.toFixed(1), 4, pad.t + innerH);
    ctx.fillText("rollouts →", pad.l, h - 6);
  }, [points, w]);

  return (
    <div ref={wrapRef} style={{ width: "100%", marginTop: 10 }}>
      <canvas ref={canvasRef} style={{ display: "block", width: "100%" }} />
    </div>
  );
}

type PlacementGhostMode = "off" | "sequential" | "fixed";

/** After first Keep Both placement, tracks that the next pending card finishes the skill flow. */
type PendingFlow = { kind: "keepBothReco" };

export function App() {
  const [board, setBoard] = useState<BoardState>(() => emptyBoard());
  const [draw1, setDraw1] = useState("10F");
  const [draw2, setDraw2] = useState("9L");
  const [skills, setSkills] = useState<SkillsState>({ keep_both: 2, discard_both: 2 });
  const [depth, setDepth] = useState(400);

  const [busy, setBusy] = useState(false);
  const [calcDots, setCalcDots] = useState(1);
  const [calcNotice, setCalcNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<CalculateResponse | null>(null);
  const [progressSeries, setProgressSeries] = useState<ProgressPoint[]>([]);
  const [lastProgress, setLastProgress] = useState<SolverProgressPayload | null>(null);
  const [pendingPlacements, setPendingPlacements] = useState<Card[]>([]);
  const [pendingHint, setPendingHint] = useState<string | null>(null);
  const [placementGhostMode, setPlacementGhostMode] = useState<PlacementGhostMode>("off");
  const [placementFixedGhosts, setPlacementFixedGhosts] = useState<
    Array<{ x: number; y: number; order: "1st" | "2nd"; text: string }>
  >([]);
  const [editing, setEditing] = useState<{ x: number; y: number } | null>(null);
  const [cellDraft, setCellDraft] = useState("");
  /** Cards known not in the draw pile (discarded, burned, etc.). Solver deck = full 54 − board − dealt − this. */
  const [excludedFromDraw, setExcludedFromDraw] = useState<Card[]>([]);
  const [excludeDraft, setExcludeDraft] = useState("");

  const workerRef = useRef<Worker | null>(null);
  const reqIdRef = useRef(0);
  /** Latest solver progress (mirrors state) so Cancel always reads the most recent tick. */
  const lastProgressRef = useRef<SolverProgressPayload | null>(null);
  const cellInputRef = useRef<HTMLInputElement | null>(null);
  const skipBlurCommitRef = useRef(false);
  const pendingFlowRef = useRef<PendingFlow | null>(null);

  const drawn: [Card, Card] | null = useMemo(() => {
    const c1 = parseCard(draw1);
    const c2 = parseCard(draw2);
    if (!c1 || !c2) return null;
    return [c1, c2];
  }, [draw1, draw2]);

  const valid = useMemo(() => validPlacements(board), [board]);
  const validSet = useMemo(() => new Set(valid.map(([x, y]) => `${x},${y}`)), [valid]);

  const recoCells = useMemo(() => {
    const s = new Set<string>();
    const r = result?.recommendation;
    if (!r) return s;
    // JSON uses 1-based column/row; grid keys are 0-based.
    if (r.type === "KeepOne") s.add(`${r.column - 1},${r.row - 1}`);
    if (r.type === "KeepBoth") {
      s.add(`${r.first.column - 1},${r.first.row - 1}`);
      s.add(`${r.second.column - 1},${r.second.row - 1}`);
    }
    return s;
  }, [result]);

  function ensureWorker() {
    if (!workerRef.current) {
      workerRef.current = new Worker(new URL("../solver/solverWorker.ts", import.meta.url), {
        type: "module",
      });
      workerRef.current.onmessage = (ev: MessageEvent<WorkerMessage>) => {
        const msg = ev.data;
        if (msg.id !== String(reqIdRef.current)) return;
        if (msg.type === "solver_progress") {
          const p = msg.payload;
          lastProgressRef.current = p;
          setLastProgress(p);
          setProgressSeries((prev) => {
            const next: ProgressPoint[] = [
              ...prev,
              {
                x: p.total_rollouts_done,
                run: p.running_mean_ev,
                best: p.best_ev_so_far,
                second: p.second_best_ev_so_far,
              },
            ];
            return next.length > MAX_PROGRESS_POINTS ? next.slice(-MAX_PROGRESS_POINTS) : next;
          });
          return;
        }
        setBusy(false);
        if (msg.type === "solver_error") {
          setError(msg.error);
          return;
        }
        setError(null);
        setCalcNotice(null);
        setResult(msg.result);
      };
      workerRef.current.onerror = () => {
        setBusy(false);
        setError("Solver worker failed. Try again.");
      };
    }
    return workerRef.current;
  }

  function cancelCalculation() {
    const snapshot = lastProgressRef.current;
    const ev = snapshot?.best_ev_so_far;
    const canApplyBest =
      snapshot !== null &&
      typeof ev === "number" &&
      Number.isFinite(ev);

    if (workerRef.current) {
      workerRef.current.terminate();
      workerRef.current = null;
    }
    reqIdRef.current += 1;
    setBusy(false);

    if (canApplyBest) {
      setResult({
        ev,
        recommendation: snapshot.best_move_so_far,
      });
      setError(null);
      setCalcNotice("Stopped early — using best move found so far.");
      lastProgressRef.current = null;
      setLastProgress(null);
      return;
    }

    setProgressSeries([]);
    lastProgressRef.current = null;
    setLastProgress(null);
    setCalcNotice("Calculation cancelled.");
  }

  useEffect(() => {
    if (pendingPlacements.length === 0) {
      setPlacementGhostMode("off");
      setPlacementFixedGhosts([]);
    }
  }, [pendingPlacements.length]);

  useEffect(() => {
    if (!busy) {
      setCalcDots(1);
      return;
    }
    let d = 1;
    setCalcDots(1);
    const t = window.setInterval(() => {
      d = d >= 5 ? 1 : d + 1;
      setCalcDots(d);
    }, 400);
    return () => window.clearInterval(t);
  }, [busy]);

  function runSolverWithBoard(b: BoardState) {
    setCalcNotice(null);
    setError(null);
    setResult(null);
    setProgressSeries([]);
    lastProgressRef.current = null;
    setLastProgress(null);
    if (!drawn) {
      setError(`Please enter two valid dealt cards. ${DEAL_CARD_FORMAT_HINT}`);
      return;
    }

    const known: Card[] = [
      ...b.cells.filter((c): c is Card => Boolean(c)),
      drawn[0],
      drawn[1],
      ...excludedFromDraw,
    ];
    const d = removeKnownCards(fullDeck54(), known);
    shuffleInPlace(d);

    const worker = ensureWorker();
    reqIdRef.current += 1;
    const id = String(reqIdRef.current);
    setBusy(true);
    worker.postMessage({
      id,
      board: b,
      deck: { cards: d },
      drawn,
      skills,
      simulationDepth: depth,
    });
  }

  function runSolver() {
    runSolverWithBoard(board);
  }

  function placeCardAt(x: number, y: number, c: Card) {
    setResult(null);
    setBoard((b) => {
      const next: BoardState = { cells: b.cells.slice() };
      next.cells[idx(x, y)] = c;
      return next;
    });
  }

  function openCellEditor(x: number, y: number) {
    setError(null);
    const c = board.cells[idx(x, y)];
    if (c && (c.rank === "joker" || c.suit === "joker")) setCellDraft("Joker");
    else if (c) setCellDraft(cardLabel(c));
    else setCellDraft("");
    setEditing({ x, y });
  }

  function commitCellEditor() {
    if (!editing) return;
    const { x, y } = editing;
    const raw = cellDraft.trim();
    const nextCells = board.cells.slice();

    if (!raw) {
      nextCells[idx(x, y)] = null;
    } else {
      const p = parseCard(raw);
      if (!p) {
        setError(`Invalid card "${raw}". ${DEAL_CARD_FORMAT_HINT}`);
        setEditing(null);
        return;
      }
      nextCells[idx(x, y)] = p;
    }

    const nextBoard: BoardState = { cells: nextCells };
    setResult(null);
    setBoard(nextBoard);
    setEditing(null);
    setPendingPlacements([]);
    setPendingHint(null);
    setPlacementGhostMode("off");
    setPlacementFixedGhosts([]);
    pendingFlowRef.current = null;
  }

  useLayoutEffect(() => {
    if (!editing) return;
    cellInputRef.current?.focus();
    cellInputRef.current?.select();
  }, [editing]);

  function recordDiscardBothForCurrentDeal() {
    if (!drawn) {
      setError(`Enter two dealt cards first. ${DEAL_CARD_FORMAT_HINT}`);
      return;
    }
    if (skills.discard_both <= 0) {
      setError("No Discard Both uses remaining.");
      return;
    }
    setError(null);
    setSkills((s) => ({ ...s, discard_both: Math.max(0, s.discard_both - 1) }));
    setExcludedFromDraw((ex) => [...ex, drawn[0], drawn[1]]);
    setDraw1("");
    setDraw2("");
    setResult(null);
  }

  function addExcludedFromInput() {
    const p = parseCard(excludeDraft);
    if (!p) {
      setError(`Invalid card to exclude. ${DEAL_CARD_FORMAT_HINT}`);
      return;
    }
    setError(null);
    setExcludedFromDraw((ex) => [...ex, p]);
    setExcludeDraft(formatDealtCardInput(excludeDraft));
  }

  function removeExcludedAt(index: number) {
    setExcludedFromDraw((ex) => ex.filter((_, i) => i !== index));
  }

  const primarySuggestedCardLabel = useMemo(() => {
    if (!result?.recommendation || !drawn) return null;
    const r = result.recommendation;
    if (r.type === "KeepOne") return cardLabel(drawn[r.keep_index]);
    if (r.type === "KeepBoth") return cardLabel(drawn[r.first_keep_index]);
    return null;
  }, [result, drawn]);

  return (
    <div className="container">
      <div className="panel">
        <div className="title">
          <h1>Fantamon Optimal Move Calculator</h1>
          <div className="muted">Wasm solver via Rust • Worker-threaded</div>
        </div>

        <div className="grid" role="grid" aria-label="5x5 board">
          {Array.from({ length: 25 }).map((_, i) => {
            const x = i % 5;
            const y = Math.floor(i / 5);
            const c = board.cells[i];
            const key = `${x},${y}`;
            const isValid = validSet.has(key);
            const isReco = recoCells.has(key);
            const isEditing = editing?.x === x && editing?.y === y;
            let ghostDraft: { order?: "1st" | "2nd"; text: string } | null = null;
            if (!isEditing && !c && pendingPlacements.length > 0) {
              if (placementGhostMode === "sequential" && isValid) {
                ghostDraft = { text: cardLabel(pendingPlacements[0]) };
              } else if (placementGhostMode === "fixed") {
                const g = placementFixedGhosts.find((h) => h.x === x && h.y === y);
                if (g) ghostDraft = { order: g.order, text: g.text };
              }
            } else if (
              !isEditing &&
              !c &&
              pendingPlacements.length === 0 &&
              drawn &&
              result?.recommendation?.type === "KeepOne" &&
              isValid
            ) {
              const r = result.recommendation;
              ghostDraft = { text: cardLabel(drawn[r.keep_index]) };
            } else if (
              !isEditing &&
              !c &&
              pendingPlacements.length === 0 &&
              drawn &&
              result?.recommendation?.type === "KeepBoth" &&
              isValid
            ) {
              const r = result.recommendation;
              const c1 = drawn[r.first_keep_index];
              const c2 = drawn[1 - r.first_keep_index];
              if (x === r.first.column - 1 && y === r.first.row - 1) {
                ghostDraft = { order: "1st", text: cardLabel(c1) };
              } else if (x === r.second.column - 1 && y === r.second.row - 1) {
                ghostDraft = { order: "2nd", text: cardLabel(c2) };
              }
            }
            const cls = ["cell", c ? "filled" : "", !c && isValid ? "valid" : "", isReco ? "reco" : ""]
              .filter(Boolean)
              .join(" ");
            return (
              <div
                key={i}
                className={cls}
                title={
                  pendingPlacements.length > 0
                    ? "Click to place the next card from the queue"
                    : c
                      ? "Click to edit card"
                      : isValid && drawn && result?.recommendation?.type === "KeepOne"
                        ? "Click to place kept card (other dealt → discard). Option-click to type a card instead."
                        : isValid && drawn && result?.recommendation?.type === "KeepBoth"
                          ? "Click a legal cell for the first card, then place the second. Option-click to type a card instead."
                          : isValid
                            ? "Click to type a card (Option-click if a suggestion highlights cells)"
                            : "Not a legal empty cell yet"
                }
                onClick={(e) => {
                  if (e.altKey) {
                    if (c) {
                      openCellEditor(x, y);
                      return;
                    }
                    if (isValid) {
                      openCellEditor(x, y);
                      return;
                    }
                  }
                  if (pendingPlacements.length > 0) {
                    if (c || !isValid) return;
                    const nextCard = pendingPlacements[0];
                    placeCardAt(x, y, nextCard);
                    const rest = pendingPlacements.slice(1);
                    setPendingPlacements(rest);
                    if (rest.length === 0) {
                      setPendingHint(null);
                      const flow = pendingFlowRef.current;
                      pendingFlowRef.current = null;
                      if (flow?.kind === "keepBothReco") {
                        setDraw1("");
                        setDraw2("");
                        setResult(null);
                      }
                    }
                    return;
                  }
                  if (!c && isValid && drawn && result?.recommendation?.type === "KeepBoth") {
                    const r = result.recommendation;
                    const cFirst = drawn[r.first_keep_index];
                    placeCardAt(x, y, cFirst);
                    const second = drawn[1 - r.first_keep_index];
                    setPendingPlacements([second]);
                    pendingFlowRef.current = { kind: "keepBothReco" };
                    setPlacementGhostMode("sequential");
                    setPlacementFixedGhosts([]);
                    setPendingHint(`Place second card: ${cardLabel(second)}.`);
                    return;
                  }
                  if (!c && isValid && drawn && result?.recommendation?.type === "KeepOne") {
                    const r = result.recommendation;
                    const ki = r.keep_index === 0 || r.keep_index === 1 ? r.keep_index : 0;
                    const keep = drawn[ki];
                    const discard = drawn[1 - ki];
                    placeCardAt(x, y, keep);
                    setExcludedFromDraw((ex) => [...ex, discard]);
                    setDraw1("");
                    setDraw2("");
                    setResult(null);
                    return;
                  }
                  if (c) {
                    openCellEditor(x, y);
                    return;
                  }
                  if (!isValid) return;
                  openCellEditor(x, y);
                }}
              >
                {isEditing ? (
                  <input
                    ref={cellInputRef}
                    className="cell-input"
                    value={cellDraft}
                    onChange={(e) => setCellDraft(e.target.value)}
                    onClick={(e) => e.stopPropagation()}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        e.preventDefault();
                        commitCellEditor();
                      }
                      if (e.key === "Escape") {
                        e.preventDefault();
                        skipBlurCommitRef.current = true;
                        setEditing(null);
                      }
                    }}
                    onBlur={() => {
                      if (skipBlurCommitRef.current) {
                        skipBlurCommitRef.current = false;
                        return;
                      }
                      commitCellEditor();
                    }}
                    aria-label={`Card at column ${x + 1} row ${y + 1}`}
                    placeholder="10F"
                  />
                ) : (
                  <div className="cell-body">
                    {c ? <span>{cardLabel(c)}</span> : null}
                    {ghostDraft ? (
                      <span className="cell-ghost" aria-hidden>
                        {ghostDraft.order ? (
                          <span className="cell-ghost-order">{ghostDraft.order}</span>
                        ) : null}
                        <span>{ghostDraft.text}</span>
                      </span>
                    ) : null}
                  </div>
                )}
              </div>
            );
          })}
        </div>

        <div style={{ marginTop: 12 }} className="muted">
          {pendingHint
            ? pendingHint
            : "Each turn: enter two dealt cards → Calculate → click the board to place (Keep Both: first click places one card, second click the other). Option-click a cell to type a card instead. Mid-game board edits: type in cells anytime."}
        </div>

        <div style={{ marginTop: 14 }}>
          <div style={{ fontWeight: 700, fontSize: 13, marginBottom: 4 }}>Live solve progress</div>
          <div className="muted" style={{ fontSize: 12, lineHeight: 1.45 }}>
            {busy && lastProgress ? (
              <>
                {lastProgress.phase === "done" ? "Finishing…" : "Running Monte Carlo…"} — candidate{" "}
                <span style={{ fontVariantNumeric: "tabular-nums" }}>
                  {lastProgress.candidate_index + 1}/{lastProgress.candidates_total}
                </span>
                , rollout{" "}
                <span style={{ fontVariantNumeric: "tabular-nums" }}>
                  {lastProgress.rollout_in_candidate}/{lastProgress.rollouts_per_candidate}
                </span>
                , mean this move{" "}
                <span style={{ fontVariantNumeric: "tabular-nums" }}>
                  {lastProgress.running_mean_ev.toFixed(2)}
                </span>
                , best so far{" "}
                <span style={{ fontVariantNumeric: "tabular-nums" }}>
                  {Number.isFinite(lastProgress.best_ev_so_far)
                    ? lastProgress.best_ev_so_far.toFixed(2)
                    : "—"}
                </span>
              </>
            ) : busy ? (
              "Starting worker…"
            ) : progressSeries.length > 0 ? (
              "Last run (chart below). Calculate again to refresh."
            ) : (
              "EV vs rollout count updates while Calculate runs (purple = mean score for the move being simulated, green = best finished candidate so far, gray = second best)."
            )}
          </div>
          {progressSeries.length > 0 ? <SolverProgressChart points={progressSeries} /> : null}
        </div>
      </div>

      <div className="panel row">
        <div className="kv">
          <div>
            <div style={{ fontWeight: 700 }}>Dealt Cards</div>
            <div className="muted">Format: {DEAL_CARD_FORMAT_HINT}</div>
          </div>
          <div className="muted">{primarySuggestedCardLabel ? `Suggested: ${primarySuggestedCardLabel}` : ""}</div>
        </div>
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 10 }}>
          <input
            className="input"
            value={draw1}
            onChange={(e) => setDraw1(e.target.value)}
            onBlur={() => setDraw1((v) => formatDealtCardInput(v))}
          />
          <input
            className="input"
            value={draw2}
            onChange={(e) => setDraw2(e.target.value)}
            onBlur={() => setDraw2((v) => formatDealtCardInput(v))}
          />
        </div>

        <div className="kv">
          <div>
            <div style={{ fontWeight: 700 }}>Not in draw pile</div>
            <div className="muted">
              Board + current deal + this list are removed before simulating. After Keep One placement, the
              unplayed dealt card is added here automatically; Discard Both adds both dealt cards here.
              Use the list for past discards (update dealt inputs after each turn so cards are not
              double-counted).
            </div>
          </div>
        </div>
        <button
          type="button"
          className="btn"
          onClick={recordDiscardBothForCurrentDeal}
          disabled={busy || !drawn || skills.discard_both <= 0}
        >
          Discard both (this deal) — use skill; both dealt cards go to this list and dealt fields clear
        </button>
        <div style={{ display: "grid", gridTemplateColumns: "1fr auto", gap: 8, alignItems: "stretch" }}>
          <input
            className="input"
            value={excludeDraft}
            onChange={(e) => setExcludeDraft(e.target.value)}
            onBlur={() => setExcludeDraft((v) => (parseCard(v) ? formatDealtCardInput(v) : v))}
            placeholder="Add discarded card"
            disabled={busy}
          />
          <button type="button" className="btn" onClick={addExcludedFromInput} disabled={busy}>
            Add
          </button>
        </div>
        {excludedFromDraw.length > 0 ? (
          <div
            className="muted"
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fill, minmax(148px, 1fr))",
              gap: "8px 10px",
              margin: 0,
              fontSize: 13,
              lineHeight: 1.45,
            }}
          >
            {excludedFromDraw.map((card, i) => (
              <div
                key={`excluded-${i}`}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  minWidth: 0,
                }}
              >
                <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                  {cardLabel(card)}
                </span>
                <button
                  type="button"
                  className="btn"
                  style={{ padding: "2px 8px", fontSize: 12, flexShrink: 0 }}
                  onClick={() => removeExcludedAt(i)}
                  disabled={busy}
                >
                  Remove
                </button>
              </div>
            ))}
          </div>
        ) : null}

        <div className="kv">
          <div>
            <div style={{ fontWeight: 700 }}>Skills Remaining</div>
            <div className="muted">Each starts at 2 uses</div>
          </div>
          <button
            className="btn"
            onClick={() => setSkills({ keep_both: 2, discard_both: 2 })}
            disabled={busy}
          >
            Reset
          </button>
        </div>
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 10 }}>
          <label className="muted">
            Keep Both
            <input
              className="input"
              type="number"
              min={0}
              max={2}
              value={skills.keep_both}
              onChange={(e) => setSkills((s) => ({ ...s, keep_both: Number(e.target.value) }))}
              disabled={busy}
            />
          </label>
          <label className="muted">
            Discard Both
            <input
              className="input"
              type="number"
              min={0}
              max={2}
              value={skills.discard_both}
              onChange={(e) => setSkills((s) => ({ ...s, discard_both: Number(e.target.value) }))}
              disabled={busy}
            />
          </label>
        </div>

        <div className="kv">
          <div>
            <div style={{ fontWeight: 700 }}>Simulation Depth</div>
            <div className="muted">Rollouts per candidate (higher = slower, better EV)</div>
          </div>
          <div style={{ fontVariantNumeric: "tabular-nums" }}>{depth}</div>
        </div>
        <input
          type="range"
          min={50}
          max={2000}
          step={50}
          value={depth}
          onChange={(e) => setDepth(Number(e.target.value))}
          disabled={busy}
        />

        <div style={{ display: "grid", gap: 10 }}>
          <div
            style={{
              display: "grid",
              gridTemplateColumns: busy ? "1fr 1fr" : "1fr",
              gap: 10,
            }}
          >
            <button className="btn primary" onClick={runSolver} disabled={busy}>
              {busy ? `Calculating${".".repeat(calcDots)}` : "Calculate Optimal Move"}
            </button>
            {busy ? (
              <button type="button" className="btn" onClick={cancelCalculation}>
                Cancel
              </button>
            ) : null}
          </div>
          <button
            className="btn"
            onClick={() => {
              setBoard(emptyBoard());
              setResult(null);
              setError(null);
              setPendingPlacements([]);
              setPendingHint(null);
              setEditing(null);
              setPlacementGhostMode("off");
              setPlacementFixedGhosts([]);
              pendingFlowRef.current = null;
              setExcludedFromDraw([]);
            }}
            disabled={busy}
          >
            Clear Board
          </button>
          {pendingPlacements.length > 0 ? (
            <button
              className="btn"
              onClick={() => {
                setPendingPlacements([]);
                setPendingHint(null);
                pendingFlowRef.current = null;
              }}
              disabled={busy}
            >
              Cancel Pending Placement
            </button>
          ) : null}
        </div>

        {calcNotice ? <div className="muted">{calcNotice}</div> : null}

        {error ? <div style={{ color: "var(--bad)", fontWeight: 650 }}>{error}</div> : null}

        {result ? (
          <div className="row">
            <div className="kv">
              <div style={{ fontWeight: 800 }}>Recommended Move</div>
              <div style={{ fontVariantNumeric: "tabular-nums" }}>EV: {result.ev.toFixed(2)}</div>
            </div>
            <div className="muted" style={{ marginBottom: 8 }}>
              EV estimates expected total points on all completed rows and columns when the grid is full or
              no more full turns can be played; leftover deck cards are fine.
            </div>
            <div className="muted" style={{ wordBreak: "break-word" }}>
              {JSON.stringify(result.recommendation)}
            </div>
          </div>
        ) : (
          <div className="muted">Tip: run `npm run wasm` once inside `frontend/` before `npm run dev`.</div>
        )}
      </div>
    </div>
  );
}

