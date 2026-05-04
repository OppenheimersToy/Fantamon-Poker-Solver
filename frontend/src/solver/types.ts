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

/** Result of multiple shuffled-deck samples (Monte Carlo over unknown pile order). */
export type AveragedSolverResult = {
  /** Samples per cascade attempt (fresh shuffles each attempt). */
  sampleCount: number;
  /**
   * Agreement bar that was satisfied: `5` = first pass (strictly &gt;4 agrees), `3` = second pass,
   * `2` = third pass, `0` = none (still show plurality consensus).
   */
  trustThresholdMet: number;
  /** 1 = met &gt;4 (≥5), 2 = met ≥3 on 2nd batch, 3 = met ≥2 on 3rd batch, 0 = no bar met. */
  trustTier: 0 | 1 | 2 | 3;
  /** How many full batches ran (1–3). */
  cascadeRoundsUsed: number;
  /** Mean EV across the batch used for this result (one batch = `sampleCount` shuffles). */
  meanEv: number;
  /** Plurality winner (highest vote count among exact recommendations) for that batch. */
  consensus: RecommendedMove;
  /** True iff `trustTier` &gt; 0. */
  trusted: boolean;
  winnerVotes: number;
  secondVotes: number;
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

