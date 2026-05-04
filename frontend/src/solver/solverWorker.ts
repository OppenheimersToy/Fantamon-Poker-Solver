import type {
  BoardState,
  DeckState,
  Card,
  SkillsState,
  CalculateResponse,
  SolverProgressPayload,
} from "./types";

type WorkRequest = {
  id: string;
  board: BoardState;
  deck: DeckState;
  drawn: [Card, Card];
  skills: SkillsState;
  simulationDepth: number;
};

type WorkResponse =
  | { id: string; type: "solver_progress"; payload: SolverProgressPayload }
  | { id: string; type: "solver_result"; result: CalculateResponse }
  | { id: string; type: "solver_error"; error: string };

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

self.onmessage = async (ev: MessageEvent<WorkRequest>) => {
  const { id, board, deck, drawn, skills, simulationDepth } = ev.data;
  try {
    const wasm = await getWasm();
    let jsVal: unknown;
    if (typeof wasm.calculate_optimal_move_with_progress === "function") {
      jsVal = wasm.calculate_optimal_move_with_progress(
        JSON.stringify(board),
        JSON.stringify(deck),
        JSON.stringify(drawn),
        JSON.stringify(skills),
        simulationDepth,
        (p: SolverProgressPayload) => {
          const msg: WorkResponse = { id, type: "solver_progress", payload: p };
          self.postMessage(msg);
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
