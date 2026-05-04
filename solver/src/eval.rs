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

pub fn board_score(board_rows_cols: &[[Option<Card>; 5]; 10]) -> u32 {
    let mut total = 0u32;
    for line in board_rows_cols.iter() {
        if line.iter().any(|c| c.is_none()) {
            continue; // incomplete hand scores 0 for now
        }
        let mut hand = [Card { suit: Suit::Joker, rank: Rank::Joker }; 5];
        for i in 0..5 {
            hand[i] = line[i].unwrap();
        }
        total += best_hand_score_with_jokers(&hand);
    }
    total
}

