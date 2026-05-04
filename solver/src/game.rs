use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Suit {
    Earth,
    Fire,
    Water,
    Lightning,
    Joker, // for transport; evaluator treats as wildcard
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rank {
    Ace,
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Ten,
    Jack,
    Queen,
    King,
    Joker,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Card {
    pub suit: Suit,
    pub rank: Rank,
}

impl Card {
    #[inline]
    pub fn is_joker(&self) -> bool {
        matches!(self.suit, Suit::Joker) || matches!(self.rank, Rank::Joker)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckState {
    /// Remaining cards in draw order (top at index 0).
    pub cards: Vec<Card>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardState {
    /// Length 25, row-major. `None` means empty.
    pub cells: Vec<Option<Card>>,
}

impl BoardState {
    pub const W: i32 = 5;
    pub const H: i32 = 5;

    pub fn new_empty() -> Self {
        Self {
            cells: vec![None; 25],
        }
    }

    #[inline]
    pub fn idx(x: i32, y: i32) -> usize {
        (y as usize) * 5 + (x as usize)
    }

    #[inline]
    pub fn in_bounds(x: i32, y: i32) -> bool {
        x >= 0 && x < 5 && y >= 0 && y < 5
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32) -> Option<Card> {
        if !Self::in_bounds(x, y) {
            return None;
        }
        self.cells[Self::idx(x, y)]
    }

    #[inline]
    pub fn set(&mut self, x: i32, y: i32, c: Option<Card>) {
        let i = Self::idx(x, y);
        self.cells[i] = c;
    }

    pub fn filled_count(&self) -> usize {
        self.cells.iter().filter(|c| c.is_some()).count()
    }

    pub fn valid_placements(&self) -> Vec<(i32, i32)> {
        let filled = self.filled_count();
        if filled == 0 {
            let mut all = Vec::with_capacity(25);
            for y in 0..5 {
                for x in 0..5 {
                    all.push((x, y));
                }
            }
            return all;
        }

        let mut v = Vec::new();
        for y in 0..5 {
            for x in 0..5 {
                if self.get(x, y).is_some() {
                    continue;
                }
                if self.has_adjacent_filled(x, y) {
                    v.push((x, y));
                }
            }
        }
        v
    }

    fn has_adjacent_filled(&self, x: i32, y: i32) -> bool {
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = x + dx;
                let ny = y + dy;
                if !Self::in_bounds(nx, ny) {
                    continue;
                }
                if self.get(nx, ny).is_some() {
                    return true;
                }
            }
        }
        false
    }

    pub fn row(&self, y: i32) -> [Option<Card>; 5] {
        let mut r = [None; 5];
        for x in 0..5 {
            r[x as usize] = self.get(x, y);
        }
        r
    }

    pub fn col(&self, x: i32) -> [Option<Card>; 5] {
        let mut c = [None; 5];
        for y in 0..5 {
            c[y as usize] = self.get(x, y);
        }
        c
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct SkillsState {
    /// Remaining uses (0..=2)
    pub keep_both: u8,
    /// Remaining uses (0..=2)
    pub discard_both: u8,
}

