import type { BoardState } from "./types";

const W = 5;
const H = 5;

function inBounds(x: number, y: number) {
  return x >= 0 && x < W && y >= 0 && y < H;
}

export function filledCount(board: BoardState): number {
  return board.cells.filter(Boolean).length;
}

export function get(board: BoardState, x: number, y: number) {
  return board.cells[y * W + x] ?? null;
}

export function validPlacements(board: BoardState): Array<[number, number]> {
  const filled = filledCount(board);
  const v: Array<[number, number]> = [];
  if (filled === 0) {
    for (let y = 0; y < H; y++) for (let x = 0; x < W; x++) v.push([x, y]);
    return v;
  }

  for (let y = 0; y < H; y++) {
    for (let x = 0; x < W; x++) {
      if (get(board, x, y)) continue;
      let ok = false;
      for (let dy = -1; dy <= 1 && !ok; dy++) {
        for (let dx = -1; dx <= 1 && !ok; dx++) {
          if (dx === 0 && dy === 0) continue;
          const nx = x + dx,
            ny = y + dy;
          if (!inBounds(nx, ny)) continue;
          if (get(board, nx, ny)) ok = true;
        }
      }
      if (ok) v.push([x, y]);
    }
  }
  return v;
}

