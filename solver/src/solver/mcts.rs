//! Full UCT (Upper Confidence bounds applied to Trees) for single-player move selection.
//!
//! - **Tree:** each node is a *decision* (board + deck + skills) and the two-card **deal** for
//!   that turn. Edges are legal [`super::RecommendedMove`] values in the same order as
//!   [`super::list_all_moves`].
//! - **Selection / expansion:** standard UCT with `Q + C * sqrt(ln(N) / n)`; unexpanded children
//!   are preferred (`+∞` UCB).
//! - **Depth limit:** nodes at `depth >= max_tree_depth` do not expand; a **playout** runs one
//!   greedy turn (same policy as rollouts) then [`super::rollout_play_to_end`]. This keeps the
//!   tree bounded for web/WASM while still looking multiple real turns ahead.
//! - **Value:** terminal and playout values use [`super::board_value`] (heuristic + completed
//!   lines), matching the rest of the solver.

use super::{
    apply_move_and_advance, board_value, can_start_full_turn, play_one_greedy_turn,
    peek_two_cards, rollout_play_to_end, RecommendedMove, SimState,
};
use crate::game;
use rand::{rngs::SmallRng, SeedableRng};

/// Exploration constant (√2 is a common default for UCT).
const UCT_C: f64 = 1.4142135623730951;

/// Default maximum **decision depth** to expand (nodes with `depth < this` may grow children).
/// Deeper search grows cost quickly; 4 real turns of lookahead is a practical whole-game default.
pub const DEFAULT_MAX_TREE_DEPTH: u8 = 4;

/// UCT state node: one “turn to decide” or a terminal snapshot.
pub(super) struct MctsNode {
    /// Game state *before* applying one of `actions` with `deal`.
    pub(super) state: SimState,
    /// The two cards for this decision (at the app root they come from JSON, not from `deck`).
    pub(super) deal: [game::Card; 2],
    pub(super) depth: u8,
    pub(super) actions: Vec<RecommendedMove>,
    pub(super) children: Vec<Option<Box<MctsNode>>>,
    pub(super) terminal_override: Option<f64>,
    pub(super) visits: u64,
    pub(super) value_sum: f64,
}

fn leaf_node(state: SimState, depth: u8, terminal_score: f64) -> MctsNode {
    MctsNode {
        deal: [dummy_card(); 2],
        state,
        depth,
        actions: Vec::new(),
        children: Vec::new(),
        terminal_override: Some(terminal_score),
        visits: 0,
        value_sum: 0.0,
    }
}

#[inline]
fn dummy_card() -> game::Card {
    game::Card {
        suit: game::Suit::Earth,
        rank: game::Rank::Two,
    }
}

pub(super) fn create_child(
    parent: &MctsNode,
    action_idx: usize,
    rng: &mut SmallRng,
) -> MctsNode {
    let mut s = parent.state.clone();
    apply_move_and_advance(rng, &mut s, parent.deal, &parent.actions[action_idx]);
    let score = super::board_value(&s.board);

    if s.board.filled_count() == super::BOARD_CELLS {
        return leaf_node(s, parent.depth + 1, score);
    }
    if !can_start_full_turn(s.deck.len()) {
        return leaf_node(s, parent.depth + 1, score);
    }
    if s.board.valid_placements().is_empty() {
        return leaf_node(s, parent.depth + 1, score);
    }
    let deal = match peek_two_cards(&s.deck) {
        Some(d) => d,
        None => {
            return leaf_node(s, parent.depth + 1, score);
        }
    };
    MctsNode {
        state: s,
        deal,
        depth: parent.depth + 1,
        actions: Vec::new(),
        children: Vec::new(),
        terminal_override: None,
        visits: 0,
        value_sum: 0.0,
    }
}

#[inline]
pub(super) fn ucb1_score(parent_visits: u64, child: &MctsNode, c: f64) -> f64 {
    let n = child.visits.max(1) as f64;
    let q = child.value_sum / n;
    let logp = (parent_visits.max(1) as f64).ln();
    q + c * (logp / n).sqrt()
}

pub(super) fn best_child_ucb(node: &MctsNode, c: f64) -> usize {
    let pv = node.visits.max(1);
    let mut best_i = 0usize;
    let mut best = f64::NEG_INFINITY;
    for i in 0..node.actions.len() {
        let u = match &node.children[i] {
            None => f64::INFINITY,
            Some(ch) => ucb1_score(pv, ch, c),
        };
        if u > best {
            best = u;
            best_i = i;
        }
    }
    best_i
}

pub(super) fn simulate_recursive(
    node: &mut MctsNode,
    max_tree_depth: u8,
    rng: &mut SmallRng,
    c: f64,
) -> f64 {
    if let Some(v) = node.terminal_override {
        node.visits += 1;
        node.value_sum += v;
        return v;
    }
    if node.depth >= max_tree_depth {
        let mut s = node.state.clone();
        play_one_greedy_turn(rng, &mut s, node.deal);
        rollout_play_to_end(rng, &mut s);
        let r = board_value(&s.board);
        node.visits += 1;
        node.value_sum += r;
        return r;
    }
    if node.actions.is_empty() {
        node.actions = super::list_all_moves(&node.state, &node.state.skills, node.deal);
        node.children = (0..node.actions.len()).map(|_| None).collect();
    }
    if node.actions.is_empty() {
        let r = board_value(&node.state.board);
        node.visits += 1;
        node.value_sum += r;
        return r;
    }
    if let Some(i) = (0..node.children.len()).find(|&i| node.children[i].is_none()) {
        let new_child = create_child(node, i, rng);
        let mut boxed = Box::new(new_child);
        let r = simulate_recursive(&mut *boxed, max_tree_depth, rng, c);
        node.children[i] = Some(boxed);
        node.visits += 1;
        node.value_sum += r;
        return r;
    }
    let i = best_child_ucb(node, c);
    let r = {
        let ch = node.children[i]
            .as_mut()
            .expect("child must exist when all slots expanded");
        simulate_recursive(&mut *ch, max_tree_depth, rng, c)
    };
    node.visits += 1;
    node.value_sum += r;
    r
}

/// Highest visit count wins; ties broken by higher mean value.
pub(super) fn best_action_index_by_visits(root: &MctsNode) -> Option<usize> {
    if root.actions.is_empty() {
        return None;
    }
    let mut best_i: Option<usize> = None;
    for i in 0..root.actions.len() {
        if let Some(ch) = &root.children[i] {
            let mean = ch.value_sum / ch.visits.max(1) as f64;
            let better = match best_i {
                None => true,
                Some(j) => {
                    let bch = root.children[j].as_ref().unwrap();
                    let bv = bch.visits;
                    let bm = bch.value_sum / bch.visits.max(1) as f64;
                    ch.visits > bv || (ch.visits == bv && mean > bm)
                }
            };
            if better {
                best_i = Some(i);
            }
        }
    }
    best_i
}

/// Run UCT. `iterations` = MCTS simulations; `max_tree_depth` = expand nodes with `depth < this`
/// (typical 3–5 for responsive play).
pub fn run_uct(
    base: SimState,
    root_deal: [game::Card; 2],
    iterations: usize,
    max_tree_depth: u8,
    mut on_step: impl FnMut(u64, u64, f64, f64, &RecommendedMove),
) -> (f64, RecommendedMove) {
    let max_tree_depth = max_tree_depth.max(1);
    let mut root = MctsNode {
        state: base,
        deal: root_deal,
        depth: 0,
        actions: Vec::new(),
        children: Vec::new(),
        terminal_override: None,
        visits: 0,
        value_sum: 0.0,
    };
    let mut rng = SmallRng::seed_from_u64(0xF4A7_4A01_5EED_u64);
    for it in 1..=iterations {
        let _pl = simulate_recursive(&mut root, max_tree_depth, &mut rng, UCT_C);
        let mean = if root.visits > 0 {
            root.value_sum / root.visits as f64
        } else {
            0.0
        };
        let best_idx = best_action_index_by_visits(&root).unwrap_or(0);
        let best_move = if root.actions.is_empty() {
            RecommendedMove::KeepOne {
                keep_index: 0,
                x: 0,
                y: 0,
            }
        } else {
            root.actions[best_idx].clone()
        };
        on_step(
            it as u64,
            iterations as u64,
            mean,
            _pl,
            &best_move,
        );
    }
    let mean = if root.visits > 0 {
        root.value_sum / root.visits as f64
    } else {
        0.0
    };
    let idx = best_action_index_by_visits(&root).unwrap_or(0);
    if root.actions.is_empty() {
        return (
            mean,
            RecommendedMove::KeepOne {
                keep_index: 0,
                x: 0,
                y: 0,
            },
        );
    }
    (mean, root.actions[idx].clone())
}
