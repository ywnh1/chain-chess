#!/usr/bin/env node
// PWA 引擎产物自检：直接加载 docs/pkg 里编译好的 wasm 跑真实规则。
//
// 为什么需要这个：PWA 跑的是 docs/pkg/chain_chess_engine_bg.wasm，而仓库里
// 同时有一份 Rust 源码。改引擎只跑 `cargo test` 是查不出问题的 —— 源码全绿，
// 网页端仍然用旧产物。用户报过「PWA 第一个子落下就判胜负」，根因就是
// 引擎修完没重编 wasm（build.sh 在找不到 wasm-pack 时也只打印一行警告就跳过）。
//
// 运行：node tauri/check_pwa_wasm.mjs
// 修了引擎记得先重编：cd docs/wasm && wasm-pack build --target web --out-dir ../pkg --release
import { readFileSync } from "fs";
import { resolve } from "path";
import { pathToFileURL } from "url";

const PKG = resolve(import.meta.dirname, "..", "docs", "pkg");
const errs = [];

let mod;
try {
  mod = await import(pathToFileURL(resolve(PKG, "chain_chess_engine.js")).href);
  await mod.default(readFileSync(resolve(PKG, "chain_chess_engine_bg.wasm")));
} catch (e) {
  console.error("加载 docs/pkg 的 wasm 失败：" + (e && e.message ? e.message : e));
  console.error("先在 docs/wasm 下跑 wasm-pack build --target web --out-dir ../pkg --release");
  process.exit(1);
}

const empty = (r, c) =>
  Array.from({ length: r }, () =>
    Array.from({ length: c }, () => ({ owner: null, count: 0, th: null, blocked: false }))
  );

/** 造一个「只有 0 号落了首子」的 7×7 局面 */
function firstMoveOnly() {
  const b = empty(7, 7);
  b[3][3] = { owner: 0, count: 3, th: null, blocked: false };
  return b;
}

function processMove(board, x, y, player, maxPlayers) {
  const r = JSON.parse(
    mod.process_move_cmd(
      JSON.stringify({
        board, size: 7, x, y, player, maxPlayers,
        borderMode: "default", capMode: "4",
        capStyle: "one-two", winCondition: "last",
        seed: 12345,
      })
    )
  );
  if (!r.ok) throw new Error("process_move 失败: " + r.error);
  return r.data;
}

/* 1. 开局第一手落地不是获胜：其他人只是还没落子，不是已被淘汰。
 * 这条曾经在 PWA 上导致「点一下第一颗子就直接结算」。 */
for (const mp of [2, 3, 4]) {
  // 0 号在自己首子上再落一手（首子已是临界态，这会炸开）
  const d = processMove(firstMoveOnly(), 3, 3, 0, mp);
  if (d.gameOver) errs.push(`${mp} 人局：第一手之后不该判胜，却给出 winner=${d.winner}`);
}

/* 2. 常规落子不能误判终局（防止为了修 1 而把判胜整个关掉） */
{
  const b = empty(7, 7);
  b[0][0] = { owner: 0, count: 1, th: null, blocked: false };
  b[6][6] = { owner: 1, count: 3, th: null, blocked: false };
  b[6][5] = { owner: 2, count: 3, th: null, blocked: false };
  const d = processMove(b, 0, 0, 0, 3);
  if (d.gameOver) errs.push(`3 人局常规落子被误判结束，winner=${d.winner}`);
}

/* 3. 一键终局必须等所有人落首子，不能把「只有一人有子」当打完 */
{
  const cfg = {};
  for (const p of [0, 1, 2]) cfg[p] = { algorithm: "strategy", depth: 1, randomScale: 0 };
  const r = JSON.parse(
    mod.simulate_to_end_cmd(
      JSON.stringify({
        board: firstMoveOnly(), size: 7, maxPlayers: 3, curPlayer: 0, eliminated: [],
        borderMode: "default", capMode: "4", capStyle: "one-two",
        winCondition: "last", firstMovePos: null, gameCount: 0, aiConfigs: cfg,
      })
    )
  );
  if (!r.ok) errs.push("simulate_to_end 失败: " + r.error);
  else {
    const acted = new Set(r.data.history.map((h) => h.mv && h.mv[2]));
    for (const p of [0, 1, 2]) {
      if (!acted.has(p)) errs.push(`一键终局：${p} 号没落首子，被当成没有这个玩家`);
    }
  }
}

if (errs.length) {
  console.log("ERRORS（多半是 docs/pkg 的 wasm 没重编）：");
  errs.forEach((e) => console.log(" - " + e));
  console.log("\n重编：cd docs/wasm && wasm-pack build --target web --out-dir ../pkg --release");
  process.exit(1);
}
console.log("ALL GOOD（docs/pkg 的 wasm 与当前规则一致）");
