import { readFileSync } from "fs";
import { resolve } from "path";

// 页面返回逻辑自检：从真实源码里解析每个页面的 back 声明，
// 逐条验证「从哪进就回哪」。
//
// 为什么需要：about-ai / about-benchmark 从「更多」进入，但 back 曾写死成
// 'about'，用户侧滑返回时会莫名其妙跑到「关于」页。这类问题语法检查
// 发现不了，只有把导航跑一遍才看得出来。
//
// 运行：node tauri/check_nav.mjs
const path = process.argv[2] || resolve(import.meta.dirname, "public/app.js");
const appJs = readFileSync(path, "utf8");

// 只取 Router 对象（不含各页 enter/leave 的业务代码）
const routerSrc = appJs.slice(appJs.indexOf("const Router = {"), appJs.indexOf("// ─── 页面注册 ───"));
const Router = new Function(
  "document", "location", "history",
  routerSrc + "; return Router;"
)({ querySelectorAll: () => [], getElementById: () => null, activeElement: null },
  { hash: "" }, { pushState() {} });

// 从源码里解析每个页面的 back 声明（字符串或函数体）
const regs = [...appJs.matchAll(/Router\.register\('([a-z-]+)',\s*\{([\s\S]*?)\n\}\);/g)];
let parsed = 0;
for (const [, id, body] of regs) {
  const mStr = /^\s*back:\s*'([A-Za-z-]+)'/m.exec(body);
  const mFn = /^\s*back\(\)\s*\{\s*return\s+(.+?);\s*\}/m.exec(body);
  const mNull = /^\s*back:\s*null/m.exec(body);
  if (mStr) { Router.register(id, { back: mStr[1] }); parsed++; }
  else if (mFn) {
    const expr = mFn[1];
    Router.register(id, { back: () => new Function("Router", "return (" + expr + ")")(Router) });
    parsed++;
  } else if (mNull) { Router.register(id, { back: null }); parsed++; }
  else { Router.register(id, {}); }
}
console.log("解析页面:", parsed, "/", regs.length);

const cases = [
  ["settings", "about-ai", "settings"],
  ["settings", "about-benchmark", "settings"],
  ["settings", "about-rules", "settings"],
  ["settings", "about-changelog", "settings"],
  ["settings", "about", "settings"],
  ["welcome", "about", "welcome"],
  ["about", "about-license", "about"],
  ["about", "about-thirdparty", "about"],
  ["gameSetup", "game-config", "gameSetup"],
  ["gameSetup", "board-editor", "gameSetup"],
  ["settings", "custom-sound", "settings"],
  ["settings", "device-bench", "settings"],
];

let bad = 0;
for (const [from, to, want] of cases) {
  // 模拟「从 from 进入 to」后的 _prev
  Router._prev = from;
  Router._current = to;
  const got = Router._resolveBack(to, "welcome");
  const ok = got === want;
  if (!ok) bad++;
  console.log((ok ? "OK   " : "FAIL ") + (from + " → " + to).padEnd(34) + "back→ " + got + (ok ? "" : "  期望 " + want));
}
console.log(bad ? "\n失败 " + bad + " 条" : "\n全部通过");
process.exit(bad ? 1 : 0);
