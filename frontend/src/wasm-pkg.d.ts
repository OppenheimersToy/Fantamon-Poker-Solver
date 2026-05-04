declare module "../wasm-pkg/fantamon_solver.js" {
  const init: (module_or_path?: unknown) => Promise<void>;
  export default init;
  export function calculate_optimal_move(
    current_board_json: string,
    current_deck_json: string,
    drawn_cards_json: string,
    remaining_skills_json: string,
    simulation_depth: number,
  ): unknown;
}

