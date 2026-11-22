#!/usr/bin/env node
// 纯逻辑自检：从前台 app.js 抽出不依赖 DOM 的函数真跑一遍。
//
// 放在这里而不是测试框架里：项目没有测试框架，这些逻辑一旦坏掉
// 用户会在界面上直接看到（棋盘列表错乱、淘汰顺序颠倒）。
// 运行：node tauri/check_logic.mjs
import { readFileSync } from "fs";
import { resolve } from "path";

// 两份前端是独立维护的副本（环境层不同），逻辑必须各自成立
const SOURCES = [
  resolve(import.meta.dirname, "public/app.js"),
  resolve(import.meta.dirname, "..", "docs", "app.js"),
];
const errs = [];

async function checkFile(appJs, label) {
  const before = errs.length;

function grab(startMarker, endMarker) {
  const i = appJs.indexOf(startMarker);
  if (i < 0) throw new Error("找不到片段起点: " + startMarker);
  const j = appJs.indexOf(endMarker, i);
  if (j < 0) throw new Error("找不到片段终点: " + endMarker);
  return appJs.slice(i, j);
}

/* ═══════ 1. 自定义棋盘库 ═══════ */
{
  const lib = grab("const CUSTOM_BOARDS_MAX", "function updateBeInfo(){");
  const val = grab("function validateBeShape(", "function loadBeShape(");
  const area = grab("function getMaxPlayersByArea", "function getMaxPlayersBySize");

  const mk = () => {
    let appSettings = {};
    const box = {};
    const factory = new Function(
      "appSettingsHolder",
      `const BE_N=20;
       let appSettings=appSettingsHolder.obj;
       const saveSettings=()=>{appSettingsHolder.saves++;};
       ${area}
       ${val}
       ${lib}
       return {getCustomBoards,upsertCustomBoard,deleteCustomBoard,
               getActiveCustomBoard,shapeCellCount,
               setSettings:(o)=>{appSettings=o;appSettingsHolder.obj=o;},
               getSettings:()=>appSettings};`
    );
    const holder = { obj: appSettings, saves: 0 };
    return factory(holder);
  };

  const api = mk();
  const shapeA = { rows: 5, cols: 5, cells: ["11111", "11111", "11111", "11111", "11111"] };
  const shapeB = { rows: 3, cols: 3, cells: ["111", "101", "111"] };

  // 旧版单 shape 迁移
  api.setSettings({ customShape: shapeA });
  let list = api.getCustomBoards();
  if (list.length !== 1) errs.push("旧 customShape 应迁移成 1 条记录");
  if (list[0]?.name !== "我的棋盘") errs.push("迁移后的默认名应为「我的棋盘」");
  if (api.getSettings().activeCustomBoard !== "legacy") errs.push("迁移后应把旧棋盘设为当前");
  if (!Array.isArray(api.getSettings().customBoards)) errs.push("迁移结果应写成数组");

  // 新建
  const r1 = api.upsertCustomBoard(shapeB, "小棋盘", null);
  list = api.getCustomBoards();
  if (list.length !== 2) errs.push("新建后应有 2 条");
  if (list[0]?.name !== "小棋盘") errs.push("新条目应排在最前");
  if (api.getSettings().activeCustomBoard !== r1.id) errs.push("新建后应设为当前");
  if (api.shapeCellCount(list[0]) !== 8) errs.push("可用格数应算成 8");

  // 编辑已有：不新增
  const r2 = api.upsertCustomBoard({ rows: 2, cols: 2, cells: ["11", "11"] }, "改过名", r1.id);
  list = api.getCustomBoards();
  if (list.length !== 2) errs.push("编辑已有棋盘不应新增条目");
  if (r2.id !== r1.id) errs.push("编辑后 id 应保持不变");
  const edited = list.find((b) => b.id === r1.id);
  if (edited?.name !== "改过名" || edited?.rows !== 2) errs.push("编辑应更新名字与尺寸");

  // 删除当前
  api.deleteCustomBoard(r1.id);
  list = api.getCustomBoards();
  if (list.length !== 1) errs.push("删除后应剩 1 条");
  if (api.getSettings().activeCustomBoard !== list[0]?.id) errs.push("删除当前项后应自动切到剩下那条");

  // 删空后不被旧字段反复迁移
  api.deleteCustomBoard(list[0].id);
  if (api.getCustomBoards().length !== 0) errs.push("删空后列表应为空");
  if (!Array.isArray(api.getSettings().customBoards)) errs.push("删空后仍应是空数组");
  if (api.getActiveCustomBoard() !== null) errs.push("删空后当前棋盘应为 null");

  // 坏数据跳过
  api.setSettings({
    customBoards: [
      { id: "a", name: "好的", rows: 2, cols: 2, cells: ["11", "11"] },
      { id: "b", name: "行数对不上", rows: 3, cols: 2, cells: ["11", "11"] },
      { id: "c", name: "非法字符", rows: 1, cols: 2, cells: ["1x"] },
      null,
    ],
  });
  list = api.getCustomBoards();
  if (list.length !== 1 || list[0].id !== "a") errs.push("应只保留合法条目");

  // 名字长度
  api.setSettings({});
  api.upsertCustomBoard(shapeA, "这个名字实在是太长了超过十二个字", null);
  if (api.getCustomBoards()[0].name.length > 12) errs.push("名字应截到 12 字以内");
}

/* ═══════ 2. 淘汰顺序 ═══════ */
{
  const fn = grab("function buildEliminationSeq(", "function renderRankingList");
  const buildEliminationSeq = new Function(fn + "; return buildEliminationSeq;")();

  // 按 renderRankingList 的比较器排序：序号大的（淘汰晚）在前
  const order = (set, rounds) => {
    const seq = buildEliminationSeq(set, rounds);
    return [...set].sort((a, b) => {
      const sa = seq.has(a) ? seq.get(a) : 1e9;
      const sb = seq.has(b) ? seq.get(b) : 1e9;
      return sb - sa;
    });
  };
  const eq = (a, b) => JSON.stringify(a) === JSON.stringify(b);

  if (!eq(order(new Set([1, 2]), [[1], [2]]), [2, 1]))
    errs.push("逐轮淘汰：后淘汰的应排在前面");
  if (!eq(order(new Set([2, 3, 4]), [[2, 3, 4]]), [4, 3, 2]))
    errs.push("同一轮多人：应按轮内顺序，后者在前");
  // 一键终局会把所有淘汰者塞进一个轮次，旧实现只比轮次下标 → 全并列、
  // 退化成按玩家编号排。这条专门盯它复活。
  if (!eq(order(new Set([3, 1, 2]), [[3, 1, 2]]), [2, 1, 3]))
    errs.push("一键终局形态：不应退化成玩家编号顺序");
  if (!eq(order(new Set([3, 1]), null), [1, 3]))
    errs.push("没有轮次信息时应按 Set 插入顺序（后插入 = 后淘汰）");
  // 回放时 Set 是按玩家编号构造的，真顺序只在记录里
  if (!eq(order(new Set([0, 1, 2]), [[2], [0], [1]]), [1, 0, 2]))
    errs.push("回放应以记录里的淘汰顺序为准");
  if (!eq(order(new Set([1, 2, 3]), [[1], [3]]), [2, 3, 1]))
    errs.push("轮次里缺人时缺失者应视为最后淘汰");
  if (!eq(order(new Set([1, 2]), [[1], [1], [2]]), [2, 1]))
    errs.push("同一人重复出现应只认第一次");
  if (buildEliminationSeq(new Set([1]), "oops").get(1) !== 0)
    errs.push("rounds 非数组时应退化为 Set 顺序");
  if (buildEliminationSeq(new Set([1]), [null, [1]]).get(1) !== 0)
    errs.push("轮次里含 null 时应跳过");
}

/* ═══════ 4.1.0：最具价值的一步（ML 评分曲线上的最大正增长） ═══════ */
{
  const fn = grab("function findBestMoves(", "async function flushOverflowHistory(");
  const best = new Function(fn + "; return findBestMoves;")();

  // 构造 3 步历史：0 号在第 2 步评分大涨，1 号全程下降
  const hist = [
    { rating: [10, 10] },
    { rating: [12, 9] },
    { rating: [25, 8], mv: { x: 1, y: 2 } },
  ];
  const b = best(hist, 2);
  // 「一步」= 相邻两步之差：第 2 步 25-12=13 才是单步最大
  if (!b[0] || b[0].delta !== 13) errs.push("MVP：0 号应取 +13 那一步，实际 " + JSON.stringify(b[0]));
  if (b[0] && b[0].turn !== 2) errs.push("MVP：应记录第 2 步为最佳");
  if (b[0] && (!b[0].mv || b[0].mv.x !== 1)) errs.push("MVP：应带上那一步的落子坐标");
  if (b[1]) errs.push("MVP：1 号全程下跌，不该有「最具价值的一步」");

  // 缺 rating 的步骤要跳过，不能当成 0 算（否则会出现 +999 的假峰值）
  const partial = [{ }, { rating: [5, 5] }, { rating: [6, 6] }];
  const pb = best(partial, 2);
  if (!pb[0] || pb[0].delta !== 1) errs.push("MVP：缺失评分的步应跳过，实际 " + JSON.stringify(pb[0]));
}

/* ═══════ 局内排名：ML 评分必须真的回填到玩家条 ═══════
 *
 * 用户报过：选了「局内排名依据 = ML 评分」，玩家条上显示的却一直是棋子数。
 * 根因是 scheduleRating 算完评分后忘了写回 _liveRating —— 排序函数读到
 * null 就降级成棋子数，功能等于没接上。这条测试盯住那次回填。
 */
{
  const schFn = grab("function scheduleRating(", "/// 从历史里找每位玩家评分正变化最大的一步");
  const code = [
    "let _ratingBusy = false;",
    "const _ratingPending = [];",
    "let _ratingGen = 0;",
    "let _liveRating = null;",
    "let gameHistory = [{ turn: 0 }, { turn: 1 }];",
    "const board = []; const maxPlayers = 2;",
    "const borderMode = 'default'; const capMode = '4';",
    "let rendered = 0;",
    "const renderPlayerBar = () => { rendered++; };",
    "const holder = { invoke: null };",
    "const tauriInvoke = (cmd, args) => holder.invoke(cmd, args);",
    schFn,
    "return {",
    "  scheduleRating,",
    "  get live() { return _liveRating; },",
    "  get rendered() { return rendered; },",
    "  setInvoke(f) { holder.invoke = f; },",
    "};",
  ].join("\n");

  const api = new Function(code)();
  let called = 0;
  api.setInvoke((cmd, args) => {
    if (cmd !== "rate_board") throw new Error("调用了意外命令: " + cmd);
    called++;
    if (args.maxPlayers !== 2) errs.push("评分：maxPlayers 没传对");
    return Promise.resolve([42, -7]);
  });

  api.scheduleRating();
  // 等微任务跑完
  await new Promise((r) => setTimeout(r, 30));

  if (called !== 1) errs.push("评分：应调用一次 rate_board，实际 " + called);
  if (!api.live) errs.push("评分：算完必须写回 _liveRating（否则排名退回棋子数）");
  if (api.live && (api.live[0] !== 42 || api.live[1] !== -7))
    errs.push("评分：_liveRating 内容不对 " + JSON.stringify(api.live));
  if (api.rendered < 1) errs.push("评分：算完应重绘玩家条，实际重绘 " + api.rendered + " 次");

  // 连续落子：推理比落子慢时请求必须排队，不能丢（丢了曲线就断在半路）
  {
    const api3 = new Function(code)();
    let n3 = 0;
    api3.setInvoke(() => { n3++; return Promise.resolve([1, 2]); });
    api3.scheduleRating();
    api3.scheduleRating();
    api3.scheduleRating();
    if (n3 > 1) errs.push("评分：同一时刻只应有一个请求在飞");
    await new Promise((r) => setTimeout(r, 60));
    if (n3 !== 3) errs.push("评分：连续三步应全部排队执行，实际 " + n3);
  }

  // 评分失败时必须静默降级，不能把上一次的分留着误导用户，也不能抛
  const api2 = new Function(code)();
  api2.setInvoke(() => Promise.reject(new Error("命令不存在")));
  api2.scheduleRating();
  await new Promise((r) => setTimeout(r, 30));
  if (api2.live) errs.push("评分：失败时不该写入 _liveRating");
}

/* ═══════ 闪回防护：引擎有结果时不得再跑 JS 模拟 ═══════
 *
 * 用户报过「6 级（以及其他阈值）闪一下爆炸效果又恢复」。根因是前端在
 * 引擎结果之外自己又判了一遍炸，判错就凭空播动画，随后被引擎棋盘覆盖。
 * 这里做结构断言：processClick 里「有 rustResult 但无快照」的分支必须在
 * JS 模拟的 while 之前 return，不能落进模拟。
 */
{
  const src = grab("async function processClick(", "async function localClick(");
  if (!/if\(rustResult && finalBoard\)\{/.test(src)) {
    errs.push("闪回防护：缺少「引擎有结果但无爆炸步骤」的提前返回分支");
  }
  const iGuard = src.indexOf("if(rustResult && finalBoard){");
  const iLoop = src.indexOf("while(chain.length)");
  if (iGuard < 0 || iLoop < 0 || iGuard > iLoop) {
    errs.push("闪回防护：提前返回分支必须在 JS 模拟循环之前");
  }
  // 该分支内必须真的 return，否则只是白算一遍
  if (iGuard >= 0) {
    const seg = src.slice(iGuard, iLoop > iGuard ? iLoop : iGuard + 800);
    if (!/return/.test(seg)) errs.push("闪回防护：该分支内没有 return，仍会落到 JS 模拟");
  }

  /* ─── 反向保护：短路分支不能吞掉该有的动画 ───
   * 短路只在「引擎说没爆」时生效。只要有爆炸，引擎一定返回 steps
   * （engine 那条 explosion_always_emits_snapshot 测试盯着），
   * 前端就必须走上面的快照动画分支。这里做结构断言，防止有人
   * 把短路挪到快照分支之前 —— 那会让所有爆炸动画消失。
   */
  const iSnap = src.indexOf("if(snapSteps){");
  if (iSnap < 0) {
    errs.push("动画：找不到快照动画分支（if(snapSteps)）");
  } else if (iGuard >= 0 && iSnap > iGuard) {
    errs.push("动画：快照分支必须在短路分支之前，否则爆炸动画会全部消失");
  }
  // 快照分支内必须真的有播特效的调用
  if (iSnap >= 0) {
    const seg = src.slice(iSnap, iGuard > iSnap ? iGuard : iSnap + 3000);
    if (!/playExplosion\(\)/.test(seg)) {
      errs.push("动画：快照分支内没有 playExplosion()，爆炸会变成静默跳变");
    }
    if (!/addShockwave|addParticles/.test(seg)) {
      errs.push("动画：快照分支内缺冲击波/粒子效果");
    }
  }
}

/* ═══════ 动画行为：有快照就必须播特效，没快照就不播 ═══════
 *
 * 这条不是结构断言，是真的把 processClick 跑起来数调用次数。
 * 用户问过「加了短路之后动画会不会消失」—— 这里用实际行为回答。
 */
{
  // 只取 processClick 本体：grab 到 localClick 会把中间十几个函数一起
  // 拖进来，重复声明会直接语法报错。这里按花括号配平精确截取。
  const fnStart = appJs.indexOf("async function processClick(");
  let fnEnd = -1, depth = 0, started = false;
  for (let k = fnStart; k < appJs.length; k++) {
    const ch = appJs[k];
    if (ch === "{") { depth++; started = true; }
    else if (ch === "}") {
      depth--;
      if (started && depth === 0) { fnEnd = k + 1; break; }
    }
  }
  if (fnStart < 0 || fnEnd < 0) throw new Error("找不到 processClick");
  const fn = appJs.slice(fnStart, fnEnd);
  const mkSandbox = (rustResult) => {
    const calls = { explosion: 0, shockwave: 0, particles: 0, render: 0 };
    const code = [
      "let animating = false;",
      "let chainSkipAll = false, autoSkipChain = false;",
      "let borderMode = 'default', capMode = '4', capStyle = 'one-two';",
      "let size = 5, cols = 5, curPlayer = 0;",
      "let cells = [];",
      "let chainStats = {};",
      "let maxChainOverall = { player: null, length: 0 };",
      "let firstMovePos = null;",
      "const COLOR_NAMES = ['红色','黄色'];",
      "const showSkipBtn = () => {};",
      "const renderBoard = () => { env.calls.render++; };",
      "const playExplosion = () => { env.calls.explosion++; };",
      "const addShockwave = () => { env.calls.shockwave++; };",
      "const addParticles = () => { env.calls.particles++; };",
      "const sleep = () => Promise.resolve();",
      "const hasPieces = (p, b) => b.some(r => r.some(c => c.owner === p));",
      "const capForMode = () => 4;",
      "const nbrsForMode = () => [];",
      fn,
      "return processClick;",
    ].join("\n");
    const env = { calls };
    return { fn: new Function("env", code)(env), calls };
  };

  // 构造 3x3 棋盘，中心一颗 4 点棋子（必炸）
  const board = () => {
    const b = [];
    for (let i = 0; i < 5; i++) {
      const row = [];
      for (let j = 0; j < 5; j++) row.push({ owner: null, count: 0, blocked: false });
      b.push(row);
    }
    b[2][2].owner = 0; b[2][2].count = 4;
    return b;
  };

  // ① 引擎给了 2 步快照 → 必须播 2 次爆炸特效
  {
    const snapBoard = board(); snapBoard[2][2] = { owner: null, count: 0, blocked: false };
    const snapBoard2 = JSON.parse(JSON.stringify(snapBoard));
    const { fn: pc, calls } = mkSandbox(null);
    await pc(board(), 5, 2, 2, 0, "explode", "#f00", {
      board: snapBoard2,
      steps: [snapBoard, snapBoard2],
      exploded: [[2, 2], [1, 2]],
      eliminated: [],
      chainCount: 2,
    });
    // playExplosion 是「这一格炸了」的核心信号；冲击波/粒子依赖真实 DOM
    // 元素（cells[x][y]），无 DOM 环境下拿不到 el，所以只验证前者。
    if (calls.explosion !== 2) {
      errs.push("动画：引擎给了 2 步快照，实际播了 " + calls.explosion + " 次爆炸（应为 2）");
    }
  }

  // ② 引擎说没爆（steps 为空）→ 不该播任何特效
  {
    const after = board(); after[2][2].count = 1;   // 点击后只是 +1，不炸
    const { fn: pc, calls } = mkSandbox(null);
    await pc(board(), 5, 2, 2, 0, "explode", "#f00", {
      board: after, steps: [], exploded: [], eliminated: [], chainCount: 0,
    });
    if (calls.explosion !== 0) {
      errs.push("动画：引擎说没爆炸，却播了 " + calls.explosion + " 次特效");
    }
  }

  // ③ 引擎不可用（无 rustResult）→ 走 JS 模拟，仍应能播动画
  {
    const { fn: pc, calls } = mkSandbox(null);
    // 空 targets 的模拟：只验证它不会崩、且走了模拟路径
    await pc(board(), 5, 2, 2, 0, "explode", "#f00", null);
    if (calls.render < 1) errs.push("动画：无引擎结果时也应渲染棋盘");
  }
}

/* ═══════ 爆炸阈值：前端模拟必须与引擎逐条对齐 ═══════
 *
 * 用户报过：选 4 以外的阈值时，不该炸的时候 UI 闪一下爆炸效果、
 * 很快又恢复正常；该炸的时候反而没动画。根因是 capForMode 漏了 6 级，
 * random 也只到 5 —— JS 侧按 4 点判炸，与引擎各算各的。
 */
{
  const fn = grab("function capForMode(", "function nbrsForMode(");
  const mk = (capMode, borderMode) => new Function(
    "let capMode = " + JSON.stringify(capMode) + ";\n" +
    "let borderMode = " + JSON.stringify(borderMode) + ";\n" +
    fn + "\nreturn capForMode;"
  )();

  // 6 级已移除（棋子只画 4 个点，5/6 点视觉无法区分）
  const cases = [
    ["3", "default", 3], ["4", "default", 4], ["5", "default", 5],
  ];
  for (const [cap, bm, want] of cases) {
    const got = mk(cap, bm)(2, 2, 7, bm, null);
    if (got !== want) errs.push("阈值：" + cap + " 级应返回 " + want + "，实际 " + got);
  }

  // random 必须能抽到 3/4/5 全部三种，且不能出现 6（已移除）
  {
    const f = mk("random", "default");
    const seen = new Set();
    for (let i = 0; i < 400; i++) seen.add(f(2, 2, 7, "default", null));
    for (const v of [3, 4, 5]) {
      if (!seen.has(v)) errs.push("阈值：random 抽不到 " + v + "，实际见过 " + [...seen].sort());
    }
    if (seen.has(6)) errs.push("阈值：random 不该再抽到 6 级，实际 " + [...seen].sort());
    if (seen.has(7) || seen.has(2)) errs.push("阈值：random 越界 " + [...seen].sort());
  }

  // 只有 4 级 + 降级边界才按位置降级（引擎语义）
  {
    const f = mk("4", "degrade");
    const corner = f(0, 0, 7, "degrade", null);
    const edge = f(0, 3, 7, "degrade", null);
    const inner = f(3, 3, 7, "degrade", null);
    if (corner !== 2) errs.push("阈值：4 级 + 降级，角落应为 2，实际 " + corner);
    if (edge !== 3) errs.push("阈值：4 级 + 降级，边缘应为 3，实际 " + edge);
    if (inner !== 4) errs.push("阈值：4 级 + 降级，内部应为 4，实际 " + inner);
  }
  // 5 级同样不受降级边界影响（引擎里只有 Cap4 才降到 3/2）
  {
    const f = mk("5", "degrade");
    if (f(0, 0, 7, "degrade", null) !== 5)
      errs.push("阈值：5 级 + 降级不应被降级，实际 " + f(0, 0, 7, "degrade", null));
  }
  // 6 级已移除：若仍有代码传 '6'，不该被当成 6（前端会先规范化成 5）
  {
    const f = mk("6", "default");
    const got = f(2, 2, 7, "default", null);
    if (got === 6) errs.push("阈值：6 级应已移除，但 capForMode 仍返回 6");
  }
}

/* ═══════ 局内排名：玩家条按所选指标动态排序 ═══════ */
{
  const fn = grab("function playerBarRanking(", "function renderPlayerBar(");
  const mk = (board, maxPlayers, metric, liveRating) => {
    const code =
      "let playerBarMetric = " + JSON.stringify(metric) + ";\n" +
      "let _liveRating = " + JSON.stringify(liveRating) + ";\n" +
      "let board = " + JSON.stringify(board) + ";\n" +
      "let maxPlayers = " + maxPlayers + ";\n" +
      fn + "\nreturn playerBarRanking();";
    return new Function(code)();
  };

  // 3 人：0 号 2 子 5 点，1 号 4 子 4 点，2 号 1 子 6 点
  const cell = (owner, count) => ({ owner: owner, count: count });
  const board = [
    [cell(0, 3), cell(0, 2), cell(1, 1), cell(1, 1)],
    [cell(1, 1), cell(1, 1), cell(2, 6), { owner: null, count: 0 }],
  ];

  // 棋子数：1 号(4) > 0 号(2) > 2 号(1)
  const byPieces = mk(board, 3, "pieces", null).order;
  if (JSON.stringify(byPieces) !== JSON.stringify([1, 0, 2]))
    errs.push("排名/棋子数：应为 [1,0,2]，实际 " + JSON.stringify(byPieces));

  // 点数：2 号(6) > 0 号(5) > 1 号(4)
  const byPoints = mk(board, 3, "points", null).order;
  if (JSON.stringify(byPoints) !== JSON.stringify([2, 0, 1]))
    errs.push("排名/点数：应为 [2,0,1]，实际 " + JSON.stringify(byPoints));

  // ML：按给的分排
  const byMl = mk(board, 3, "ml", [-5, 30, 12]).order;
  if (JSON.stringify(byMl) !== JSON.stringify([1, 2, 0]))
    errs.push("排名/ML：应为 [1,2,0]，实际 " + JSON.stringify(byMl));

  // 默认就是 ML（用户明确要求默认走 ML，而不是棋子数）
  const defaultOrder = mk(board, 3, "ml", [-5, 30, 12]).order;
  if (JSON.stringify(defaultOrder) !== JSON.stringify([1, 2, 0]))
    errs.push("排名：默认应为 ML 排序");

  // ML 评分还没算出来（null）→ 退回棋子数，不能崩也不能乱排
  const fallback = mk(board, 3, "ml", null).order;
  if (JSON.stringify(fallback) !== JSON.stringify([1, 0, 2]))
    errs.push("排名：评分缺失时应退回棋子数，实际 " + JSON.stringify(fallback));

  // 每轮都要重排：同一份代码，局面变了顺序就得跟着变。
  // 用户报过「选了 ML/点数却一直按棋子数排」—— 除了评分没回填，
  // 也可能出现「只排一次就不动了」，所以这里用连续两个局面验证。
  {
    const posA = [[cell(0, 1), cell(0, 1), cell(1, 1)]];
    const posB = [[cell(0, 1), cell(1, 1), cell(1, 1)]];
    for (const metric of ["pieces", "points", "ml"]) {
      const ratingA = metric === "ml" ? [30, -5] : null;
      const ratingB = metric === "ml" ? [-5, 30] : null;
      const a = mk(posA, 2, metric, ratingA).order;
      const b = mk(posB, 2, metric, ratingB).order;
      if (JSON.stringify(a) === JSON.stringify(b)) {
        errs.push("排名：局面变化后 " + metric + " 的顺序没跟着变（" +
          JSON.stringify(a) + " → " + JSON.stringify(b) + "）");
      }
    }
  }

  // 已淘汰的人沉底，即使他的 ML 分更高。
  // 构造：0 号在棋盘上没有任何棋子（已被清空），1 号有。
  const withElim = [
    [{ owner: null, count: 0 }, cell(1, 1)],
  ];
  const elimOrder = mk(withElim, 2, "ml", [999, -999]).order;
  if (elimOrder[0] !== 1)
    errs.push("排名：已淘汰者（0 号无棋子）应沉底，实际 " + JSON.stringify(elimOrder));
}

/* ═══════ 4.1.0：页面返回逻辑（侧滑与 ← 按钮都要原路返回） ═══════ */
{
  // ① 二级页必须用函数式 back，而不是写死目标。
  //    about-ai / about-benchmark 从「更多」进入，写死 'about' 会回错页。
  const mustBeDynamic = [
    "about-ai", "about-benchmark", "about-license", "about-thirdparty",
    "about-rules", "game-config", "about-changelog", "about",
  ];
  for (const page of mustBeDynamic) {
    const i = appJs.indexOf("Router.register('" + page + "'");
    if (i < 0) { errs.push("返回逻辑：找不到页面注册 " + page); continue; }
    const seg = appJs.slice(i, appJs.indexOf("});", i));
    if (!/back\(\)\s*\{/.test(seg)) {
      errs.push("返回逻辑：" + page + " 的 back 必须写成函数（原路返回），不能写死");
    }
    if (!/Router\.getPrev\(\)/.test(seg)) {
      errs.push("返回逻辑：" + page + " 的 back 应基于 Router.getPrev()");
    }
  }

  // ② 侧滑表不能再硬编码二级页目标（曾经把 about-ai 钉成 'about'）
  if (/SWIPE_TABLE/.test(appJs)) {
    errs.push("返回逻辑：侧滑表应改为 SWIPE_SPECIAL + _resolveBack，不能保留写死的 SWIPE_TABLE");
  }
  for (const page of ["about-ai", "about-benchmark", "about-license", "about-thirdparty"]) {
    const re = new RegExp("'" + page + "'\\s*:\\s*\\(\\)\\s*=>\\s*Router\\.switchPage");
    if (re.test(appJs)) errs.push("返回逻辑：侧滑仍把 " + page + " 硬编码到别的页面");
  }

  // ③ _resolveBack 的语义：有 getPrev 用它，没有就退回 fallback
  const routerSrc = grab("const Router = {", "// ─── 页面注册 ───");
  const Router = new Function(
    "history", "document", "location",
    routerSrc + "; return Router;"
  )(
    { pushState() {} },
    { querySelectorAll: () => [], getElementById: () => null, activeElement: null },
    { hash: "" }
  );

  Router.register("settings", { back: "welcome" });
  Router.register("about-ai", { back() { return Router.getPrev() || "about"; } });
  Router.register("orphan", {});

  // 从 settings 进 about-ai → 返回 settings
  Router._prev = "settings";
  if (Router._resolveBack("about-ai") !== "settings")
    errs.push("返回逻辑：从 settings 进 about-ai，返回应回 settings，实际 " +
      Router._resolveBack("about-ai"));

  // 没有上一页（深链进来）→ 退回 fallback
  Router._prev = null;
  if (Router._resolveBack("about-ai") !== "about")
    errs.push("返回逻辑：无上一页时应退回 about，实际 " + Router._resolveBack("about-ai"));

  // 静态字符串 back 照常可用
  Router._prev = "whatever";
  if (Router._resolveBack("settings") !== "welcome")
    errs.push("返回逻辑：静态 back 应生效");

  // 没有 back 声明的页面 → welcome
  if (Router._resolveBack("orphan") !== "welcome")
    errs.push("返回逻辑：无 back 声明应退回 welcome");
}

/* ═══════ 游戏配置：改动立即生效，不必先点「保存」 ═══════
 *
 * 用户要求：改了规则就该马上作用于当前这一局；「保存」只负责把它
 * 存成可复用的预设。所以界面上必须有一份「草稿」，它优先于预设，
 * 并且在进出配置页时不被预设覆盖回去。
 */
{
  const def = grab("const GAME_CONFIG_DEFAULT", "function getGameConfigs(");
  const getFn = grab("function getGameConfigs(", "function saveGameConfigs(");
  const activeFn = grab("function getActiveGameConfig(", "/// 规则草稿：界面上的当前选择。");
  const draftFn = grab("/// 规则草稿：界面上的当前选择。", "/// 把某个组的选中态设成 value");
  const dirtyFn = grab("/// 草稿是否与当前预设不同", "function updateGameConfigSummary(");

  const code =
    "let appSettings = holder.obj;\n" +
    "const saveSettings = () => {};\n" +
    "const document = { getElementById: () => ({}) };\n" +   // captureGameConfigDraft 首行会探测
    "const readGameConfigFromDom = () => holder.domValue;\n" +
    "const updateGameConfigSummary = () => {};\n" +
    def + getFn + activeFn + draftFn + dirtyFn +
    "return { getGameConfigDraft, captureGameConfigDraft, isGameConfigDirty, setApp:(o)=>{appSettings=o;holder.obj=o;} };";
  const holder = { obj: {}, domValue: null };
  const api = new Function("holder", code)(holder);

  // ① 没有草稿时，退回当前预设
  api.setApp({ gameConfigs: [{ id: "default", name: "标准", capMode: "4" }], activeGameConfig: "default" });
  let d = api.getGameConfigDraft();
  if (d.capMode !== "4") errs.push("草稿：无草稿时应退回当前预设，实际 " + d.capMode);

  // ② 有草稿时，草稿优先（这就是「改了立即生效」）
  api.setApp({
    gameConfigs: [{ id: "default", name: "标准", capMode: "4" }],
    activeGameConfig: "default",
    gameConfigDraft: { borderMode: "wrap", capMode: "5", capStyle: "two-one", winCondition: "points", rankMetric: "points" },
  });
  d = api.getGameConfigDraft();
  if (d.capMode !== "5") errs.push("草稿：应优先于预设，实际 " + d.capMode);
  if (d.rankMetric !== "points") errs.push("草稿：排名依据应来自草稿，实际 " + d.rankMetric);

  // ③ 草稿缺字段时补默认值，不能出现 undefined
  api.setApp({ gameConfigs: [{ id: "default", name: "标准" }], activeGameConfig: "default", gameConfigDraft: {} });
  d = api.getGameConfigDraft();
  if (d.winCondition !== "last") errs.push("草稿：缺失字段应补默认 wins=last，实际 " + d.winCondition);

  // ④ 脏标记：草稿与预设不同 → true；相同 → false
  api.setApp({
    gameConfigs: [{ id: "default", name: "标准", capMode: "4", borderMode: "default",
                    capStyle: "one-two", winCondition: "last", rankMetric: "ml" }],
    activeGameConfig: "default",
    gameConfigDraft: { borderMode: "default", capMode: "5", capStyle: "one-two",
                       winCondition: "last", rankMetric: "ml" },
  });
  if (!api.isGameConfigDirty()) errs.push("草稿：capMode 变了却没标成未保存");
  api.setApp({
    gameConfigs: [{ id: "default", name: "标准", capMode: "4", borderMode: "default",
                    capStyle: "one-two", winCondition: "last", rankMetric: "ml" }],
    activeGameConfig: "default",
    gameConfigDraft: { borderMode: "default", capMode: "4", capStyle: "one-two",
                       winCondition: "last", rankMetric: "ml" },
  });
  if (api.isGameConfigDirty()) errs.push("草稿：与预设一致时不该标未保存");
}

/* ═══════ 4.1.0：评分随历史压缩/展开往返不丢 ═══════ */
{
  const comp = grab("function compactHistory(", "function expandHistory(");
  const exp = grab("function expandHistory(", "// Save game history");
  const api = new Function(comp + exp + "; return {compactHistory, expandHistory};")();

  const hist = [
    { snapshot: { "0": { pieces: 1, points: 1 }, "1": { pieces: 1, points: 1 } },
      mv: { x: 0, y: 0, player: 0, seed: 7 }, rating: [5, 3] },
    { snapshot: { "0": { pieces: 2, points: 2 }, "1": { pieces: 1, points: 1 } },
      mv: { x: 1, y: 0, player: 1, seed: 8 }, rating: [7, -1] },
  ];
  const c = api.compactHistory(hist, 2);
  if (!c.r) errs.push("评分：压缩后应保留评分数组");
  const e = api.expandHistory(c, 2);
  if (!e[0].rating || e[0].rating[0] !== 5) errs.push("评分：展开后第 0 步 0 号应为 5，实际 " + JSON.stringify(e[0].rating));
  if (!e[1].rating || e[1].rating[1] !== -1) errs.push("评分：负数评分不应丢失");

  // 没有评分的旧记录：不能凭空造出评分（否则曲线会出现假 0 值平台）
  const noRate = [{ snapshot: { "0": { pieces: 1, points: 1 } }, mv: null }];
  const c2 = api.compactHistory(noRate, 2);
  if (c2.r) errs.push("评分：无评分时不该写入 r 字段");
  const e2 = api.expandHistory(c2, 2);
  if (e2[0].rating) errs.push("评分：无评分的历史不该被展开出 rating");
}

/* ═══════ 4.1.0：游戏配置预设 ═══════ */
{
  const def = grab("const GAME_CONFIG_DEFAULT", "function getGameConfigs(");
  const getFn = grab("function getGameConfigs(", "function saveGameConfigs(");
  const activeFn = grab("function getActiveGameConfig(", "/// 把某个组的选中态设成 value");
  const code =
    "let appSettings = holder.obj;\n" +
    "const saveSettings = () => { holder.saved++; };\n" +
    def + getFn + activeFn +
    "return {getGameConfigs, getActiveGameConfig, setApp:(o)=>{appSettings=o;holder.obj=o;}};";
  const holder = { obj: {}, saved: 0 };
  const api = new Function("holder", code)(holder);

  // 空数据：必须自动补一个内置「标准」配置
  api.setApp({});
  const list0 = api.getGameConfigs();
  if (list0.length !== 1) errs.push("预设：空数据应自动补 1 个内置配置");
  if (list0[0].id !== "default") errs.push("预设：内置配置 id 必须是 default");
  if (list0[0].capMode !== "4" || list0[0].capStyle !== "one-two" || list0[0].winCondition !== "last")
    errs.push("预设：内置配置默认值不对 " + JSON.stringify(list0[0]));

  // 用户删掉 default 之后，读取时仍要能拿到内置配置（不能崩）
  api.setApp({ gameConfigs: [{ id: "x", name: "我的", capMode: "6" }] });
  const list1 = api.getGameConfigs();
  if (!list1.some((c) => c.id === "default")) errs.push("预设：default 缺失时应补回来");
  if (!list1.some((c) => c.id === "x")) errs.push("预设：用户配置不该被丢掉");

  // 老配置缺字段：要补齐成默认值，而不是读到 undefined
  api.setApp({ gameConfigs: [{ id: "y", name: "老配置" }], activeGameConfig: "y" });
  const act = api.getActiveGameConfig();
  if (act.capStyle !== "one-two") errs.push("预设：缺失字段应补默认值，实际 " + act.capStyle);
  if (act.winCondition !== "last") errs.push("预设：缺失胜负判定应补 last");
}

/* ═══════ 一键终局：历史记录必须存下来，且带上排名/回放依赖的字段 ═══════
 *
 * 用户报过：一键终局结束后，历史列表里根本没有这一局。
 * 两个真实的坑：
 *  1) 保存原来挂在结算渲染之后，渲染中途抛错就整条丢掉（已改为先保存再渲染）；
 *  2) 记录不带 playerTypes / killedBy / eliminatedOrder，历史详情页的类型与
 *     「被谁淘汰」全是空的 —— serde 会静默丢弃结构里没有的字段。
 */
{
  // 终点用注释本身（不含前面的 `}`），否则抽出来的函数少一个收尾大括号
  const saveFn = grab("async function saveGameHistory(", "// ─── 保存未完成游戏历史记录");
  const typesFn = grab("function toColorNamesArray(", "function getPlayerTypesArray(")
               + grab("function getPlayerTypesArray(", "async function saveGameHistory(");
  const comp = grab("function compactHistory(", "function expandHistory(");
  const code = [
    "let maxPlayers = 3, size = 7, cols = 7, boardCells = null;",
    "const borderMode = 'default', capMode = '4', capStyle = 'two-one', winCondition = 'last';",
    "let playerBarMetric = 'ml', gameMode = 'eve', aiAlgorithm = 'pvs', aiDepth = 3;",
    "const gameCount = 4, chainStats = {}, maxChainOverall = { player: null, length: 0 };",
    "const eliminationInfo = { 1: 0 }, eliminationRounds = [[1], [2]];",
    "const aiPlayers = new Set([0, 1, 2]);",
    "const aiConfigs = { 0: { algorithm: 'pvs', depth: 3, randomScale: 5, evalKind: 'points' },",
    "                    1: { algorithm: 'alphabeta', depth: 2, randomScale: 0, evalKind: 'ml' },",
    "                    2: { algorithm: 'strategy', depth: 1, randomScale: 0 } };",
    "const COLOR_NAMES = ['红', '蓝', '绿'];",
    "const window = { _colorNames: { 0: 'A', 1: 'B', 2: 'C' } };",
    "const formatTime = () => '2026/01/01 00:00:00';",
    "const logWarn = () => {};",
    "const showToast = () => {};",
    "const cleared = [];",
    "const holder = { record: null };",
    "const tauriInvoke = (cmd, args) => {",
    "  if (cmd === 'save_game_history') { holder.record = args.record; return Promise.resolve(); }",
    "  if (cmd === 'clear_round_history') { cleared.push(cmd); return Promise.resolve(); }",
    "  throw new Error('意外命令: ' + cmd);",
    "};",
    comp,
    typesFn,
    saveFn,
    "return { saveGameHistory, get record() { return holder.record; }, cleared };",
  ].join("\n");

  const api = new Function(code)();
  const hist = [
    { turn: 0, snapshot: { "0": { pieces: 1, points: 1 }, "1": { pieces: 1, points: 1 } }, mv: null },
    { turn: 1, snapshot: { "0": { pieces: 2, points: 3 }, "1": { pieces: 0, points: 0 } },
      mv: { x: 2, y: 3, player: 0, seed: 9 }, rating: [4, -4] },
  ];
  const okSaved = await api.saveGameHistory(0, undefined, undefined, undefined, hist);
  if (okSaved !== true) errs.push("历史保存：成功时应返回 true");
  const rec = api.record;
  if (!rec) errs.push("历史保存：应当调用一次 save_game_history");
  if (rec) {
    if (rec.winner !== 0) errs.push("历史保存：winner 没写进去");
    if (rec.finished !== true) errs.push("历史保存：已结束的对局必须标 finished=true");
    if (!rec.playerTypes || rec.playerTypes.length !== 3)
      errs.push("历史保存：必须带 playerTypes（历史详情靠它显示 AI 类型）");
    if (rec.playerTypes && rec.playerTypes[0] !== "pvs")
      errs.push("历史保存：playerTypes 应按 AI 配置填，实际 " + JSON.stringify(rec.playerTypes));
    if (!rec.killedBy || rec.killedBy["1"] !== 0)
      errs.push("历史保存：必须带 killedBy（「被谁淘汰」靠它）");
    if (!Array.isArray(rec.eliminatedOrder) || rec.eliminatedOrder.join(",") !== "1,2")
      errs.push("历史保存：淘汰先后顺序应展开成 [1,2]，实际 " + JSON.stringify(rec.eliminatedOrder));
    if (rec.capStyle !== "two-one") errs.push("历史保存：双爆方式没写进去");
    if (!rec.history || !rec.history.c) errs.push("历史保存：回合历史应为紧凑格式");
  }
  if (api.cleared.length !== 1) errs.push("历史保存：成功后应清掉 round_history");
}

  if (errs.length > before) {
    // 把这一文件新增的错误标注来源，避免两份报错混在一起分不清
    for (let k = before; k < errs.length; k++) errs[k] = "[" + label + "] " + errs[k];
  }
}

for (const file of SOURCES) {
  await checkFile(readFileSync(file, "utf-8"), file.includes("docs") ? "docs" : "tauri");
}

if (errs.length) {
  console.log("ERRORS:");
  errs.forEach((e) => console.log(" - " + e));
  process.exit(1);
}
console.log("ALL GOOD（自定义棋盘库 + 淘汰顺序 + 配置预设 + MVP，两份前端）");
