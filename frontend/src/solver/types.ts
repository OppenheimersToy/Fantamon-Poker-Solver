export type Suit = "earth" | "fire" | "water" | "lightning" | "joker";
export type Rank =
  | "ace"
  | "two"
  | "three"
  | "four"
  | "five"
  | "six"
  | "seven"
  | "eight"
  | "nine"
  | "ten"
  | "jack"
  | "queen"
  | "king"
  | "joker";

export type Card = { suit: Suit; rank: Rank };

export type BoardState = { cells: Array<Card | null> }; // length 25
export type DeckState = { cards: Card[] };
export type SkillsState = { keep_both: number; discard_both: number };

/** 1–5: column left→right, row top→bottom (matches solver JSON). */
export type CellCoordHuman = { column: number; row: number };

export type RecommendedMove =
  | { type: "KeepOne"; keep_index: number; column: number; row: number }
  | {
      type: "KeepBoth";
      first_keep_index: number;
      first: CellCoordHuman;
      second: CellCoordHuman;
    }
  | { type: "DiscardBoth" };

export type CalculateResponse = {
  ev: number;
  recommendation: RecommendedMove;
};

/** Live Monte Carlo tick from the Wasm solver (human 1-based cell coords). */
export type SolverProgressPayload = {
  phase: string;
  candidates_total: number;
  candidate_index: number;
  total_rollouts_done: number;
  total_rollouts_planned: number;
  rollout_in_candidate: number;
  rollouts_per_candidate: number;
  running_mean_ev: number;
  best_ev_so_far: number;
  second_best_ev_so_far: number;
  current_candidate: RecommendedMove;
  best_move_so_far: RecommendedMove;
};

