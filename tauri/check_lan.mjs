#!/usr/bin/env node
// 局域网联机协议自检：把 public/app.js 里的联机模块抽出来，
// 用多个独立沙箱（房主 + 若干客户端）共享一个假网络跑一遍房间与对局流程。
//
// 项目没有测试框架，这份脚本就是「逻辑坏掉会失败」的最小检查：
// 座位分配、重编号、准备状态、开局、落子排队与转发、名字清洗、掉线清理，
// 任何一处走偏都会红。
// 运行：node tauri/check_lan.mjs

import { readFileSync } from "fs";
import { resolve } from "path";

const appJs = readFileSync(resolve(import.meta.dirname, "public/app.js"), "utf8");
const START = "// 局域网联机（Rust 传输层 lan.rs";
const END = "// ─── 兼容旧函数（逐步迁移） ───";
const i = appJs.indexOf(START);
const j = appJs.indexOf(END, i);
if (i < 0 || j < 0) {
  console.error("找不到联机模块（标记被改名了？）");
  process.exit(1);
}
const code = appJs.slice(i, j);

/* 联机模块依赖几个定义在它之前的函数。直接抽真实实现注入沙箱，
 * 而不是在测试里重写一份 —— 重写的那份会掩盖真实代码的 bug。 */
function grabFn(name) {
  let i = appJs.indexOf("async function " + name + "(");
  if (i < 0) i = appJs.indexOf("function " + name + "(");
  if (i < 0) return "";
  // 从函数首行扫到行首的 "}"，按花括号配平，避免被嵌套块骗
  let depth = 0, started = false;
  for (let k = i; k < appJs.length; k++) {
    const ch = appJs[k];
    if (ch === "{") { depth++; started = true; }
    else if (ch === "}") {
      depth--;
      if (started && depth === 0) return appJs.slice(i, k + 1);
    }
  }
  return "";
}

const DEPS = ["getMaxPlayersByArea", "countOpenCells", "resolveRandomBorder",
              "resolveRandomCap", "htmlEsc", "setSelected"];

/* triggerAI 定义在 LAN 模块之外，单独抽出来注入（它依赖 lanSendMove / lanRole，
 * 所以必须排在 LAN 模块之后）。 */
const triggerAiSrc = grabFn("triggerAI");
if (!triggerAiSrc.includes("function triggerAI(")) {
  console.error("抽不到 triggerAI");
  process.exit(1);
}
/* localClick 也要用真的：它开头的「轮到 AI 思考」守卫曾经把远端发来的
 * AI 着法静默吞掉。用桩测不出这个 bug，必须跑真实代码。 */
const localClickSrc = grabFn("localClick");
if (!localClickSrc.includes("function localClick(")) {
  console.error("抽不到 localClick");
  process.exit(1);
}
const depsSrc = DEPS.map(grabFn).join("\n");
for (const n of DEPS) if (!depsSrc.includes("function " + n + "(")) {
  console.error("抽不到依赖函数: " + n);
  process.exit(1);
}

const errs = [];
function ok(cond, label) {
  if (!cond) errs.push(label);
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/* ─── 假网络：一个房主 + 若干客户端 ─── */
const net = {
  nextPeerId: 1,
  hostInbox: [],        // 房主收到的 { type, id, data }
  clients: new Map(),   // peerId -> { inbox: [] }
};

function resetNet() {
  net.nextPeerId = 1;
  net.hostInbox = [];
  net.clients.clear();
}

function clientInbox(peerId) {
  const c = net.clients.get(peerId);
  return c ? c.inbox : [];
}

/// 极简 DOM 元素：只实现联机代码真正用到的部分
/// （class 选择器 / 属性选择器 / appendChild / innerHTML 清空）。
function makeEl(id, tag = "div", cls = "") {
  const classes = new Set(cls ? cls.split(/\s+/).filter(Boolean) : []);
  const el = {
    id,
    tagName: tag.toUpperCase(),
    style: {},
    dataset: {},
    value: "",
    textContent: "",
    children: [],
    options: [],
    onclick: null,
    onchange: null,
    classList: {
      add(c) { classes.add(c); },
      remove(c) { classes.delete(c); },
      contains(c) { return classes.has(c); },
      toggle(c, on) { on ? classes.add(c) : classes.delete(c); },
    },
    get _classes() { return [...classes]; },
    get className() { return [...classes].join(" "); },
    set className(v) {
      classes.clear();
      String(v || "").split(/\s+/).filter(Boolean).forEach((c) => classes.add(c));
    },
    appendChild(c) {
      this.children.push(c);
      if (this.tagName === "SELECT") this.options.push(c);
      return c;
    },
    // ".a,.b" 形式：任一命中即可（联机代码里 setSelected 就是这么用的）
    matches(sel) {
      return String(sel).split(",").some((part) => {
        const p = part.trim();
        if (!p) return false;
        if (p[0] === ".") return classes.has(p.slice(1));
        return this.tagName === p.toUpperCase();
      });
    },
    setAttribute(k, v) { this._attrs = this._attrs || {}; this._attrs[k] = String(v); },
    getAttribute(k) { return this._attrs ? this._attrs[k] : undefined; },
    addEventListener() {},
    querySelectorAll(sel) { return queryAll(this, sel); },
    querySelector(sel) { return queryAll(this, sel)[0] || null; },
  };
  // innerHTML = "" 要清空子节点（代码用它重置下拉框）
  let _html = "";
  Object.defineProperty(el, "innerHTML", {
    get() { return _html; },
    set(v) { _html = v; if (v === "") { el.children = []; el.options = []; } },
  });
  return el;
}

/// 支持 .cls / tag / .cls[data-value="x"] 三种形态
function queryAll(root, sel) {
  const m = /^([.#]?[\w-]+)?(?:\[([\w-]+)="([^"]*)"\])?$/.exec(sel);
  if (!m) return [];
  const [, name, attr, attrVal] = m;
  const out = [];
  (function walk(node) {
    for (const c of node.children || []) {
      let ok = true;
      if (name) {
        if (name[0] === ".") ok = c.classList.contains(name.slice(1));
        else if (name[0] === "#") ok = c.id === name.slice(1);
        else ok = c.tagName === name.toUpperCase();
      }
      if (ok && attr) {
        const v = attr === "data-value" ? c.dataset.value : c[attr];
        ok = String(v) === attrVal;
      }
      if (ok) out.push(c);
      walk(c);
    }
  })(root);
  return out;
}

/// 预置联机规则控件（房主页面）。返回按 id 取元素的函数。
function buildRuleControls() {
  const mk = (id, tag, cls) => makeEl(id, tag, cls);

  const shape = mk("lanShapeGroup", "div", "toggle-group");
  for (const [v, label] of [["square", "正方形"], ["rect", "长方形"], ["random", "随机"], ["custom", "自定义"]]) {
    const b = mk("", "button", "tg-btn" + (v === "square" ? " selected" : ""));
    b.dataset.value = v; b.textContent = label;
    shape.appendChild(b);
  }

  const sizes = mk("lanSizeGrid", "div", "size-grid");
  for (let v = 5; v <= 19; v++) {
    const b = mk("", "button", "size-btn" + (v === 7 ? " selected" : ""));
    b.dataset.value = String(v); b.textContent = v + "×" + v;
    sizes.appendChild(b);
  }

  const border = mk("lanBorderGroup", "div", "btn-group");
  for (const v of ["default", "wrap", "bounce", "degrade", "random"]) {
    const b = mk("", "button", "gb" + (v === "default" ? " selected" : ""));
    b.dataset.value = v;
    border.appendChild(b);
  }

  const cap = mk("lanCapGroup", "div", "btn-group");
  for (const v of ["3", "4", "5", "random"]) {
    const b = mk("", "button", "gb" + (v === "4" ? " selected" : ""));
    b.dataset.value = v;
    cap.appendChild(b);
  }

  const rowsSel = mk("lanRectRows", "select");
  rowsSel.value = "7";
  const colsSel = mk("lanRectCols", "select");
  colsSel.value = "7";
  const customSel = mk("lanCustomBoard", "select");

  // 人机数量 / 强度：都是 <select>，测试里直接改 value
  const aiCountSel = mk("lanAiCount", "select");
  aiCountSel.value = "0";
  const aiStrengthSel = mk("lanAiStrength", "select");
  aiStrengthSel.value = "strategy";

  return {
    lanNetBody: mk("lanNetBody"), lanRoomRemoteRow: mk("lanRoomRemoteRow"),
    lanRoomV6: mk("lanRoomV6"), lanCopyLan: mk("lanCopyLan"),
    lanCopyV6: mk("lanCopyV6"), lanRoomAddr: mk("lanRoomAddr"),
    lanNetCard: mk("lanNetCard"),
    lanAiCount: aiCountSel, lanAiStrength: aiStrengthSel,
    lanAiStrengthRow: mk("lanAiStrengthRow"),
    lanShapeGroup: shape, lanSizeGrid: sizes, lanBorderGroup: border, lanCapGroup: cap,
    lanRectRows: rowsSel, lanRectCols: colsSel, lanCustomBoard: customSel,
    lanSquareRow: mk("lanSquareRow"), lanRectRow: mk("lanRectRow"),
    lanCustomRow: mk("lanCustomRow"), lanRandomHint: mk("lanRandomHint"),
    lanBorderDesc: mk("lanBorderDesc"), lanCapDesc: mk("lanCapDesc"),
    lanConfigHost: mk("lanConfigHost"), lanConfigReadonly: mk("lanConfigReadonly"),
    lanRulesOwner: mk("lanRulesOwner"),
  };
}

function makeSandbox(role, peerId = null, port = 43210) {
  // 预置联机规则控件（房主页面）与下拉框，其余元素按需创建
  const els = new Map(Object.entries(buildRuleControls()));
  const document = {
    getElementById(id) {
      if (!els.has(id)) els.set(id, makeEl(id));
      return els.get(id);
    },
    // 注意 makeEl 的第一个参数是 id：这里必须把 tag 传进第二参，
    // 否则 createElement('select') 会造出一个 DIV。
    createElement(tag) { return makeEl("", tag); },
    createTextNode(t) { return { nodeValue: t }; },
    body: { style: {}, classList: { add() {}, remove() {} } },
  };
  const messages = [];
  const localClicks = [];
  const sent = [];
  const aiMoveCalls = [];
  // 每个客户端一条独立的收件队列，按 peerId 注册到假网络
  const inbox = [];
  if (role === "client") net.clients.set(peerId, { inbox });

  const invoke = async (cmd, args = {}) => {
    switch (cmd) {
      case "lan_host":
        // 4.1.0 起返回 {port, ipv6}；env.hostIpv6=false 可模拟「系统绑不上 IPv6」
        return { port: port, ipv6: env.hostIpv6 !== false };
      case "lan_local_ip":
        return "192.168.1.5";
      case "lan_ipv6":
        // env.ipv6Missing = true 模拟旧版本包里根本没有这条命令
        if (env.ipv6Missing) throw new Error("Command lan_ipv6 not found");
        return env.ipv6 || {
          available: false, addr: null, raw: null, code: "no_route",
          reason: "网络没有下发 IPv6 地址（当前 Wi-Fi/移动数据未启用 IPv6）",
        };
      case "lan_join":
        if (!/^.+:\d+$/.test(args.addr)) throw new Error("bad addr");
        return null;
      case "lan_send":
        sent.push(args.msg);
        if (role === "host") {
          for (const c of net.clients.values()) c.inbox.push({ type: "msg", id: null, data: args.msg });
        } else {
          net.hostInbox.push({ type: "msg", id: peerId, data: args.msg });
        }
        return null;
      case "lan_send_to":
        clientInbox(args.id).push({ type: "msg", id: args.id, data: args.msg });
        return null;
      case "lan_poll": {
        const q = role === "host" ? net.hostInbox : inbox;
        return q.splice(0, q.length);
      }
      case "lan_stop":
        return null;
      case "lan_discover":
        return [];
      case "process_move":
        env.lastSeed = args.seed;
        return { board: __env.getBoard(), eliminated: [], killedBy: [],
                 steps: null, exploded: null };
      case "ai_move_strategy":
      case "ai_move":
      case "ai_move_v2":
        // 固定落在 (0,0)，也就是测试里 setAiPiece 给 AI 放的那颗棋子上。
        // （有棋子之后只能点自己的棋子，落到空格会被合法性质检拦下）
        aiMoveCalls.push(args.player);
        return [0, 0];
      default:
        throw new Error("unexpected cmd: " + cmd);
    }
  };

  const env = {
    document,
    typeof: undefined,
    window: { __TAURI_INTERNALS__: { invoke: async () => {} } },
    localStorage: { getItem: () => null, setItem: () => {} },
    tauriInvoke: invoke,
    showMsg: (t, c) => messages.push({ text: t, kind: c }),
    Router: { register() {}, navigate() {} },
    COLORS: Array.from({ length: 10 }, (_, k) => "#" + k),
    COLORS_LIGHT: Array.from({ length: 10 }, () => "rgba(0,0,0,.06)"),
    setInterval: () => 1,
    clearInterval: () => {},
    messages,
    localClicks,
    aiMoveCalls,
    customBoards: [],
    vibrate: () => {},
    ipv6: null,
    hostIpv6: true,
  };

  const prelude = `
    const { document, window, localStorage, tauriInvoke, showMsg, Router, COLORS, COLORS_LIGHT,
            setInterval: mockSetInterval, clearInterval: mockClearInterval } = __env;
    const setInterval = mockSetInterval, clearInterval = mockClearInterval;
    const vibrate = __env.vibrate;
    let gameMode=null, curPlayer=0, gameOver=false, isPaused=false, animating=false, aiThinking=false;
    let board=[], size=7, cols=7, maxPlayers=2, boardCells=null, borderMode='default', capMode='4', firstMovePos=null;
    let _colorNames={}, _originPage=null;
    let aiPlayers=new Set(), aiConfigs={}, eliminatedPlayers=new Set();
    let eliminationRounds=[], eliminationInfo={}, gameHistory=[], chainStats={};
    let maxChainOverall={player:null,length:0}, undoStack=[];
    // 4.1.0 起 localClick 会带上这两项；沙箱里给默认值即可
    let capStyle='one-two', winCondition='last';
    let cells = [];
    const mkBoard=(s,c)=>Array.from({length:s},()=>Array.from({length:c},()=>({owner:null,count:0})));
    const resetRoundHistory=()=>{gameHistory=[]};
    const clearSavedGameState=()=>{};
    const recordHistory=()=>{gameHistory.push({turn:gameHistory.length})};
    // renderBoard 在真实代码里负责填充 cells；这里用同尺寸占位元素代替，
    // 让 triggerAI 里的 cells[x][y] 取得到东西。
    const renderBoard=()=>{
      cells = board.map(row=>row.map(()=>({style:{},classList:{add(){},remove(){}}})));
    };
    const show=()=>{}; const renderPlayerBar=()=>{};
    const updateFastFinishBtn=()=>{}; const exitGame=()=>{ gameMode=null; };
    const hasPieces=(p,b)=>b.some(r=>r.some(c=>c.owner===p));
    const isInRestrictedZone=()=>false;
    // 随机棋盘与自定义棋盘库：测试里给固定实现（形状相关的正确性在 check_logic 里测）
    const makeRandomShape=(cnt)=>({rows:8,cols:8,cells:Array.from({length:8},()=>Array(8).fill('1'))});
    const getCustomBoards=()=>__env.customBoards||[];
    const BORDER_MODE_DESC={default:'默认边界',wrap:'回环边界',bounce:'反弹边界',degrade:'降级边界',random:'随机边界'};
    const CAP_MODE_DESC={'3':'3 级','4':'4 级','5':'5 级',random:'随机阈值'};
    /*__DEPS__*/

    /* ─── triggerAI 跑通所需的桩（只为验证「谁有权驱动 AI」这一件事）─── */
    let aiAlgorithm = 'strategy', aiDepth = 2, gameCount = 0;
    const sleep = (ms) => new Promise(r => setTimeout(r, ms));
    const saveUndoState = () => { undoStack.push({}); };
    const startAiWaitHint = () => {};
    const stopAiWaitHint = () => {};
    const playClick = () => {};
    const playGameOver = () => {};
    const playExplosion = () => {};
    const addRipple = () => {};
    const setBg = () => {};
    const showSettlement = () => {};
    const logWarn = () => {};
    const COLOR_NAMES = ['红色','黄色','蓝色','绿色','紫色','粉色','青色','橙色','棕色','深灰色'];
    // 真实 localClick 通过 tauriInvoke('process_move') + processClick 落子；
    // 这里在 processClick 里记录，等价于「这一手真的被应用了」。
    const processClick = async (b,s,x,y,pl) => {
      __env.localClicks.push({x,y,pl,seed:__env.lastSeed});
      if(b[x] && b[x][y]) b[x][y].owner = pl;
      return [];
    };
    const handleEliminations = () => {};
    __env.getBoard = () => board;
    __env.getCurPlayer = () => curPlayer;
    __env.setAiPiece = (pid, x, y) => { board[x][y].owner = pid; board[x][y].count = 1; };
    const __set = { setAnimating:(v)=>{animating=v}, setBoard:(b)=>{board=b} };
  `;
  const epilogue = `
    return {
      get role(){return lanRole}, get myPid(){return lanMyPid},
      get players(){return lanPlayers}, get addr(){return lanRoomAddr},
      get gameMode(){return gameMode}, get curPlayer(){return curPlayer},
      get maxPlayers(){return maxPlayers}, get names(){return _colorNames},
      get inbox(){return lanInbox},
      get seats(){return lanSeats?Array.from(lanSeats.entries()):null},
      get remoteCfg(){return lanRemoteCfg},
      get aiSeats(){return [...aiPlayers]},
      get aiCfg(){return aiConfigs},
      get aiThinking(){return aiThinking},
      get aiMoveCalls(){return __env.aiMoveCalls},
      triggerAI,
      setAiPiece:(pid,x,y)=>__env.setAiPiece(pid,x,y),
      get cfg(){return lanCfg},
      get size(){return size},
      get cols(){return cols},
      get borderMode(){return borderMode},
      get capMode(){return capMode},
      // 直接改选中态，模拟用户点按钮 / 改下拉
      setRule:(groupId, value)=>{
        const box=document.getElementById(groupId);
        if(box.tagName==='SELECT'){ box.value=String(value); return; }
        box.querySelectorAll('button').forEach(function(b){
          b.classList.toggle('selected', b.dataset.value===value);
        });
      },
      onRuleChange:(caller)=>lanOnRuleChange(caller),
      renderAiRows:()=>lanRenderAiRows(),
      // 设定第 k 个 AI 的算法（等价于用户在下拉里选）
      setAiAlgo:(k, alg)=>{
        const _b=document.getElementById('lanAiRows');

        const box=document.getElementById('lanAiRows');
        const rows=box.querySelectorAll('.lan-ai-row');
        const sel=rows[k] && rows[k].querySelector('select');
        if(!sel) throw new Error('AI 行不存在: '+k);
        sel.value=alg;
      },
      resolveMatch:(raw, n)=>lanResolveMatch(raw, n),
      checkIpv6:()=>lanCheckIpv6(),
      setIpv6:(v)=>{ __env.ipv6 = v; },
      setIpv6Missing:(v)=>{ __env.ipv6Missing = v; },
      setHostIpv6:(v)=>{ __env.hostIpv6 = v; },
      // innerHTML 在 mock 里只是字符串，直接读出来做断言
      get netBodyHtml(){ return (document.getElementById('lanNetBody')||{}).innerHTML || ''; },
      get roomRowShown(){ return (document.getElementById('lanRoomRemoteRow')||{}).style.display !== 'none'; },
      get roomV6Text(){ return (document.getElementById('lanRoomV6')||{}).textContent || ''; },
      renderRoomAddr:()=>lanRenderRoomAddr(),
      v6Endpoint:(a,p)=>lanV6Endpoint(a,p),
      get customBoards(){ return __env.customBoards; },
      lanCreateRoom, lanJoinRoom, lanHostStart, lanToggleReady, lanDrainEvents,
      lanSendMove, lanLeaveRoom, lanApplyRemote,
      setCurPlayer:(p)=>{curPlayer=p},
      setGameMode:(m)=>{gameMode=m},
      localClicks: __env.localClicks,
      messages: __env.messages,
      ...__set,
    };
  `;
  // 依赖源码当占位符替换进去；用函数形式避免 $ 被 replace 特殊解释
  const full = prelude.replace("/*__DEPS__*/", () => depsSrc) + code
             + triggerAiSrc + localClickSrc + epilogue;
  const fn = new Function("__env", full);
  return fn(env);
}

/// 等所有沙箱的事件队列与落子队列都空下来
async function settle(sandboxes) {
  for (let k = 0; k < 20; k++) {
    for (const sb of sandboxes) await sb.lanDrainEvents();
    await sleep(70);
    if (sandboxes.every((sb) => sb.inbox.length === 0)) return;
  }
}

async function main() {
  const host = makeSandbox("host");
  const clientA = makeSandbox("client", 1);
  const clientB = makeSandbox("client", 2);
  const all = [host, clientA, clientB];

  /* 1. 房主建房 */
  await host.lanCreateRoom();
  ok(host.role === "host", "房主角色没设上");
  ok(host.addr === "192.168.1.5:43210", "房间地址不对: " + host.addr);
  ok(host.players.filter(Boolean).length === 1, "房主自己没在玩家列表里");

  /* 2. 两个客户端加入 */
  await clientA.lanJoinRoom("192.168.1.5:43210");
  await settle(all);
  await clientB.lanJoinRoom("192.168.1.5:43210");
  await settle(all);
  ok(clientA.myPid === 1, "客户端 A 座位号不对: " + clientA.myPid);
  ok(clientB.myPid === 2, "客户端 B 座位号不对: " + clientB.myPid);
  ok(host.players.filter(Boolean).length === 3, "房主没记上两位玩家");

  /* 3. 准备 */
  clientA.lanToggleReady();
  clientB.lanToggleReady();
  await settle(all);
  ok(host.players[1] && host.players[1].ready === true, "A 的准备状态没传到房主");
  ok(host.players[2] && host.players[2].ready === true, "B 的准备状态没传到房主");

  /* 4. B 中途离开：房主座位表应把 2 号清空 */
  net.hostInbox.push({ type: "peer", event: "leave", id: 2 });
  await settle(all);
  ok(host.players.filter(Boolean).length === 2, "掉线玩家的座位没清理");

  /* 5. 开局：座位重编号后 lanSeats 必须同步（关键回归点） */
  host.lanHostStart();
  await settle(all);
  ok(host.gameMode === "lan" && clientA.gameMode === "lan", "开局后没有进入 lan 模式");
  ok(host.role === "host", "开局把房主的会话拆了（lan 页 leave 钩子过早断网）");
  ok(clientA.role === "client", "开局把客户端的会话拆了（lan 页 leave 钩子过早断网）");
  ok(host.maxPlayers === 2 && clientA.maxPlayers === 2, "重编号后人数不一致");
  ok(clientA.myPid === 1, "重编号后客户端座位号不对: " + clientA.myPid);

  /* 6. A 落子 → 房主应用；房主自己的回合不推进 */
  host.setCurPlayer(1);
  clientA.setCurPlayer(1);
  clientA.lanSendMove(2, 3, 777);
  await settle(all);
  ok(host.localClicks.length === 1, "房主没应用客户端的落子（lanSeats 没回写？）");
  ok((host.localClicks[0]||{}).x === 2 && (host.localClicks[0]||{}).y === 3, "落子坐标传丢了");
  ok((host.localClicks[0]||{}).seed === 777, "随机种子传丢了（会导致两端棋盘不一致）");

  /* 7. 房主落子 → 客户端按 seed 应用 */
  host.setCurPlayer(0);
  clientA.setCurPlayer(0);
  host.lanSendMove(4, 5, 12345);
  await settle(all);
  ok(clientA.localClicks.length === 1, "客户端没应用房主的落子");
  ok((clientA.localClicks[0]||{}).seed === 12345, "房主落子的种子丢了");

  /* 8. 动画期间收到的落子不能丢（排队后仍要应用） */
  clientA.setAnimating(true);
  clientA.setCurPlayer(1);
  // 不 await：真实场景里它是被消息驱动的，动画没播完就一直排队
  const queued = clientA.lanApplyRemote({ player: 1, x: 6, y: 6, seed: 999 });
  await sleep(150);
  ok(clientA.localClicks.length === 1, "动画期间的落子被直接丢掉了（应有队列）");
  clientA.setAnimating(false);
  await queued;
  ok(clientA.localClicks.length === 2, "动画结束后排队的落子没有补上");
  ok((clientA.localClicks[1]||{}).seed === 999, "排队的落子种子丢了");

  /* 9. 越界坐标不能崩、也不能被应用 */
  const beforeOut = host.localClicks.length;
  net.hostInbox.push({ type: "msg", id: 1, data: { t: "move", x: -1, y: 999, seed: 3 } });
  await settle(all);
  ok(host.localClicks.length === beforeOut, "房主应用了越界坐标");

  /* 10. 玩家名清洗：HTML 元字符与控制字符都要剥掉 */
  host.setGameMode(null);   // 对局已结束：回到房间态才允许新人加入
  await clientA.lanLeaveRoom();
  await settle(all);
  await clientA.lanJoinRoom("192.168.1.5:43210");
  const nick = clientA.messages.length;   // 占位，确保 messages 可读
  void nick;
  net.hostInbox.push({ type: "msg", id: 99, data: { t: "hello", name: "<img src=x>张三" } });
  await settle(all);
  const cleaned = host.players.filter(Boolean).map((p) => p.name).join("|");
  ok(!/[<>"]/.test(cleaned), "玩家名没有清洗掉 HTML 元字符: " + cleaned);
  ok(cleaned.includes("张三"), "清洗把正常字符也删了: " + cleaned);

  /* 11. 客户端不信任主机：恶意的 welcome / start 也洗不进来 */
  net.clients.set(4, { inbox: [] });
  const evil = makeSandbox("client", 4);
  clientInbox(4).push({ type: "msg", id: null, data: {
    t: "welcome", you: 0,
    players: [{ pid: 0, name: "<img src=x onerror=alert(1)>", ready: true, host: true }],
  }});
  await evil.lanDrainEvents();
  const evilName = ((evil.players || []).filter(Boolean)[0] || {}).name || "";
  ok(!/[<>]/.test(evilName), "客户端没洗掉主机发来的恶意名字: " + evilName);

  clientInbox(4).push({ type: "msg", id: null, data: {
    t: "start", you: 0, size: 7, cols: 7, maxPlayers: 1,
    names: { "0": "<script>x</script>" }, borderMode: "default", capMode: "4",
  }});
  await evil.lanDrainEvents();
  const evilStart = (evil.names || {})[0] || "";
  ok(!/[<>]/.test(evilStart), "客户端没洗掉 start 里的恶意名字: " + evilStart);

  /* ═══ 场景二：房主选完整规则 → 客户端同步 → 开局带全部配置 ═══ */
  resetNet();
  const host2 = makeSandbox("host", null, 43211);
  const cli2 = makeSandbox("client", 1);
  const all2 = [host2, cli2];

  await host2.lanCreateRoom();
  await cli2.lanJoinRoom("192.168.1.5:43210");
  await settle(all2);
  cli2.lanToggleReady();
  await settle(all2);

  /* 12. 房主改规则 → 客户端收到只读规则 */
  host2.setRule("lanShapeGroup", "rect");
  host2.onRuleChange("shape");
  host2.setRule("lanBorderGroup", "wrap");
  host2.onRuleChange("border");
  host2.setRule("lanCapGroup", "5");
  host2.onRuleChange("cap");
  host2.setRule("lanRectRows", "8");
  host2.setRule("lanRectCols", "13");
  host2.onRuleChange("shape");
  await settle(all2);
  const rc = cli2.remoteCfg || {};
  ok(rc.shape === "rect", "客户端没收到形状规则: " + JSON.stringify(rc));
  ok(rc.rows === 8 && rc.cols === 13, "客户端收到的行列不对: " + rc.rows + "x" + rc.cols);
  ok(rc.borderMode === "wrap", "客户端收到的边界不对: " + rc.borderMode);
  ok(rc.capMode === "5", "客户端收到的阈值不对: " + rc.capMode);

  /* 13. 开局：长方形 + 回环 + 5 级要原样落到两端 */
  host2.lanHostStart();
  await settle(all2);
  ok(host2.gameMode === "lan" && cli2.gameMode === "lan", "规则场景开局失败");
  ok(host2.size === 8 && host2.cols === 13, "房主端棋盘尺寸不对: " + host2.size + "x" + host2.cols);
  ok(cli2.size === 8 && cli2.cols === 13, "客户端棋盘尺寸不对: " + cli2.size + "x" + cli2.cols);
  ok(host2.borderMode === "wrap" && cli2.borderMode === "wrap", "边界两端不一致");
  ok(host2.capMode === "5" && cli2.capMode === "5", "阈值两端不一致");

  /* 14. 降级边界 + 5 级：互斥纠正为 4 级（不能让两端各判各的） */
  resetNet();
  const host3 = makeSandbox("host", null, 43212);
  const cli3 = makeSandbox("client", 1);
  const all3 = [host3, cli3];
  await host3.lanCreateRoom();
  await cli3.lanJoinRoom("192.168.1.5:43210");
  await settle(all3);
  cli3.lanToggleReady();
  await settle(all3);
  host3.setRule("lanBorderGroup", "degrade");
  host3.onRuleChange("border");
  host3.setRule("lanCapGroup", "5");
  host3.onRuleChange("cap");          // 后点阈值 → 边界弹回默认
  await settle(all3);
  host3.lanHostStart();
  await settle(all3);
  ok(host3.capMode !== "degrade" && host3.borderMode !== "degrade",
     "降级+5 级没做互斥纠正: border=" + host3.borderMode + " cap=" + host3.capMode);

  /* 15. 随机项必须由房主解析成确定值再下发 */
  resetNet();
  const host4 = makeSandbox("host", null, 43213);
  const cli4 = makeSandbox("client", 1);
  const all4 = [host4, cli4];
  await host4.lanCreateRoom();
  await cli4.lanJoinRoom("192.168.1.5:43210");
  await settle(all4);
  cli4.lanToggleReady();
  await settle(all4);
  host4.setRule("lanShapeGroup", "random");
  host4.onRuleChange("shape");
  host4.setRule("lanBorderGroup", "random");
  host4.onRuleChange("border");
  host4.setRule("lanCapGroup", "random");
  host4.onRuleChange("cap");
  await settle(all4);
  host4.lanHostStart();
  await settle(all4);
  ok(host4.borderMode !== "random" && cli4.borderMode !== "random",
     "随机边界没被解析成确定值: host=" + host4.borderMode + " client=" + cli4.borderMode);
  ok(host4.capMode !== "random" && cli4.capMode !== "random",
     "随机阈值没被解析成确定值: host=" + host4.capMode + " client=" + cli4.capMode);
  ok(host4.borderMode === cli4.borderMode && host4.capMode === cli4.capMode,
     "随机解析后两端不一致（会直接分叉）");

  /* 16. 容量不够时拒绝开局，不能带着非法配置进对局 */
  resetNet();
  const host5 = makeSandbox("host", null, 43214);
  const cli5a = makeSandbox("client", 1);
  const cli5b = makeSandbox("client", 2);
  const all5 = [host5, cli5a, cli5b];
  await host5.lanCreateRoom();
  await cli5a.lanJoinRoom("192.168.1.5:43210");
  await settle(all5);
  await cli5b.lanJoinRoom("192.168.1.5:43210");
  await settle(all5);
  cli5a.lanToggleReady();
  cli5b.lanToggleReady();
  await settle(all5);
  // 5×5 最多 5 人，这里 3 人 → 允许
  host5.setRule("lanSizeGrid", "5");
  host5.onRuleChange("size");
  await settle(all5);
  host5.lanHostStart();
  await settle(all5);
  ok(host5.gameMode === "lan", "5×5 + 3 人应当允许开局");
  ok(host5.maxPlayers === 3, "人数不对: " + host5.maxPlayers);

  /* 17. 容量拦截：棋盘放不下这么多人就该拒绝，而不是开局后有人没地方落子 */
  const smallBoard = { id: "small", name: "小盘", rows: 8, cols: 8,
    cells: Array.from({ length: 8 }, (_, i) =>
      Array.from({ length: 8 }, (_, j) => (i < 3 && j < 4 ? "1" : "0"))) };  // 只有 12 格
  host5.customBoards.push(smallBoard);
  const tooMany = host5.resolveMatch({ shape: "custom", customId: "small",
    borderMode: "default", capMode: "4" }, 6);
  ok(!!tooMany.error, "12 格的自定义棋盘放 6 人应当被拒绝: " + JSON.stringify(tooMany));
  const fits = host5.resolveMatch({ shape: "custom", customId: "small",
    borderMode: "default", capMode: "4" }, 5);
  ok(!fits.error, "12 格放 5 人应当允许: " + JSON.stringify(fits));

  /* ═══ 场景三：房内人机 ═══ */
  resetNet();
  const host6 = makeSandbox("host", null, 43215);
  const cli6 = makeSandbox("client", 1);
  const all6 = [host6, cli6];
  await host6.lanCreateRoom();
  await cli6.lanJoinRoom("192.168.1.5:43210");
  await settle(all6);
  cli6.lanToggleReady();
  await settle(all6);

  /* 18. 房主开 2 个 AI，并且分别给不同的算法 */
  host6.setRule("lanAiCount", "2");
  host6.renderAiRows();
  host6.setAiAlgo(0, "alphabeta");
  host6.setAiAlgo(1, "pvs");
  host6.onRuleChange("ai");
  await settle(all6);
  const rcAi = (cli6.remoteCfg || {}).aiList || [];
  ok(rcAi.length === 2, "客户端没收到 2 个 AI: " + JSON.stringify(cli6.remoteCfg));
  ok((rcAi[0] || {}).algorithm === "alphabeta",
     "第 1 个 AI 的算法没同步，实际 " + (rcAi[0] || {}).algorithm);
  ok((rcAi[1] || {}).algorithm === "pvs",
     "第 2 个 AI 的算法没同步，实际 " + (rcAi[1] || {}).algorithm);

  /* 18.5 增减 AI 数量时，已选好的算法要保留（不然调个数量全得重选） */
  host6.setRule("lanAiCount", "3");
  host6.renderAiRows();          // 2 → 3：前两个应保持 alphabeta / pvs
  host6.onRuleChange("ai");
  await settle(all6);
  const grow = (cli6.remoteCfg || {}).aiList || [];
  ok(grow.length === 3, "加到 3 个 AI 后长度不对: " + grow.length);
  ok((grow[0] || {}).algorithm === "alphabeta", "加 AI 后第 1 个算法被重置了");
  ok((grow[1] || {}).algorithm === "pvs", "加 AI 后第 2 个算法被重置了");
  ok((grow[2] || {}).algorithm === "strategy", "新增的第 3 个 AI 应为默认策略");
  // 减回去
  host6.setRule("lanAiCount", "2");
  host6.renderAiRows();
  host6.onRuleChange("ai");
  await settle(all6);
  const shrink = (cli6.remoteCfg || {}).aiList || [];
  ok(shrink.length === 2, "减到 2 个 AI 后长度不对: " + shrink.length);
  ok((shrink[0] || {}).algorithm === "alphabeta" && (shrink[1] || {}).algorithm === "pvs",
     "减 AI 后前两个算法变了");

  /* 19. 开局：2 真人 + 2 AI = 4 人，两端 AI 座位一致 */
  host6.lanHostStart();
  await settle(all6);
  ok(host6.maxPlayers === 4, "房主端人数应为 4，实际 " + host6.maxPlayers);
  ok(cli6.maxPlayers === 4, "客户端人数应为 4，实际 " + cli6.maxPlayers);
  const hs = host6.aiSeats.join(","), cs = cli6.aiSeats.join(",");
  ok(hs === "2,3", "房主端 AI 座位应为 2,3，实际 " + hs);
  ok(hs === cs, "两端 AI 座位不一致: host=" + hs + " client=" + cs);
  /* 每个 AI 各用各的算法：座位 2 = alphabeta，座位 3 = pvs
     （两端都要一致，否则客户端显示的和房主实际跑的对不上） */
  ok((host6.aiCfg[2] || {}).algorithm === "alphabeta",
     "房主端 AI 1 算法应为 alphabeta，实际 " + (host6.aiCfg[2] || {}).algorithm);
  ok((host6.aiCfg[3] || {}).algorithm === "pvs",
     "房主端 AI 2 算法应为 pvs，实际 " + (host6.aiCfg[3] || {}).algorithm);
  ok((cli6.aiCfg[2] || {}).algorithm === "alphabeta", "客户端 AI 1 算法不一致");
  ok((cli6.aiCfg[3] || {}).algorithm === "pvs", "客户端 AI 2 算法不一致");
  // 深度按各自算法推：pvs → 3，alphabeta → 2
  ok((host6.aiCfg[2] || {}).depth === 2, "AI 1 深度不对: " + (host6.aiCfg[2] || {}).depth);
  ok((host6.aiCfg[3] || {}).depth === 3, "AI 2 深度不对: " + (host6.aiCfg[3] || {}).depth);

  /* 20. 客户端绝不能自己驱动 AI（否则两端算出不同着法，棋盘分叉） */
  cli6.setAiPiece(2, 0, 0);
  cli6.setCurPlayer(2);
  await cli6.triggerAI();
  await sleep(500);
  ok(cli6.aiMoveCalls.length === 0, "客户端跑了 AI 计算（会与房主分叉）");
  ok(cli6.curPlayer === 2, "客户端的 AI 回合被本地推进了");

  /* 21. 房主驱动 AI 时必须把落子广播出去 */
  host6.setAiPiece(2, 0, 0);
  host6.setCurPlayer(2);
  const beforeClicks = cli6.localClicks.length;
  await host6.triggerAI();
  await sleep(600);
  await settle(all6);
  ok(host6.aiMoveCalls.length === 1, "房主没跑 AI 计算");
  ok(cli6.localClicks.length === beforeClicks + 1,
     "AI 的落子没广播到客户端（客户端会停在原地）");

  /* ═══ 场景四：公网 IPv6 与联机范围提示 ═══ */

  /* 22. 没有 IPv6 → 必须告诉用户「只能局域网联机」 */
  resetNet();
  const hostV6a = makeSandbox("host", null, 43220);
  hostV6a.setIpv6({ available: false, addr: null, raw: null,
    reason: "没有可用的 IPv6 出口（未连到支持 IPv6 的网络）" });
  await hostV6a.checkIpv6();
  ok(/只能/.test(hostV6a.netBodyHtml || ""), "无 IPv6 时没提示只能局域网联机");

  /* 23. 有 IPv6 → 显示地址、一键复制、说明可远程联机 */
  resetNet();
  const hostV6 = makeSandbox("host", null, 43221);
  const V6 = "2409:8a02:c22:99a0:3c62:c2ff:fefb:d93f";
  hostV6.setIpv6({ available: true, addr: V6, raw: V6, reason: null });
  await hostV6.checkIpv6();
  const html = hostV6.netBodyHtml || "";
  ok(/远程联机/.test(html), "有 IPv6 时没提示可以远程联机");
  ok(html.includes(V6), "有 IPv6 时没把地址显示出来");
  ok(/复制 IPv6/.test(html), "有 IPv6 时没有一键复制按钮");
  ok(/防火墙/.test(html), "没提醒入站可能被路由器防火墙拦（否则用户会误判）");

  /* 24. 建房后房间页给出可直连的 [v6]:port */
  await hostV6.lanCreateRoom();
  await settle([hostV6]);
  ok(hostV6.roomRowShown === true, "有 IPv6 建房后没显示公网地址行");
  ok(hostV6.roomV6Text === "[" + V6 + "]:43221",
     "公网地址格式不对（应为 [v6]:port）: " + hostV6.roomV6Text);

  /* 25. 无 IPv6 时房间页不显示公网地址行 */
  resetNet();
  const hostNoV6 = makeSandbox("host", null, 43222);
  hostNoV6.setIpv6({ available: false, addr: null, raw: null, reason: "无" });
  await hostNoV6.checkIpv6();
  await hostNoV6.lanCreateRoom();
  await settle([hostNoV6]);
  ok(hostNoV6.roomRowShown === false, "无 IPv6 时不该显示公网地址行");

  /* 25.5 旧版本：调用 lan_ipv6 会失败 → 必须说「版本过旧」，
     不能替一个根本没跑的检测下「只能局域网」的结论 */
  resetNet();
  const hostOld = makeSandbox("host", null, 43223);
  hostOld.setIpv6Missing(true);
  await hostOld.checkIpv6();
  const oldHtml = hostOld.netBodyHtml || "";
  ok(/版本过旧/.test(oldHtml), "旧版本应提示版本过旧，实际: " + oldHtml.slice(0, 120));
  ok(/4\.1\.0/.test(oldHtml), "旧版本提示里应写明需要哪个版本");

  /* 25.6 只有内网 IPv6（fd00::）→ 判否，且要把地址显出来便于自查 */
  resetNet();
  const hostPriv = makeSandbox("host", null, 43224);
  hostPriv.setIpv6({
    available: false, addr: null, raw: "fd00::1", code: "private_only",
    reason: "只有内网 IPv6（fd00:: / fe80::），无法跨网络访问",
  });
  await hostPriv.checkIpv6();
  const privHtml = hostPriv.netBodyHtml || "";
  ok(/只能/.test(privHtml), "内网 IPv6 应提示只能局域网");
  ok(privHtml.includes("fd00::1"), "内网地址应显示出来，便于用户判断");
  ok(/内网段/.test(privHtml), "应说明该地址属于内网段");

  /* 25.7 有公网 IPv6 但监听没覆盖（系统绑不上 ::）→ 绝不能给出地址，
     否则用户复制出去一个「合法但没人监听」的端点，永远连不上 */
  resetNet();
  const hostNoListen = makeSandbox("host", null, 43225);
  hostNoListen.setHostIpv6(false);
  const V6X = "2409:8a02:c22:99a0:e13b:2795:2e46:70c0";
  hostNoListen.setIpv6({ available: true, addr: V6X, raw: V6X, code: "ok" });
  await hostNoListen.checkIpv6();
  await hostNoListen.lanCreateRoom();
  await settle([hostNoListen]);
  const msg = hostNoListen.roomV6Text || "";
  ok(msg.indexOf(V6X) === -1, "监听不含 IPv6 时不该显示公网地址: " + msg);
  ok(/未在 IPv6 上监听/.test(msg), "监听不含 IPv6 时应明确说明，实际: " + msg);

  /* 26. 端口拼接：无端口时不能拼出垃圾 */
  ok(hostV6.v6Endpoint("2409::1", "") === "", "无端口时应返回空串");
  ok(hostV6.v6Endpoint("2409::1", "1234") === "[2409::1]:1234", "端点拼接不对");

  if (errs.length) {
    console.error("❌ 局域网联机自检失败：");
    for (const e of errs) console.error("   - " + e);
    process.exit(1);
  }
  console.log("✅ 局域网联机自检通过（建房 / 加入 / 准备 / 掉线 / 重编号开局 / 落子转发 / 动画排队 / 越界 / 名字清洗 / 房主规则同步 / 随机项解析 / 互斥纠正 / 房内人机 / 公网 IPv6 判定与复制）");
}

main().catch((e) => {
  console.error("❌ 自检脚本异常:", e);
  process.exit(1);
});
