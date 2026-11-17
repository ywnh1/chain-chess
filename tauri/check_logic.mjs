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
  await new Promise((r) => setTimeout(r, 10));

  if (called !== 1) errs.push("评分：应调用一次 rate_board，实际 " + called);
  if (!api.live) errs.push("评分：算完必须写回 _liveRating（否则排名退回棋子数）");
  if (api.live && (api.live[0] !== 42 || api.live[1] !== -7))
    errs.push("评分：_liveRating 内容不对 " + JSON.stringify(api.live));
  if (api.rendered < 1) errs.push("评分：算完应重绘玩家条，实际重绘 " + api.rendered + " 次");

  // 评分失败时必须静默降级，不能把上一次的分留着误导用户，也不能抛
  const api2 = new Function(code)();
  api2.setInvoke(() => Promise.reject(new Error("命令不存在")));
  api2.scheduleRating();
  await new Promise((r) => setTimeout(r, 10));
  if (api2.live) errs.push("评分：失败时不该写入 _liveRating");
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
