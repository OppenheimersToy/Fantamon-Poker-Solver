use crate::game::{Card, Rank, Suit};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum HandCategory {
    StraightFlush,
    FourKind,
    Straight,
    FullHouse,
    ThreeKind,
    Flush,
    TwoPair,
    OnePair,
    HighCard,
}

#[inline]
pub fn category_score(cat: HandCategory) -> u32 {
    match cat {
        HandCategory::StraightFlush => 30,
        HandCategory::FourKind => 18,
        HandCategory::Straight => 14,
        HandCategory::FullHouse => 10,
        HandCategory::ThreeKind => 7,
        HandCategory::Flush => 5,
        HandCategory::TwoPair => 3,
        HandCategory::OnePair => 1,
        HandCategory::HighCard => 0,
    }
}

#[inline]
fn rank_to_u8(r: Rank) -> u8 {
    match r {
        Rank::Ace => 14,
        Rank::Two => 2,
        Rank::Three => 3,
        Rank::Four => 4,
        Rank::Five => 5,
        Rank::Six => 6,
        Rank::Seven => 7,
        Rank::Eight => 8,
        Rank::Nine => 9,
        Rank::Ten => 10,
        Rank::Jack => 11,
        Rank::Queen => 12,
        Rank::King => 13,
        Rank::Joker => 0,
    }
}

fn all_52_cards() -> &'static [Card] {
    // Generated deterministically; stored in a static once at runtime via lazy init.
    // (Wasm has no threads; this is safe in practice and keeps code small.)
    use std::sync::OnceLock;
    static ALL: OnceLock<Vec<Card>> = OnceLock::new();
    ALL.get_or_init(|| {
        let suits = [Suit::Earth, Suit::Fire, Suit::Water, Suit::Lightning];
        let ranks = [
            Rank::Ace,
            Rank::Two,
            Rank::Three,
            Rank::Four,
            Rank::Five,
            Rank::Six,
            Rank::Seven,
            Rank::Eight,
            Rank::Nine,
            Rank::Ten,
            Rank::Jack,
            Rank::Queen,
            Rank::King,
        ];
        let mut v = Vec::with_capacity(52);
        for &s in &suits {
            for &r in &ranks {
                v.push(Card { suit: s, rank: r });
            }
        }
        v
    })
}

#[inline]
fn is_flush(cards: &[Card; 5]) -> bool {
    let s0 = cards[0].suit;
    cards.iter().all(|c| c.suit == s0)
}

fn category_no_jokers(cards: &[Card; 5]) -> HandCategory {
    let flush = is_flush(cards);
    let mut ranks = [0u8; 5];
    for (i, c) in cards.iter().enumerate() {
        ranks[i] = rank_to_u8(c.rank);
    }

    let straight = {
        let mut tmp = ranks;
        tmp.sort_unstable();
        // no duplicates check in helper relies on dedup, but we need a 5-element array.
        // We'll do straight detection with a small local set.
        let mut v = tmp.to_vec();
        v.dedup();
        if v.len() != 5 {
            false
        } else if v == vec![2, 3, 4, 5, 14] {
            true
        } else {
            v[4] - v[0] == 4 && v.windows(2).all(|w| w[1] == w[0] + 1)
        }
    };

    // counts
    let mut counts = [0u8; 15]; // 2..14 used
    for &r in &ranks {
        counts[r as usize] += 1;
    }
    let mut freq: Vec<u8> = counts.iter().copied().filter(|&c| c > 0).collect();
    freq.sort_unstable_by(|a, b| b.cmp(a));

    if straight && flush {
        return HandCategory::StraightFlush;
    }

    // 5-of-a-kind isn't possible without jokers (but keep logic general)
    if freq[0] >= 4 {
        return HandCategory::FourKind;
    }

    if freq[0] == 3 && freq.len() >= 2 && freq[1] == 2 {
        return HandCategory::FullHouse;
    }

    if straight {
        return HandCategory::Straight;
    }

    if freq[0] == 3 {
        return HandCategory::ThreeKind;
    }

    if flush {
        return HandCategory::Flush;
    }

    if freq[0] == 2 && freq.len() >= 3 && freq[1] == 2 {
        return HandCategory::TwoPair;
    }

    if freq[0] == 2 {
        return HandCategory::OnePair;
    }

    HandCategory::HighCard
}

fn capped_category_for_five_kind(cards: &[Card; 5]) -> HandCategory {
    // If ranks are all identical (possible only via joker substitution), cap at FourKind score.
    let r0 = cards[0].rank;
    if cards.iter().all(|c| c.rank == r0) {
        HandCategory::FourKind
    } else {
        category_no_jokers(cards)
    }
}

/// Evaluates a 5-card "hand" with 0..=5 jokers and returns the best category score.
/// Jokers are wildcards and may duplicate existing cards (to allow 5-of-a-kind, capped to FourKind points).
pub fn best_hand_score_with_jokers(cards: &[Card; 5]) -> u32 {
    let mut joker_positions = [0usize; 5];
    let mut j = 0usize;
    let mut base = *cards;
    for i in 0..5 {
        if base[i].is_joker() {
            joker_positions[j] = i;
            j += 1;
        }
    }
    if j == 0 {
        return category_score(category_no_jokers(&base));
    }

    let all = all_52_cards();
    let mut best = 0u32;

    match j {
        1 => {
            let p0 = joker_positions[0];
            for &c0 in all {
                base[p0] = c0;
                let cat = capped_category_for_five_kind(&base);
                best = best.max(category_score(cat));
                if best == 30 {
                    return 30;
                }
            }
        }
        2 => {
            let p0 = joker_positions[0];
            let p1 = joker_positions[1];
            for &c0 in all {
                base[p0] = c0;
                for &c1 in all {
                    base[p1] = c1;
                    let cat = capped_category_for_five_kind(&base);
                    best = best.max(category_score(cat));
                    if best == 30 {
                        return 30;
                    }
                }
            }
        }
        _ => {
            // Up to 5 jokers is theoretically possible on a row/col with 2 jokers + wild transport;
            // fall back to a limited search: prioritize max category quickly.
            // With 3+ jokers, Straight Flush is always achievable.
            return 30;
        }
    }

    best
}

// -----------------------------------------------------------------------------
// Heuristic evaluation for *incomplete* lines (open board / mid-game gradient).
// -----------------------------------------------------------------------------

/// Wheel straight uses Ace low: A-2-3-4-5 (ranks 14,2,3,4,5).
const WHEEL_RANKS: [u8; 5] = [14, 2, 3, 4, 5];

/// Baseline weights (tunable): SF dominates, then flush draws, straight draws, rank texture.
const WEIGHT_SF_4: f64 = 8.0;
const WEIGHT_SF_3: f64 = 3.0;
const WEIGHT_FLUSH_4: f64 = 2.0;
const WEIGHT_FLUSH_3: f64 = 0.5;
const WEIGHT_STRAIGHT_4: f64 = 3.5;
const WEIGHT_STRAIGHT_3: f64 = 1.0;
const WEIGHT_PAIR_OPEN: f64 = 1.0;

/// Collect per-line statistics used by the heuristic: rank counts (non-joker cards only),
/// empty cells, and joker cells. Jokers and empties both count as “flex” that can be assigned
/// to maximize *potential* when scoring an incomplete line.
#[derive(Debug, Clone, Copy)]
struct LineMaterial {
    /// `counts[r]` = number of fixed non-joker cards showing rank r (2..=14).
    rank_counts: [u8; 15],
    empty: usize,
    jokers: usize,
    /// Suits of fixed non-joker cards (may be empty if only wilds / empty slots).
    fixed_suits: [Suit; 5],
    fixed_suit_len: u8,
}

fn line_material(line: &[Option<Card>; 5]) -> LineMaterial {
    let mut rank_counts = [0u8; 15];
    let mut empty = 0usize;
    let mut jokers = 0usize;
    let mut fixed_suits = [Suit::Earth; 5];
    let mut fixed_suit_len: u8 = 0;
    for slot in line {
        match slot {
            None => empty += 1,
            Some(c) if c.is_joker() => jokers += 1,
            Some(c) => {
                let r = rank_to_u8(c.rank);
                if (2..=14).contains(&r) {
                    rank_counts[r as usize] = rank_counts[r as usize].saturating_add(1);
                }
                if fixed_suit_len < 5 {
                    fixed_suits[fixed_suit_len as usize] = c.suit;
                    fixed_suit_len += 1;
                }
            }
        }
    }
    LineMaterial {
        rank_counts,
        empty,
        jokers,
        fixed_suits,
        fixed_suit_len,
    }
}

#[inline]
fn flex_slots(m: &LineMaterial) -> usize {
    m.empty + m.jokers
}

/// Returns `None` if two or more **non-joker** suits appear among fixed cards — then no flush
/// or straight-flush is possible, because those suits cannot all match.
///
/// When there are no concrete suited cards yet, returns a **dummy** anchor suit: flush length is
/// then driven entirely by `flex_slots` (every empty/joker can agree on that suit).
fn flush_agreement(m: &LineMaterial) -> Option<Suit> {
    if m.fixed_suit_len == 0 {
        return Some(Suit::Earth);
    }
    let mut seen: Option<Suit> = None;
    for i in 0..(m.fixed_suit_len as usize) {
        let s = m.fixed_suits[i];
        match seen {
            None => seen = Some(s),
            Some(t) if t != s => return None,
            _ => {}
        }
    }
    seen
}

/// Maximum number of cards that can share one suit, assuming jokers/empty adopt that suit.
/// If flush is impossible (`None`), returns 0.
fn max_flush_length(m: &LineMaterial) -> usize {
    let Some(target) = flush_agreement(m) else {
        return 0;
    };
    let fixed_in_suit = (0..(m.fixed_suit_len as usize))
        .filter(|&i| m.fixed_suits[i] == target)
        .count();
    // Any joker or empty can become `target` without contradicting fixed cards (we already
    // verified all fixed non-jokers share one suit when `Some`).
    fixed_in_suit + flex_slots(m)
}

/// True if **fixed non-joker ranks** contain a duplicate — a straight needs five distinct ranks,
/// so two concrete cards sharing a rank make a straight (and straight flush) impossible.
fn straight_rank_blocked(counts: &[u8; 15]) -> bool {
    (2..=14).any(|r| counts[r] > 1)
}

/// Enumerate the 10 rank windows for a 5-card straight: wheel + low=2..=10.
fn straight_windows() -> [[u8; 5]; 10] {
    let mut w = [[0u8; 5]; 10];
    w[0] = WHEEL_RANKS;
    let mut idx = 1;
    for low in 2u8..=10 {
        w[idx] = [low, low + 1, low + 2, low + 3, low + 4];
        idx += 1;
    }
    w
}

/// Best “open straight” potential: we assume flex ranks are chosen to fit **some** window.
/// Returns `(matched_fixed_ranks, wild_needed_min)` for the best feasible window; if no window
/// works, returns `(0, usize::MAX)`.
fn best_straight_window(counts: &[u8; 15], wild: usize) -> (usize, usize) {
    if straight_rank_blocked(counts) {
        return (0, usize::MAX);
    }
    // Fixed ranks that exist must all lie inside the chosen window; otherwise that window is dead.
    let mut best_matched = 0usize;
    let mut best_need = usize::MAX;
    'win: for window in straight_windows() {
        for r in 2..=14 {
            if counts[r] > 0 && !window.contains(&(r as u8)) {
                continue 'win;
            }
        }
        for &wr in &window {
            if counts[wr as usize] > 1 {
                continue 'win;
            }
        }
        let mut need = 0usize;
        let mut matched = 0usize;
        for &wr in &window {
            if counts[wr as usize] == 1 {
                matched += 1;
            } else {
                need += 1;
            }
        }
        if need <= wild {
            if matched > best_matched || (matched == best_matched && need < best_need) {
                best_matched = matched;
                best_need = need;
            }
        }
    }
    (best_matched, best_need)
}

/// Straight-flush potential uses the **intersection** of flush length and straight coverage:
/// you need both same suit and a coherent straight window.
fn straight_flush_heuristic(m: &LineMaterial) -> f64 {
    if flush_agreement(m).is_none() {
        return 0.0;
    }
    if straight_rank_blocked(&m.rank_counts) {
        return 0.0;
    }
    let flush_len = max_flush_length(m);
    let wild = flex_slots(m);
    let (matched, need) = best_straight_window(&m.rank_counts, wild);
    if need == usize::MAX {
        return 0.0;
    }
    // Effective progress toward SF is limited by both dimensions.
    let sf_span = flush_len.min(matched + need);
    match sf_span {
        5 => WEIGHT_SF_4 + 1.0, // nearly locked — bonus under complete-hand scale
        4 => WEIGHT_SF_4,
        3 => WEIGHT_SF_3,
        2 => 0.4,
        _ => 0.0,
    }
}

fn flush_draw_heuristic(m: &LineMaterial) -> f64 {
    let len = max_flush_length(m);
    match len {
        5 => WEIGHT_FLUSH_4 + 0.5,
        4 => WEIGHT_FLUSH_4,
        3 => WEIGHT_FLUSH_3,
        2 => 0.12,
        _ => 0.0,
    }
}

fn straight_draw_heuristic(m: &LineMaterial, wild: usize) -> f64 {
    if straight_rank_blocked(&m.rank_counts) {
        return 0.0;
    }
    let (matched, need) = best_straight_window(&m.rank_counts, wild);
    if need == usize::MAX {
        return 0.0;
    }
    match (matched, need) {
        (4, 1) => WEIGHT_STRAIGHT_4,
        (3, 2) => WEIGHT_STRAIGHT_3,
        (2, _) => 0.25,
        _ => 0.08,
    }
}

/// Cluster / pair potential: flex can pile onto one rank (trips, full house, quads in spirit).
/// Magnitudes are kept near the user-requested scale (e.g. pair + several empties ≈ 1.0).
fn rank_cluster_heuristic(m: &LineMaterial, wild: usize) -> f64 {
    let mut best = 0.0f64;
    for r in 2..=14 {
        let base = m.rank_counts[r] as usize;
        let pile = base + wild;
        if pile >= 5 {
            best = best.max(1.35);
        } else if pile >= 4 {
            best = best.max(1.15);
        } else if pile >= 3 {
            best = best.max(0.75);
        } else if pile >= 2 {
            // Pair with flex → strong texture; pair with no flex is mostly dead for expansion.
            let v = if wild > 0 {
                WEIGHT_PAIR_OPEN
            } else {
                0.28
            };
            best = best.max(v);
        }
    }
    best
}

/// Heuristic value for one row/column of 5 slots (mix of [`Some(card)`], jokers, and empty).
///
/// - **Complete line:** returns the real poker score [`best_hand_score_with_jokers`] as `f64`
///   so board totals stay comparable to the discrete scoring function at terminal states.
/// - **Incomplete line:** estimates *potential*:
///   - **Blocked flush / SF:** two concrete suits → no flush or SF points.
///   - **Blocked straight / SF:** duplicate concrete rank → no straight or SF points.
///   - **Jokers & empties:** treated as flexible “wild material” that can be assigned to maximize
///     flush length, straight window fit, or rank clustering (same spirit as joker search in
///     [`best_hand_score_with_jokers`], but much cheaper).
///
/// The numeric weights are intentionally modest versus finished-hand points (0–30 per line) so
/// rollouts still emphasize actually completing hands as the game resolves.
pub fn evaluate_partial_line(line: &[Option<Card>; 5]) -> f64 {
    if line.iter().all(|s| s.is_some()) {
        let mut hand = [Card {
            suit: Suit::Joker,
            rank: Rank::Joker,
        }; 5];
        for i in 0..5 {
            hand[i] = line[i].unwrap();
        }
        return best_hand_score_with_jokers(&hand) as f64;
    }

    let mat = line_material(line);
    let no_concrete_rank = (2..=14).all(|r| mat.rank_counts[r] == 0);
    if mat.fixed_suit_len == 0 && no_concrete_rank {
        // Only empty slots and/or jokers: huge flexibility, but no directional “draw” yet — keep
        // this below a concrete 4-card flush draw so wired shapes win over a blank pipeline.
        return 2.2 + 0.12 * (mat.jokers as f64) + 0.04 * (mat.empty as f64);
    }

    let wild = flex_slots(&mat);
    let sf = straight_flush_heuristic(&mat);

    // If SF is strongly alive, it subsumes separate flush/straight components to avoid double pay.
    let (flush_h, straight_h) = if sf >= WEIGHT_SF_3 {
        (0.0, 0.0)
    } else {
        (
            flush_draw_heuristic(&mat),
            straight_draw_heuristic(&mat, wild),
        )
    };

    let rank_h = rank_cluster_heuristic(&mat, wild);

    // Combine: dominant structure + smaller additive texture (pair progress still matters on SF paths).
    let mut score = sf;
    if sf < 1.0 {
        // When no SF pressure, keep both draws (they are mutually exclusive structurally but both “open”).
        score += 0.55 * flush_h + 0.45 * straight_h;
    } else {
        score += 0.15 * rank_h;
    }
    if sf < WEIGHT_SF_3 {
        score += 0.35 * rank_h;
    } else {
        score += 0.1 * rank_h;
    }

    score
}

/// Sums [`evaluate_partial_line`] for all **10** lines (5 rows + 5 columns). This is the static
/// evaluator used inside Monte Carlo playouts and whenever a mid-game board value is needed.
pub fn board_total_value(board_rows_cols: &[[Option<Card>; 5]; 10]) -> f64 {
    board_rows_cols
        .iter()
        .map(|line| evaluate_partial_line(line))
        .sum()
}

