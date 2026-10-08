#!/usr/bin/env node
// 自定义音效的自检：跑 `node tauri/check_custom_sound.mjs`，有错就非 0 退出。
// 覆盖三件容易悄悄坏掉的事：
//   1) 数据契约：四个音效位必须与内置主题的键一一对应（少一个键，那位就永远不响）
//   2) 校验规则：格式 / 大小 / 时长 / 主题总量四条拒绝路径都要给出理由
//   3) 镜像一致：tauri/public 与 docs 两份 app.js 的关键函数必须逐字相同
import { readFileSync } from "fs";
import { resolve } from "path";

const root = resolve(import.meta.dirname, "..");
const read = p => readFileSync(resolve(root, p), "utf-8");
const src = { tauri: read("tauri/public/app.js"), docs: read("docs/app.js") };
const html = { tauri: read("tauri/public/index.html"), docs: read("docs/index.html") };
const errs = [];
const check = (ok, msg) => { if (!ok) errs.push(msg); };

// 按大括号配对从源码里抠出一个函数（用来单独喂 stub 跑）
function extractFn(code, name) {
  const i = code.indexOf("function " + name + "(");
  if (i < 0) return null;
  let depth = 0, k = code.indexOf("{", i);
  const start = k;
  for (; k < code.length; k++) {
    if (code[k] === "{") depth++;
    else if (code[k] === "}") { depth--; if (depth === 0) { k++; break; } }
  }
  return code.slice(i, k);
}

// 抠出 `const NAME={...}` 这种纯字面量常量并求值（SOUND_THEMES / DOG_BARK_FILES 都是）
function extractConst(code, name) {
  const i = code.indexOf("const " + name + "=");
  if (i < 0) return null;
  const open = code.indexOf("{", i);
  let depth = 0, k = open;
  for (; k < code.length; k++) {
    if (code[k] === "{") depth++;
    else if (code[k] === "}") { depth--; if (depth === 0) { k++; break; } }
  }
  return new Function("return " + code.slice(open, k))();
}

// ── 1) 数据契约 ──
const SLOT_KEYS = ["click", "explosion", "elim", "gameover"];
for (const k of SLOT_KEYS) {
  check(src.tauri.includes("{key:'" + k + "'"), "CS_SLOTS 里缺了音效位 " + k);
  check(new RegExp("^\\s+" + k + ":", "m").test(src.tauri), "内置主题里没有 " + k + " 这个音效位");
}
check(/const SOUND_EXTS=\['mp3','wav','ogg'\]/.test(src.tauri), "SOUND_EXTS 白名单不是 mp3/wav/ogg");

// ── 2) 校验规则真的会拒绝（用 stub 喂各种坏文件） ──
const fnSrc = extractFn(src.tauri, "validateSoundFile");
const extSrc = extractFn(src.tauri, "soundExt");
if (!fnSrc) errs.push("找不到 validateSoundFile");
else if (!extSrc) errs.push("找不到 soundExt");
else {
  const soundExt = new Function("return " + extSrc)();
  const V = new Function(
    "csDraft", "probeSoundDuration", "CS_SLOTS", "SOUND_EXTS",
    "SOUND_MAX_SIZE", "SOUND_MAX_TOTAL", "SOUND_MAX_SECONDS", "soundExt",
    "return " + fnSrc
  );
  const mk = (slots, dur) => V({ slots }, async () => dur, SLOT_KEYS.map(k => ({ key: k })),
    ["mp3", "wav", "ogg"], 1024 * 1024, 4 * 1024 * 1024, 10, soundExt);

  const cases = [
    ["格式不对被拒", await mk({}, 3)({ name: "a.txt", size: 1000 }, "click"), "不支持的格式"],
    ["大小超限被拒", await mk({}, 3)({ name: "a.mp3", size: 3 * 1024 * 1024 }, "click"), "文件太大"],
    ["时长超限被拒", await mk({}, 30)({ name: "a.mp3", size: 1000 }, "click"), "时长太长"],
    ["读不出音频被拒", await mk({}, -1)({ name: "a.mp3", size: 1000 }, "click"), "读不出音频信息"],
    ["正常文件放行", await mk({}, 3)({ name: "a.mp3", size: 1000 }, "click"), ""],
    ["主题总量超限被拒",
      await mk({ explosion: { kind: "file", ref: "r", size: 3.5 * 1024 * 1024 } }, 3)({ name: "a.mp3", size: 1024 * 1024 }, "click"),
      "太多"],
  ];
  for (const [name, got, want] of cases) {
    if (want === "") check(got === "", name + "：应当放行，实际返回「" + got + "」");
    else check(typeof got === "string" && got.includes(want), name + "：期望提示含「" + want + "」，实际「" + got + "」");
  }
}

// ── 主题名会拼进 innerHTML，必须转义 ──
const escSrc = extractFn(src.tauri, "htmlEsc");
if (!escSrc) errs.push("找不到 htmlEsc");
else {
  const esc = new Function("return " + escSrc)();
  check(esc("<img src=x onerror=alert(1)>").indexOf("<") < 0, "htmlEsc 没有挡住标签注入");
  check(esc('a\'b"c&d') === "a&#39;b&quot;c&amp;d", "htmlEsc 转义结果不对：" + esc('a\'b"c&d'));
  check(esc(null) === "" && esc(undefined) === "", "htmlEsc 对空值应返回空串");
}

// ── 大狗叫的「短中长」：自定义槽位自带的 bark 要真的盖过设置里的全局值 ──
const pbSrc = extractFn(src.tauri, "playBuiltinSlot");
const themes = extractConst(src.tauri, "SOUND_THEMES");
const barks = extractConst(src.tauri, "DOG_BARK_FILES");
if (!pbSrc || !themes || !barks) errs.push("抽不出 playBuiltinSlot / SOUND_THEMES / DOG_BARK_FILES");
else {
  const played = [];
  const appSettings = { dogBarkMode: "long" };
  const pb = new Function("SOUND_THEMES", "DOG_BARK_FILES", "appSettings", "playSoundFile", "playTone",
    pbSrc + "\nreturn playBuiltinSlot;")(themes, barks, appSettings, s => played.push(s), () => {});
  pb("dog", "explosion", "none");
  check(played[0] === barks.none, "自定义槽位指定无淡出，实际播的是 " + played[0]);
  pb("dog", "explosion", "medium");
  check(played[1] === barks.medium, "自定义槽位指定中淡出，实际播的是 " + played[1]);
  pb("dog", "explosion");   // 内置大狗叫主题不传 bark → 跟随设置
  check(played[2] === barks.long, "不传 bark 时应跟随设置里的长淡出，实际 " + played[2]);
  appSettings.dogBarkMode = "none";
  pb("dog", "explosion");
  check(played[3] === barks.none, "设置改成无淡出后没跟随，实际 " + played[3]);
  pb("dog", "click");
  check(played[4] === "audio/大狗.mp3", "大狗叫的落子音应仍是固定文件，实际 " + played[4]);
  check(/DOG_BARK_LABELS\[bm\]/.test(src.tauri), "编辑器没把大狗叫的叫声模式摊开成选项");
}

// ── 3) 镜像一致 + 页面/JS 的元素 id 对得上 ──
for (const n of ["htmlEsc", "validateSoundFile", "playBuiltinSlot", "playThemeSound", "getCustomSoundTheme", "onCsFileChosen"]) {
  check(extractFn(src.tauri, n) === extractFn(src.docs, n), "两份镜像的 " + n + " 不一致（只改了一份）");
}
for (const [who, h] of Object.entries(html)) {
  check(h.includes('id="custom-sound"'), who + "/index.html 缺 custom-sound 页面");
  for (const id of ["csThemeList", "csEditor", "csLabel", "csSlots", "csSaveBtn", "csDeleteBtn", "csFileInput"]) {
    // JS 通过 getElementById 拿的每个 id，HTML 里都得真有
    if (src[who].includes("'" + id + "'")) check(h.includes('id="' + id + '"'), who + "/index.html 缺元素 id=" + id);
  }
}
check(src.tauri.includes("Router.register('custom-sound'"), "app.js 没注册 custom-sound 路由");

if (errs.length) {
  console.log("自定义音效自检：发现 " + errs.length + " 个问题");
  errs.forEach(e => console.log(" - " + e));
  process.exit(1);
}
console.log("自定义音效自检：全部通过");
