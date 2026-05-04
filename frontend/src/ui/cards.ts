import type { Card, Rank, Suit } from "../solver/types";

const rankMap: Record<string, Rank> = {
  a: "ace",
  "2": "two",
  "3": "three",
  "4": "four",
  "5": "five",
  "6": "six",
  "7": "seven",
  "8": "eight",
  "9": "nine",
  t: "ten",
  j: "jack",
  q: "queen",
  k: "king",
};

const suitMap: Record<string, Suit> = {
  e: "earth",
  f: "fire",
  w: "water",
  l: "lightning",
};

/** Shown in errors and hints: capital rank + capital suit letter. */
export const DEAL_CARD_FORMAT_HINT =
  "Rank (A–9, 10, J, Q, K) + suit E Earth, F Fire, W Water, L Lightning — e.g. 10F, 9L, QW, AE, Joker.";

export function parseCard(input: string): Card | null {
  const t = input.trim().toLowerCase();
  if (!t) return null;
  if (t === "joker" || t === "jk" || t === "jo") return { suit: "joker", rank: "joker" };
  if (t.length < 2) return null;

  let rankKey: string;
  let suitChar: string;

  if (t.startsWith("10")) {
    if (t.length < 3) return null;
    rankKey = "10";
    suitChar = t[t.length - 1]!;
  } else {
    rankKey = t[0]!;
    suitChar = t[t.length - 1]!;
  }

  const rank = rankKey === "10" ? "ten" : rankMap[rankKey];
  const suit = suitMap[suitChar];
  if (!rank || !suit) return null;
  return { rank, suit };
}

/** Canonical display for a valid card (capital letters where rank/suit use letters). */
export function formatDealtCardInput(input: string): string {
  const c = parseCard(input);
  if (!c) return input.trim();
  return cardToInputShorthand(c);
}

function cardToInputShorthand(c: Card): string {
  if (c.rank === "joker" || c.suit === "joker") return "Joker";
  const rankPart =
    c.rank === "ace"
      ? "A"
      : c.rank === "two"
        ? "2"
        : c.rank === "three"
          ? "3"
          : c.rank === "four"
            ? "4"
            : c.rank === "five"
              ? "5"
              : c.rank === "six"
                ? "6"
                : c.rank === "seven"
                  ? "7"
                  : c.rank === "eight"
                    ? "8"
                    : c.rank === "nine"
                      ? "9"
                      : c.rank === "ten"
                        ? "10"
                        : c.rank === "jack"
                          ? "J"
                          : c.rank === "queen"
                            ? "Q"
                            : "K";
  const suitPart =
    c.suit === "earth" ? "E" : c.suit === "fire" ? "F" : c.suit === "water" ? "W" : "L";
  return `${rankPart}${suitPart}`;
}

export function cardLabel(c: Card | null): string {
  if (!c) return "";
  if (c.rank === "joker" || c.suit === "joker") return "JOKER";
  return cardToInputShorthand(c);
}

