//! Monte Carlo + **UCT** (full MCTS-style) move search.
//!
//! **Search:** multi-ply UCT with a tunable depth cap (see [`mcts::DEFAULT_MAX_TREE_DEPTH`]) so
//! the tree stays tractable in WASM while allocating simulations toward promising moves.
//!
//! **Objective:** maximize expected **board value** — [`crate::eval::board_total_value`] blends
//! exact poker scores on finished lines with heuristic *potential* on open rows/columns (see
//! [`crate::eval::evaluate_partial_line`]).
//!
//! **Simulation policy:** after the tree stops expanding, one **greedy** turn (same policy as
//! [`rollout_play_to_end`]) plus rollout-to-terminal completes each playout.

mod mcts;

/// Default UCT tree depth (expand nodes with `depth < this`). Tunable via [`crate::CalculateRequest::uct_max_depth`].
pub const UCT_MAX_TREE_DEPTH: u8 = mcts::DEFAULT_MAX_TREE_DEPTH;

use crate::{eval, game};
use rand::{rngs::SmallRng, Rng};
use serde::{Deserialize, Serialize};

/// Cells on the 5×5 board (exported for the UCT child builder).
pub(crate) const BOARD_CELLS: usize = 25;

#[inline]
pub(crate) fn can_start_full_turn(deck_len: usize) -> bool {
    deck_len >= 2
}

/// Live progress tick for Monte Carlo UI (throttled during search).
#[derive(Debug, Clone, Serialize)]
pub struct SolverProgress {
    pub phase: &'static str,
    pub candidates_total: u32,
    pub candidate_index: u32,
    pub total_rollouts_done: u64,
    pub total_rollouts_planned: u64,
    pub rollout_in_candidate: u32,
    pub rollouts_per_candidate: u32,
    pub running_mean_ev: f64,
    pub best_ev_so_far: f64,
    pub second_best_ev_so_far: f64,
    pub current_candidate: RecommendedMove,
    pub best_move_so_far: RecommendedMove,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RecommendedMove {
    /// Keep one of the drawn cards and place it at (x,y).
    KeepOne {
        keep_index: u8, // 0 or 1
        x: i32,
        y: i32,
    },
    /// Use "Keep Both" skill: place keep_index_first then the other card.
    KeepBoth {
        first_keep_index: u8, // 0 or 1
        first: (i32, i32),
        second: (i32, i32),
    },
    /// Use "Discard Both" skill, then (in expectation) place a kept card.
    DiscardBoth,
}

#[derive(Clone)]
pub(crate) struct SimState {
    pub board: game::BoardState,
    pub deck: Vec<game::Card>,
    pub skills: game::SkillsState,
}

/// Next two cards that would be drawn (deck pops from the **end** — same order as `draw_top`).
pub(crate) fn peek_two_cards(deck: &[game::Card]) -> Option<[game::Card; 2]> {
    let n = deck.len();
    if n < 2 {
        return None;
    }
    Some([deck[n - 1], deck[n - 2]])
}

/// Enumerate legal moves in the **same order** as the pre-UCT solver (stable indices for debugging).
pub(crate) fn list_all_moves(state: &SimState, skills: &game::SkillsState, drawn: [game::Card; 2]) -> Vec<RecommendedMove> {
    let mut out = Vec::new();
    let valid = state.board.valid_placements();
    for keep_idx in 0u8..2 {
        for &(x, y) in &valid {
            out.push(RecommendedMove::KeepOne {
                keep_index: keep_idx,
                x,
                y,
            });
        }
    }
    if skills.keep_both > 0 {
        for first_keep_index in 0u8..2 {
            for &(x1, y1) in &valid {
                let mut b1 = state.board.clone();
                b1.set(x1, y1, Some(drawn[first_keep_index as usize]));
                let valid2 = b1.valid_placements();
                if valid2.is_empty() {
                    continue;
                }
                for &(x2, y2) in &valid2 {
                    out.push(RecommendedMove::KeepBoth {
                        first_keep_index,
                        first: (x1, y1),
                        second: (x2, y2),
                    });
                }
            }
        }
    }
    if skills.discard_both > 0 {
        out.push(RecommendedMove::DiscardBoth);
    }
    out
}

/// One turn of the greedy rollout policy: given the current `deal`, update `s` in place
/// (draw is *not* popped from the deck here — the caller passes `deal` explicitly).
pub(crate) fn play_one_greedy_turn(rng: &mut SmallRng, s: &mut SimState, deal: [game::Card; 2]) {
    const KEEP_BOTH_COST: f64 = 2.0;
    const DISCARD_BOTH_COST: f64 = 0.8;

    let mut one0 = best_keep_one_placement(s, deal[0]);
    let mut one1 = best_keep_one_placement(s, deal[1]);
    one0.2 = 0;
    one1.2 = 1;
    let best_one = if one0.0 >= one1.0 { one0 } else { one1 };

    let mut best_both = (f64::NEG_INFINITY, None::<((i32, i32), (i32, i32), u8)>);
    if s.skills.keep_both > 0 {
        if let Some(v) = best_keep_both_placements(s, deal) {
            best_both = (v.0, Some((v.1, v.2, v.3)));
        }
    }

    let should_discard = s.skills.discard_both > 0
        && best_one.0 < 0.5
        && (!best_both.0.is_finite() || best_both.0 < 1.0);

    if should_discard {
        s.skills.discard_both -= 1;
        if rng.gen_bool((DISCARD_BOTH_COST / 10.0).clamp(0.0, 0.2)) && can_start_full_turn(s.deck.len()) {
            let _ = draw_top(&mut s.deck);
            let _ = draw_top(&mut s.deck);
        }
        return;
    }

    let use_keep_both = best_both.1.is_some() && (best_both.0 - KEEP_BOTH_COST) > best_one.0;

    if use_keep_both {
        s.skills.keep_both -= 1;
        let ((x1, y1), (x2, y2), first_idx) = best_both.1.unwrap();
        let c1 = deal[first_idx as usize];
        let c2 = deal[(1 - first_idx) as usize];
        s.board.set(x1, y1, Some(c1));
        s.board.set(x2, y2, Some(c2));
    } else {
        let (delta, pos, card_idx) = best_one;
        let valid = s.board.valid_placements();
        if valid.is_empty() {
            return;
        }
        let (x, y) = pos.unwrap_or_else(|| valid[rng.gen_range(0..valid.len())]);
        let card = deal[card_idx as usize];
        let _ = delta;
        s.board.set(x, y, Some(card));
    }
}

fn count_candidates(base: &SimState, skills: &game::SkillsState, drawn: [game::Card; 2]) -> u32 {
    list_all_moves(base, skills, drawn).len() as u32
}

/// Returns `(EV, best_move)` where EV is the mean **root board value** from UCT.
pub fn calculate(req: crate::CalculateRequest) -> (f64, RecommendedMove) {
    calculate_with_progress(req, |_| ())
}

/// Same as [`calculate`], but invokes `on_progress` on throttled UCT steps for UI charts.
pub fn calculate_with_progress<F>(req: crate::CalculateRequest, mut on_progress: F) -> (f64, RecommendedMove)
where
    F: FnMut(SolverProgress),
{
    let mut base = SimState {
        board: req.current_board,
        deck: req.current_deck.cards,
        skills: req.remaining_skills,
    };

    base.deck.reverse();

    if base.board.cells.len() != BOARD_CELLS {
        base.board.cells.resize(BOARD_CELLS, None);
    }

    // Total UCT simulations (was “rollouts per candidate”).
    let iterations = req.simulation_depth.max(80) as usize;
    let candidates_total = count_candidates(&base, &base.skills, req.drawn_cards);
    let total_rollouts_planned = iterations as u64;
    let mut total_rollouts_done: u64 = 0;
    let throttle = (iterations / 60).max(1);

    let mut best_ev_so_far = f64::NEG_INFINITY;
    let mut second_best_ev_so_far = f64::NEG_INFINITY;
    let mut best_move_so_far = RecommendedMove::KeepOne {
        keep_index: 0,
        x: 0,
        y: 0,
    };

    let uct_depth = req.uct_max_depth.clamp(1, 16);
    let (ev, best_move) = mcts::run_uct(
        base,
        req.drawn_cards,
        iterations,
        uct_depth,
        |done, planned, running_mean, _last_playout, cur_best| {
            total_rollouts_done = done;
            if running_mean > best_ev_so_far {
                second_best_ev_so_far = best_ev_so_far;
                best_ev_so_far = running_mean;
            } else if running_mean > second_best_ev_so_far {
                second_best_ev_so_far = running_mean;
            }
            best_move_so_far = cur_best.clone();
            if done == 1 || done % throttle as u64 == 0 || done == planned {
                on_progress(SolverProgress {
                    phase: "uct",
                    candidates_total,
                    candidate_index: 0,
                    total_rollouts_done: done,
                    total_rollouts_planned: planned,
                    rollout_in_candidate: done as u32,
                    rollouts_per_candidate: planned as u32,
                    running_mean_ev: running_mean,
                    best_ev_so_far,
                    second_best_ev_so_far,
                    current_candidate: cur_best.clone(),
                    best_move_so_far: best_move_so_far.clone(),
                });
            }
        },
    );

    on_progress(SolverProgress {
        phase: "done",
        candidates_total,
        candidate_index: 0,
        total_rollouts_done,
        total_rollouts_planned,
        rollout_in_candidate: iterations as u32,
        rollouts_per_candidate: iterations as u32,
        running_mean_ev: ev,
        best_ev_so_far: ev,
        second_best_ev_so_far,
        current_candidate: best_move.clone(),
        best_move_so_far: best_move.clone(),
    });

    (ev, best_move)
}

/// Greedy rollouts until the board is full or no full turn can be played (see module docs).
pub(crate) fn rollout_play_to_end(rng: &mut SmallRng, s: &mut SimState) {
    while s.board.filled_count() < BOARD_CELLS && can_start_full_turn(s.deck.len()) {
        let deal = [draw_top(&mut s.deck), draw_top(&mut s.deck)];
        play_one_greedy_turn(rng, s, deal);
    }
}

fn best_keep_one_placement(s: &SimState, card: game::Card) -> (f64, Option<(i32, i32)>, u8) {
    let valid = s.board.valid_placements();
    if valid.is_empty() {
        return (f64::NEG_INFINITY, None, 0);
    }
    let base_score = board_value(&s.board);
    let mut best = (f64::NEG_INFINITY, None);
    for &(x, y) in &valid {
        let mut b2 = s.board.clone();
        b2.set(x, y, Some(card));
        let sc = board_value(&b2);
        let delta = sc - base_score;
        if delta > best.0 {
            best = (delta, Some((x, y)));
        }
    }
    if !best.0.is_finite() {
        (0.0, Some(valid[0]), 0)
    } else {
        (best.0, best.1, 0)
    }
}

fn best_keep_both_placements(
    s: &SimState,
    deal: [game::Card; 2],
) -> Option<(f64, (i32, i32), (i32, i32), u8)> {
    let base_score = board_value(&s.board);
    let valid1 = s.board.valid_placements();
    if valid1.is_empty() {
        return None;
    }
    let mut best: Option<(f64, (i32, i32), (i32, i32), u8)> = None;

    for first_idx in 0u8..2 {
        let c1 = deal[first_idx as usize];
        let c2 = deal[(1 - first_idx) as usize];
        for &(x1, y1) in &valid1 {
            let mut b1 = s.board.clone();
            b1.set(x1, y1, Some(c1));
            let valid2 = b1.valid_placements();
            if valid2.is_empty() {
                continue;
            }
            for &(x2, y2) in &valid2 {
                let mut b2 = b1.clone();
                b2.set(x2, y2, Some(c2));
                let sc = board_value(&b2);
                let delta = sc - base_score;
                match best {
                    None => best = Some((delta, (x1, y1), (x2, y2), first_idx)),
                    Some((bdelta, _, _, _)) if delta > bdelta => {
                        best = Some((delta, (x1, y1), (x2, y2), first_idx))
                    }
                    _ => {}
                }
            }
        }
    }
    best
}

pub(crate) fn apply_move_and_advance(
    rng: &mut SmallRng,
    s: &mut SimState,
    drawn: [game::Card; 2],
    mv: &RecommendedMove,
) {
    match *mv {
        RecommendedMove::KeepOne { keep_index, x, y } => {
            let c = drawn[keep_index as usize];
            s.board.set(x, y, Some(c));
        }
        RecommendedMove::KeepBoth {
            first_keep_index,
            first: (x1, y1),
            second: (x2, y2),
        } => {
            if s.skills.keep_both > 0 {
                s.skills.keep_both -= 1;
            }
            let c1 = drawn[first_keep_index as usize];
            let c2 = drawn[(1 - first_keep_index) as usize];
            s.board.set(x1, y1, Some(c1));
            s.board.set(x2, y2, Some(c2));
        }
        RecommendedMove::DiscardBoth => {
            if s.skills.discard_both > 0 {
                s.skills.discard_both -= 1;
            }
            if s.deck.len() >= 2 {
                let d1 = draw_top(&mut s.deck);
                let d2 = draw_top(&mut s.deck);
                let valid = s.board.valid_placements();
                if valid.is_empty() {
                    return;
                }
                let mut best = None;
                for keep_idx in 0..2u8 {
                    let c = if keep_idx == 0 { d1 } else { d2 };
                    for &(x, y) in &valid {
                        let mut b2 = s.board.clone();
                        b2.set(x, y, Some(c));
                        let sc = board_value(&b2);
                        match best {
                            None => best = Some((sc, x, y, c)),
                            Some((bsc, _, _, _)) if sc > bsc => best = Some((sc, x, y, c)),
                            _ => {}
                        }
                    }
                }
                if let Some((_sc, x, y, c)) = best {
                    s.board.set(x, y, Some(c));
                } else {
                    let (x, y) = valid[rng.gen_range(0..valid.len())];
                    s.board.set(x, y, Some(d1));
                }
            }
        }
    }
}

#[inline]
fn draw_top(deck: &mut Vec<game::Card>) -> game::Card {
    deck.pop().expect("deck underflow")
}

pub(crate) fn board_value(board: &game::BoardState) -> f64 {
    let mut lines = [[None; 5]; 10];
    for y in 0..5 {
        lines[y as usize] = board.row(y);
    }
    for x in 0..5 {
        lines[(5 + x) as usize] = board.col(x);
    }
    eval::board_total_value(&lines)
}
