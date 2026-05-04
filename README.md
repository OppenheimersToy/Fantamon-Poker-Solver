# Fantamon Optimal Move Calculator

Monorepo with:

- `solver/`: Rust solver compiled to WebAssembly via `wasm-pack`
- `frontend/`: React + TypeScript UI (Vite) calling the solver via a Web Worker

## Prereqs

- Node.js 18+ (20+ recommended)
- Rust stable
- `wasm-pack` (`cargo install wasm-pack`)

## Build & run

### 1) Build the Wasm solver

```bash
cd solver
wasm-pack build --target web
```

This outputs a Wasm package under `solver/pkg/`.

### 2) Install & run the frontend

```bash
cd ../frontend
npm install
npm run dev
```

## Notes

- The solver API is exposed as `calculate_optimal_move(...)` and accepts JSON strings.
- The UI uses a worker (`src/solver/solverWorker.ts`) to avoid blocking React during computation.

