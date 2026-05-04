import type {
  BoardState,
  DeckState,
  Card,
  SkillsState,
  CalculateResponse,
  SolverProgressPayload,
  RecommendedMove,
  AveragedSolverResult,
} from "./types";
import { fullDeck54, removeKnownCards, shuffleInPlace } from "./deck";

type WorkRequestSingle = {
  id: string;
  mode?: "single";
  board: BoardState;
  deck: DeckState;
  drawn: [Card, Card];
  skills: SkillsState;
  simulationDepth: number;
};

type WorkRequestAveraged = {
  id: string;
  mode: "averaged";
  board: BoardState;
  drawn: [Card, Card];
  skills: SkillsState;
  simulationDepth: number;
  excludedFromDraw: Card[];
  sampleCount?: number;
};

type WorkRequest = WorkRequestSingle | WorkRequestAveraged;

type WorkResponse =
  | { id: string; type: "solver_progress"; payload: SolverProgressPayload }
  | { id: string; type: "solver_result"; result: CalculateResponse }
  | { id: string; type: "solver_error"; error: string }
  | { id: string; type: "solver_averaged_progress"; current: number; total: number }
  | { id: string; type: "solver_averaged_result"; result: AveragedSolverResult };

type WasmModule = {
  calculate_optimal_move: (
    boardJson: string,
    deckJson: string,
    drawnJson: string,
    skillsJson: string,
    simulationDepth: number,
  ) => unknown;
  calculate_optimal_move_with_progress?: (
    boardJson: string,
    deckJson: string,
    drawnJson: string,
    skillsJson: string,
    simulationDepth: number,
    progress: (p: SolverProgressPayload) => void,
  ) => unknown;
};

let wasmReady: Promise<WasmModule> | null = null;

async function getWasm() {
  if (!wasmReady) {
    wasmReady = (async () => {
      const mod = await import("../wasm-pkg/fantamon_solver.js");
      await mod.default();
      return mod as WasmModule;
    })();
  }
  return wasmReady;
}

type BatchAgg = {
  sampleCount: number;
  winnerVotes: number;
  secondVotes: number;
  consensus: RecommendedMove;
  meanEv: number;
};

/** One batch of `sampleCount` independent shuffles + solver runs. */
function runOneBatch(
  wasm: WasmModule,
  id: string,
  known: Card[],
  boardJson: string,
  drawnJson: string,
  skillsJson: string,
  simulationDepth: number,
  sampleCount: number,
  progressOffset: number,
  progressTotal: number,
): BatchAgg {
  const byKey = new Map<string, { count: number; recommendation: RecommendedMove }>();
  let sumEv = 0;

  for (let i = 0; i < sampleCount; i++) {
    const d = removeKnownCards(fullDeck54(), known);
    shuffleInPlace(d);
    const deckJson = JSON.stringify({ cards: d });
    const jsVal = wasm.calculate_optimal_move(
      boardJson,
      deckJson,
      drawnJson,
      skillsJson,
      simulationDepth,
    ) as CalculateResponse;
    sumEv += jsVal.ev;
    const r = jsVal.recommendation;
    const key = JSON.stringify(r);
    const prev = byKey.get(key);
    if (prev) prev.count += 1;
    else byKey.set(key, { count: 1, recommendation: r });
    const prog: WorkResponse = {
      id,
      type: "solver_averaged_progress",
      current: progressOffset + i + 1,
      total: progressTotal,
    };
    self.postMessage(prog);
  }

  let bestKey = "";
  let winnerVotes = 0;
  for (const [key, v] of byKey) {
    if (v.count > winnerVotes) {
      winnerVotes = v.count;
      bestKey = key;
    }
  }
  let secondVotes = 0;
  for (const [key, v] of byKey) {
    if (key === bestKey) continue;
    if (v.count > secondVotes) secondVotes = v.count;
  }
  const consensus = byKey.get(bestKey)?.recommendation ?? ({ type: "DiscardBoth" } as RecommendedMove);

  return {
    sampleCount,
    winnerVotes,
    secondVotes,
    consensus,
    meanEv: sumEv / sampleCount,
  };
}

/**
 * Up to 3 batches of fresh shuffles. Stop early when consensus vote count meets:
 * 1) strictly &gt; 4 (i.e. ≥ 5), else 2) ≥ 3, else 3) ≥ 2. If none meet, return last batch with `trustTier` 0.
 */
function runAveraged(wasm: WasmModule, data: WorkRequestAveraged, id: string): AveragedSolverResult {
  const sampleCount = data.sampleCount ?? 10;
  const CASCADE = [
    { tier: 1 as const, passes: (w: number) => w > 4, thresholdLabel: 5 },
    { tier: 2 as const, passes: (w: number) => w >= 3, thresholdLabel: 3 },
    { tier: 3 as const, passes: (w: number) => w >= 2, thresholdLabel: 2 },
  ];
  const totalProgress = sampleCount * CASCADE.length;

  const known: Card[] = [
    ...data.board.cells.filter((c): c is Card => Boolean(c)),
    data.drawn[0],
    data.drawn[1],
    ...data.excludedFromDraw,
  ];
  const boardJson = JSON.stringify(data.board);
  const drawnJson = JSON.stringify(data.drawn);
  const skillsJson = JSON.stringify(data.skills);

  let last: BatchAgg | null = null;
  let trustTier: 0 | 1 | 2 | 3 = 0;
  let trustThresholdMet = 0;

  for (let r = 0; r < CASCADE.length; r++) {
    const offset = r * sampleCount;
    last = runOneBatch(
      wasm,
      id,
      known,
      boardJson,
      drawnJson,
      skillsJson,
      data.simulationDepth,
      sampleCount,
      offset,
      totalProgress,
    );
    const rule = CASCADE[r];
    if (rule.passes(last.winnerVotes)) {
      trustTier = rule.tier;
      trustThresholdMet = rule.thresholdLabel;
      break;
    }
  }

  const b = last!;
  const trusted = trustTier > 0;

  return {
    sampleCount,
    trustThresholdMet,
    trustTier,
    cascadeRoundsUsed: trustTier > 0 ? trustTier : CASCADE.length,
    meanEv: b.meanEv,
    consensus: b.consensus,
    trusted,
    winnerVotes: b.winnerVotes,
    secondVotes: b.secondVotes,
  };
}

self.onmessage = async (ev: MessageEvent<WorkRequest>) => {
  const data = ev.data;
  const { id } = data;
  try {
    const wasm = await getWasm();
    if (data.mode === "averaged") {
      const result = runAveraged(wasm, data, id);
      const msg: WorkResponse = { id, type: "solver_averaged_result", result };
      self.postMessage(msg);
      return;
    }

    const { board, deck, drawn, skills, simulationDepth } = data as WorkRequestSingle;
    let jsVal: unknown;
    if (typeof wasm.calculate_optimal_move_with_progress === "function") {
      jsVal = wasm.calculate_optimal_move_with_progress(
        JSON.stringify(board),
        JSON.stringify(deck),
        JSON.stringify(drawn),
        JSON.stringify(skills),
        simulationDepth,
        (p: SolverProgressPayload) => {
          const m: WorkResponse = { id, type: "solver_progress", payload: p };
          self.postMessage(m);
        },
      );
    } else {
      jsVal = wasm.calculate_optimal_move(
        JSON.stringify(board),
        JSON.stringify(deck),
        JSON.stringify(drawn),
        JSON.stringify(skills),
        simulationDepth,
      );
    }
    const result = jsVal as CalculateResponse;
    const msg: WorkResponse = { id, type: "solver_result", result };
    self.postMessage(msg);
  } catch (e: unknown) {
    const msg: WorkResponse = {
      id,
      type: "solver_error",
      error: String((e as { message?: string })?.message ?? e),
    };
    self.postMessage(msg);
  }
};

export {};
