import type { Card, Rank, Suit } from "./types";

const suits: Suit[] = ["earth", "fire", "water", "lightning"];
const ranks: Rank[] = [
  "ace",
  "two",
  "three",
  "four",
  "five",
  "six",
  "seven",
  "eight",
  "nine",
  "ten",
  "jack",
  "queen",
  "king",
];

export function fullDeck54(): Card[] {
  const d: Card[] = [];
  for (const s of suits) for (const r of ranks) d.push({ suit: s, rank: r });
  d.push({ suit: "joker", rank: "joker" });
  d.push({ suit: "joker", rank: "joker" });
  return d;
}

export function cardKey(c: Card): string {
  return `${c.rank}:${c.suit}`;
}

export function removeKnownCards(deck: Card[], known: Card[]): Card[] {
  // For non-jokers, remove exact suit+rank occurrences.
  // For jokers, remove up to two.
  const counts = new Map<string, number>();
  for (const c of known) {
    const k = cardKey(c);
    counts.set(k, (counts.get(k) ?? 0) + 1);
  }
  const out: Card[] = [];
  for (const c of deck) {
    const k = cardKey(c);
    const n = counts.get(k) ?? 0;
    if (n > 0) {
      counts.set(k, n - 1);
      continue;
    }
    out.push(c);
  }
  return out;
}

export function shuffleInPlace<T>(arr: T[]): void {
  for (let i = arr.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [arr[i], arr[j]] = [arr[j], arr[i]];
  }
}

