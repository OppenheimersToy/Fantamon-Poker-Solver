# Fantamon Optimal Move Calculator

A web app that recommends moves for a **5×5 grid** card game: each **row** and **column** is scored as a 5-card poker hand when complete. The solver maximizes **expected total points** over those ten lines (with skills like **Keep Both** and **Discard Both**).

This repo is a small monorepo:

- **`solver/`** — Rust engine compiled to **WebAssembly** (`wasm-pack`)
- **`frontend/`** — **React + TypeScript** (Vite), calling WASM from a **Web Worker** so the UI stays responsive

---

## How it works (high level)

### Game model in the UI

1. You set the **board**, the two **dealt** cards, cards **not in the draw pile** (discards / burns), and **skill uses** remaining.
2. The app builds the **unknown draw pile** as: **full 54-card deck** minus every card already accounted for (board, current deal, and your exclude list). The **order** of that pile matters for simulations.

### Single solve — **Calculate Optimal Move**

- Runs **UCT** (Monte Carlo tree search with an upper-confidence bound) in Rust for the **current deal**.
- **`simulation_depth`** is the number of **UCT iterations** (total rollouts from the root), not “per candidate.”
- **`uct_max_depth`** (default **4** in the WASM request) caps how many **decision plies** expand in the tree; deeper nodes switch to a **fast rollout** (greedy placement policy + play out until the game ends or no full turn is possible).
- **Leaf / rollout value** uses a **static evaluator**: completed lines get real poker **hand scores**; incomplete lines get a **heuristic “potential”** (flush/straight shape, pairs, etc.) so the search has signal **before** rows/columns are full.

### Unknown pile order — **Calculate Optimal Move Averaged**

The real pile order is usually **unknown**. For each solve, the frontend **shuffles** the remaining deck with `Math.random()` before calling WASM, so two clicks are two **different** futures unless nothing changes.

**Averaged** runs up to **three passes** of **`N` shuffles** each (default **`N = 10`**):

1. **Pass 1:** stop early if the **winning recommended move** appears **strictly more than 4** times out of `N` (i.e. **≥ 5**).
2. Else **Pass 2:** fresh `N` shuffles; stop if the winner has **≥ 3** votes.
3. Else **Pass 3:** another `N` shuffles; stop if the winner has **≥ 2** votes.

If no bar is met, the **third** batch is still shown as the plurality result, marked **low agreement**. The UI shows the **consensus move** and **mean EV** from that **last** batch only, plus a short notice about how many samples agreed and which pass (1–3) succeeded.

### Worker and WASM

- `frontend/src/solver/solverWorker.ts` loads the WASM module and runs `calculate_optimal_move` / `calculate_optimal_move_with_progress`.
- JSON passed into Rust matches **`BoardState`**, **`DeckState`**, dealt cards, and **`SkillsState`** (see `solver/src/game.rs` and `solver/src/lib.rs`).

---

## Prereqs

- Node.js 18+ (20+ recommended)
- Rust stable
- `wasm-pack` (`cargo install wasm-pack`)

---

## Build & run

### 1) Build the Wasm solver

```bash
cd solver
wasm-pack build --target web
```

This writes the package under `solver/pkg/`.

### 2) Copy into the frontend (if you use the npm script)

From `frontend/`, `npm run wasm:copy` (see `package.json`) copies `solver/pkg` into `frontend/src/wasm-pkg/` for Vite imports.

### 3) Install & run the frontend

```bash
cd frontend
npm install
npm run dev
```

---

## API note

The solver entrypoint is **`calculate_optimal_move(...)`** (JSON strings for board, deck, dealt cards, skills, plus **`simulation_depth`**). Optional **`uct_max_depth`** can be added to the request struct when exposing more WASM parameters from JS.

---

## Layout

| Path | Role |
|------|------|
| `solver/src/eval.rs` | Poker scoring + **heuristic** partial-line evaluation |
| `solver/src/solver/` | **UCT**, rollouts, move enumeration |
| `frontend/src/ui/App.tsx` | Board, controls, single vs averaged flows |
| `frontend/src/solver/solverWorker.ts` | WASM bridge + **averaged** cascade |
