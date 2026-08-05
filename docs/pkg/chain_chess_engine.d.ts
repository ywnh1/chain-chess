/* tslint:disable */
/* eslint-disable */

/**
 * AI 走棋命令（按 algorithm 分派：mcts / pvs / alphabeta / strategy）
 */
export function ai_move_cmd(json: string): string;

/**
 * 单局 AI 基准命令（与 Tauri 端 bench_ai_game 语义一致，供设备性能检测页使用）
 */
export function bench_ai_game_cmd(json: string): string;

export function engine_version(): string;

/**
 * process_move 命令
 */
export function process_move_cmd(json: string): string;

/**
 * 一键终局命令
 */
export function simulate_to_end_cmd(json: string): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly ai_move_cmd: (a: number, b: number, c: number) => void;
    readonly bench_ai_game_cmd: (a: number, b: number, c: number) => void;
    readonly engine_version: (a: number) => void;
    readonly process_move_cmd: (a: number, b: number, c: number) => void;
    readonly simulate_to_end_cmd: (a: number, b: number, c: number) => void;
    readonly __wbindgen_export: (a: number) => void;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export2: (a: number, b: number) => number;
    readonly __wbindgen_export3: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_export4: (a: number, b: number, c: number) => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
