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

function checkFile(appJs, label) {
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

  if (errs.length > before) {
    // 把这一文件新增的错误标注来源，避免两份报错混在一起分不清
    for (let k = before; k < errs.length; k++) errs[k] = "[" + label + "] " + errs[k];
  }
}

for (const file of SOURCES) {
  checkFile(readFileSync(file, "utf-8"), file.includes("docs") ? "docs" : "tauri");
}

if (errs.length) {
  console.log("ERRORS:");
  errs.forEach((e) => console.log(" - " + e));
  process.exit(1);
}
console.log("ALL GOOD（自定义棋盘库 + 淘汰顺序，两份前端）");
