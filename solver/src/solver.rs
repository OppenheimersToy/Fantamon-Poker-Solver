//! Monte Carlo move search.
//!
//! **Objective:** maximize the expected **total poker score** summed over every
//! **completed** row and column (10 lines). Partial lines (fewer than 5 cards) score 0
//! until that line is complete.
//!
//! **Terminal state (rollout stops):** the grid has **25 filled cells** (all lines then
//! contribute), **or** the deck has **fewer than 2 cards** so another full
//! deal–select–place turn cannot occur—**remaining deck cards are unused**, which matches
//! real play when skills are not used to burn through the deck. A rare third stop is
//! **no legal empty cell** while the board is not full (stuck); incomplete lines still score 0.

use crate::{eval, game};
use rand::{rngs::SmallRng, Rng, SeedableRng};
use serde::{Deserialize, Serialize};

const BOARD_CELLS: usize = 25;

#[inline]
fn can_start_full_turn(deck_len: usize) -> bool {
    deck_len >= 2
}

/// Live progress tick for Monte Carlo UI (throttled during rollouts).
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
    /// This recommendation signals that the best action is to reroll the deal.
    DiscardBoth,
}

#[derive(Clone)]
struct SimState {
    board: game::BoardState,
    deck: Vec<game::Card>,
    skills: game::SkillsState,
}

fn count_candidates(base: &SimState, skills: &game::SkillsState, drawn: [game::Card; 2]) -> u32 {
    let valid = base.board.valid_placements();
    let mut n = (2 * valid.len()) as u32;
    if skills.keep_both > 0 {
        for first_keep_index in 0u8..2 {
            for &(x1, y1) in &valid {
                let mut b1 = base.board.clone();
                b1.set(x1, y1, Some(drawn[first_keep_index as usize]));
                let valid2 = b1.valid_placements();
                n += valid2.len() as u32;
            }
        }
    }
    if skills.discard_both > 0 {
        n += 1;
    }
    n
}

/// Returns `(EV, best_move)` where EV is the mean **terminal total line score** after
/// rollouts (see module docs). No progress callbacks.
pub fn calculate(req: crate::CalculateRequest) -> (f64, RecommendedMove) {
    calculate_with_progress(req, |_| ())
}

/// Same as [`calculate`], but invokes `on_progress` on throttled rollout ticks for UI charts.
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

    let rollouts = req.simulation_depth.max(50) as usize;
    let mut rng = SmallRng::seed_from_u64(0xF4A7_4A01_5EED_u64);

    let candidates_total = count_candidates(&base, &base.skills, req.drawn_cards);
    let total_rollouts_planned = (candidates_total as u64) * (rollouts as u64);
    let mut total_rollouts_done: u64 = 0;
    let mut candidate_index: u32 = 0;

    let mut best_ev = f64::NEG_INFINITY;
    let mut second_best_ev = f64::NEG_INFINITY;
    let mut best_move = RecommendedMove::KeepOne {
        keep_index: 0,
        x: 0,
        y: 0,
    };

    let valid = base.board.valid_placements();

    // KeepOne candidates.
    for keep_idx in 0u8..2 {
        for &(x, y) in &valid {
            let mv = RecommendedMove::KeepOne {
                keep_index: keep_idx,
                x,
                y,
            };
            let ev = estimate_ev_for_move(
                &mut rng,
                &base,
                req.drawn_cards,
                &mv,
                rollouts,
                &mut total_rollouts_done,
                candidate_index,
                candidates_total,
                total_rollouts_planned,
                best_ev,
                second_best_ev,
                &best_move,
                &mut on_progress,
            );
            if ev > best_ev {
                second_best_ev = best_ev;
                best_ev = ev;
                best_move = mv.clone();
            } else if ev > second_best_ev {
                second_best_ev = ev;
            }
            candidate_index += 1;
        }
    }

    // KeepBoth candidates.
    if base.skills.keep_both > 0 {
        for first_keep_index in 0u8..2 {
            for &(x1, y1) in &valid {
                let mut b1 = base.board.clone();
                b1.set(x1, y1, Some(req.drawn_cards[first_keep_index as usize]));
                let valid2 = b1.valid_placements();
                if valid2.is_empty() {
                    continue;
                }
                for &(x2, y2) in &valid2 {
                    let mv = RecommendedMove::KeepBoth {
                        first_keep_index,
                        first: (x1, y1),
                        second: (x2, y2),
                    };
                    let ev = estimate_ev_for_move(
                        &mut rng,
                        &base,
                        req.drawn_cards,
                        &mv,
                        rollouts,
                        &mut total_rollouts_done,
                        candidate_index,
                        candidates_total,
                        total_rollouts_planned,
                        best_ev,
                        second_best_ev,
                        &best_move,
                        &mut on_progress,
                    );
                    if ev > best_ev {
                        second_best_ev = best_ev;
                        best_ev = ev;
                        best_move = mv.clone();
                    } else if ev > second_best_ev {
                        second_best_ev = ev;
                    }
                    candidate_index += 1;
                }
            }
        }
    }

    // DiscardBoth candidate.
    if base.skills.discard_both > 0 {
        let mv = RecommendedMove::DiscardBoth;
        let ev = estimate_ev_for_move(
            &mut rng,
            &base,
            req.drawn_cards,
            &mv,
            rollouts,
            &mut total_rollouts_done,
            candidate_index,
            candidates_total,
            total_rollouts_planned,
            best_ev,
            second_best_ev,
            &best_move,
            &mut on_progress,
        );
        if ev > best_ev {
            second_best_ev = best_ev;
            best_ev = ev;
            best_move = mv.clone();
        } else if ev > second_best_ev {
            second_best_ev = ev;
        }
        candidate_index += 1;
    }

    on_progress(SolverProgress {
        phase: "done",
        candidates_total,
        candidate_index: candidate_index.saturating_sub(1),
        total_rollouts_done,
        total_rollouts_planned,
        rollout_in_candidate: rollouts as u32,
        rollouts_per_candidate: rollouts as u32,
        running_mean_ev: best_ev,
        best_ev_so_far: best_ev,
        second_best_ev_so_far: second_best_ev,
        current_candidate: best_move.clone(),
        best_move_so_far: best_move.clone(),
    });

    (best_ev, best_move)
}

/// Average `total_line_score` after applying `mv`, then simulating to a terminal state.
fn estimate_ev_for_move<F>(
    rng: &mut SmallRng,
    base: &SimState,
    drawn: [game::Card; 2],
    mv: &RecommendedMove,
    rollouts: usize,
    total_rollouts_done: &mut u64,
    candidate_index: u32,
    candidates_total: u32,
    total_rollouts_planned: u64,
    best_ev_so_far: f64,
    second_best_ev_so_far: f64,
    best_move_so_far: &RecommendedMove,
    on_progress: &mut F,
) -> f64
where
    F: FnMut(SolverProgress),
{
    let throttle = (rollouts / 60).max(1);
    let mut sum = 0f64;
    for i in 0..rollouts {
        let mut s = base.clone();
        apply_move_and_advance(rng, &mut s, drawn, mv);
        rollout_play_to_end(rng, &mut s);
        sum += total_line_score(&s.board) as f64;
        *total_rollouts_done += 1;
        let done = (i + 1) as u32;
        let running = sum / (i + 1) as f64;
        if done == 1 || done % throttle as u32 == 0 || i + 1 == rollouts {
            on_progress(SolverProgress {
                phase: "rollout",
                candidates_total,
                candidate_index,
                total_rollouts_done: *total_rollouts_done,
                total_rollouts_planned,
                rollout_in_candidate: done,
                rollouts_per_candidate: rollouts as u32,
                running_mean_ev: running,
                best_ev_so_far,
                second_best_ev_so_far: second_best_ev_so_far,
                current_candidate: mv.clone(),
                best_move_so_far: best_move_so_far.clone(),
            });
        }
    }
    sum / (rollouts as f64)
}

/// Greedy rollouts until the board is full or no full turn can be played (see module docs).
fn rollout_play_to_end(rng: &mut SmallRng, s: &mut SimState) {
    // Heuristic, skill-aware rollout:
    // - Prefer placements that increase completed row/col scores (delta on completed lines).
    // - Use KeepBoth only if it beats KeepOne by a margin (opportunity cost).
    // - Use DiscardBoth only if the deal is very poor (and we have uses left).
    const KEEP_BOTH_COST: f64 = 2.0;
    const DISCARD_BOTH_COST: f64 = 0.8;

    while s.board.filled_count() < BOARD_CELLS && can_start_full_turn(s.deck.len()) {
        let deal = [draw_top(&mut s.deck), draw_top(&mut s.deck)];

        let mut one0 = best_keep_one_placement(s, deal[0]);
        let mut one1 = best_keep_one_placement(s, deal[1]);
        one0.2 = 0;
        one1.2 = 1;
        let best_one = if one0.0 >= one1.0 { one0 } else { one1 };

        // Evaluate KeepBoth (both orders) if available.
        let mut best_both = (f64::NEG_INFINITY, None::<((i32, i32), (i32, i32), u8)>);
        if s.skills.keep_both > 0 {
            if let Some(v) = best_keep_both_placements(s, deal) {
                best_both = (v.0, Some((v.1, v.2, v.3)));
            }
        }

        // Evaluate DiscardBoth if available.
        let should_discard = s.skills.discard_both > 0
            && best_one.0 < 0.5
            && (best_both.0.is_finite() == false || best_both.0 < 1.0);

        if should_discard {
            s.skills.discard_both -= 1;
            // Discarding is equivalent to consuming the turn; continue loop.
            // Apply a soft opportunity cost by reducing future potential via consuming the skill:
            // (we model this by a slight random "waste" draw sometimes).
            if rng.gen_bool((DISCARD_BOTH_COST / 10.0).clamp(0.0, 0.2)) && can_start_full_turn(s.deck.len()) {
                let _ = draw_top(&mut s.deck);
                let _ = draw_top(&mut s.deck);
            }
            continue;
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
            // Keep one: choose best delta placement; if no delta info, random valid.
            let (delta, pos, card_idx) = best_one;
            let valid = s.board.valid_placements();
            if valid.is_empty() {
                break;
            }
            let (x, y) = pos.unwrap_or_else(|| valid[rng.gen_range(0..valid.len())]);
            let card = deal[card_idx as usize];
            let _ = delta;
            s.board.set(x, y, Some(card));
        }
    }
}

fn best_keep_one_placement(s: &SimState, card: game::Card) -> (f64, Option<(i32, i32)>, u8) {
    // Returns: (best_delta_score, best_pos, which_card_index_placeholder)
    // The last field is filled by caller (0/1); here set to 0.
    let valid = s.board.valid_placements();
    if valid.is_empty() {
        return (f64::NEG_INFINITY, None, 0);
    }
    let base_score = total_line_score(&s.board) as i32;
    let mut best = (f64::NEG_INFINITY, None);
    for &(x, y) in &valid {
        let mut b2 = s.board.clone();
        b2.set(x, y, Some(card));
        let sc = total_line_score(&b2) as i32;
        let delta = (sc - base_score) as f64;
        if delta > best.0 {
            best = (delta, Some((x, y)));
        }
    }
    // If all deltas are 0 (likely early game), return 0 with a position so we still behave consistently.
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
    // Returns: (combined_delta, first_pos, second_pos, first_card_index)
    let base_score = total_line_score(&s.board) as i32;
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
                let sc = total_line_score(&b2) as i32;
                let delta = (sc - base_score) as f64;
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

fn apply_move_and_advance(
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
            // Replace the current deal with two random cards from deck (if possible).
            if s.deck.len() >= 2 {
                let d1 = draw_top(&mut s.deck);
                let d2 = draw_top(&mut s.deck);
                // Then do a simple greedy: choose the higher immediate-score placement among the two.
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
                        let sc = total_line_score(&b2) as i32;
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
                    // fallback random
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

/// Sum of poker points for each **complete** row and column (10 lines). Incomplete lines = 0.
fn total_line_score(board: &game::BoardState) -> u32 {
    let mut lines = [[None; 5]; 10];
    for y in 0..5 {
        lines[y as usize] = board.row(y);
    }
    for x in 0..5 {
        lines[(5 + x) as usize] = board.col(x);
    }
    eval::board_score(&lines)
}

