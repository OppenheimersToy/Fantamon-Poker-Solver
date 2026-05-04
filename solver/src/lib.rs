mod eval;
mod game;
mod solver;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use js_sys::Function;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalculateRequest {
    pub current_board: game::BoardState,
    pub current_deck: game::DeckState,
    pub drawn_cards: [game::Card; 2],
    pub remaining_skills: game::SkillsState,
    /// Number of UCT **iterations** (total MCTS simulations from the root).
    pub simulation_depth: u32,
    /// How many **decision plies** to keep in the UCT tree (1 = no lookahead beyond playout;
    /// typical 3–5 for a balance of quality and speed). Capped in the solver.
    #[serde(default = "default_uct_max_depth")]
    pub uct_max_depth: u8,
}

fn default_uct_max_depth() -> u8 {
    solver::UCT_MAX_TREE_DEPTH
}

/// One-based cell coordinates: column 1–5 left→right, row 1–5 top→bottom (human-facing JSON).
#[derive(Debug, Clone, Serialize)]
pub struct CellCoordHuman {
    pub column: u8,
    pub row: u8,
}

/// Serialized recommendation for the UI (1-based columns/rows).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum RecommendedMoveHuman {
    KeepOne {
        keep_index: u8,
        column: u8,
        row: u8,
    },
    KeepBoth {
        first_keep_index: u8,
        first: CellCoordHuman,
        second: CellCoordHuman,
    },
    DiscardBoth,
}

impl From<solver::RecommendedMove> for RecommendedMoveHuman {
    fn from(m: solver::RecommendedMove) -> Self {
        match m {
            solver::RecommendedMove::KeepOne { keep_index, x, y } => RecommendedMoveHuman::KeepOne {
                keep_index,
                column: (x + 1) as u8,
                row: (y + 1) as u8,
            },
            solver::RecommendedMove::KeepBoth {
                first_keep_index,
                first,
                second,
            } => RecommendedMoveHuman::KeepBoth {
                first_keep_index,
                first: CellCoordHuman {
                    column: (first.0 + 1) as u8,
                    row: (first.1 + 1) as u8,
                },
                second: CellCoordHuman {
                    column: (second.0 + 1) as u8,
                    row: (second.1 + 1) as u8,
                },
            },
            solver::RecommendedMove::DiscardBoth => RecommendedMoveHuman::DiscardBoth,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CalculateResponse {
    pub ev: f64,
    pub recommendation: RecommendedMoveHuman,
}

/// Progress tick for live Monte Carlo charts (mirrors `solver::SolverProgress` with human coords).
#[derive(Debug, Clone, Serialize)]
pub struct SolverProgressHuman {
    pub phase: String,
    pub candidates_total: u32,
    pub candidate_index: u32,
    pub total_rollouts_done: u64,
    pub total_rollouts_planned: u64,
    pub rollout_in_candidate: u32,
    pub rollouts_per_candidate: u32,
    pub running_mean_ev: f64,
    pub best_ev_so_far: f64,
    pub second_best_ev_so_far: f64,
    pub current_candidate: RecommendedMoveHuman,
    pub best_move_so_far: RecommendedMoveHuman,
}

fn solver_progress_to_human(p: solver::SolverProgress) -> SolverProgressHuman {
    SolverProgressHuman {
        phase: p.phase.to_string(),
        candidates_total: p.candidates_total,
        candidate_index: p.candidate_index,
        total_rollouts_done: p.total_rollouts_done,
        total_rollouts_planned: p.total_rollouts_planned,
        rollout_in_candidate: p.rollout_in_candidate,
        rollouts_per_candidate: p.rollouts_per_candidate,
        running_mean_ev: p.running_mean_ev,
        best_ev_so_far: p.best_ev_so_far,
        second_best_ev_so_far: p.second_best_ev_so_far,
        current_candidate: p.current_candidate.into(),
        best_move_so_far: p.best_move_so_far.into(),
    }
}

#[wasm_bindgen]
pub fn calculate_optimal_move(
    current_board_json: &str,
    current_deck_json: &str,
    drawn_cards_json: &str,
    remaining_skills_json: &str,
    simulation_depth: u32,
) -> Result<JsValue, JsValue> {
    let current_board: game::BoardState =
        serde_json::from_str(current_board_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let current_deck: game::DeckState =
        serde_json::from_str(current_deck_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let drawn_cards: [game::Card; 2] =
        serde_json::from_str(drawn_cards_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let remaining_skills: game::SkillsState = serde_json::from_str(remaining_skills_json)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    let req = CalculateRequest {
        current_board,
        current_deck,
        drawn_cards,
        remaining_skills,
        simulation_depth,
        uct_max_depth: solver::UCT_MAX_TREE_DEPTH,
    };

    let (ev, recommendation) = solver::calculate(req);
    let resp = CalculateResponse {
        ev,
        recommendation: recommendation.into(),
    };
    serde_wasm_bindgen::to_value(&resp).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Like [`calculate_optimal_move`], but invokes `progress` with a JSON-like object on throttled rollout ticks.
#[wasm_bindgen]
pub fn calculate_optimal_move_with_progress(
    current_board_json: &str,
    current_deck_json: &str,
    drawn_cards_json: &str,
    remaining_skills_json: &str,
    simulation_depth: u32,
    progress: &Function,
) -> Result<JsValue, JsValue> {
    let current_board: game::BoardState =
        serde_json::from_str(current_board_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let current_deck: game::DeckState =
        serde_json::from_str(current_deck_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let drawn_cards: [game::Card; 2] =
        serde_json::from_str(drawn_cards_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let remaining_skills: game::SkillsState = serde_json::from_str(remaining_skills_json)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    let req = CalculateRequest {
        current_board,
        current_deck,
        drawn_cards,
        remaining_skills,
        simulation_depth,
        uct_max_depth: solver::UCT_MAX_TREE_DEPTH,
    };

    let (ev, recommendation) = solver::calculate_with_progress(req, |sp| {
        let h = solver_progress_to_human(sp);
        if let Ok(v) = serde_wasm_bindgen::to_value(&h) {
            let _ = progress.call1(&JsValue::NULL, &v);
        }
    });

    let resp = CalculateResponse {
        ev,
        recommendation: recommendation.into(),
    };
    serde_wasm_bindgen::to_value(&resp).map_err(|e| JsValue::from_str(&e.to_string()))
}

