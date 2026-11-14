#!/usr/bin/env node
// 局域网联机协议自检：把 public/app.js 里的联机模块抽出来，
// 用两个独立沙箱（主机 / 客户端）共享一个假网络跑一遍房间流程。
//
// 项目没有测试框架，这份脚本就是「logic 坏掉会失败」的最小检查：
// 座位分配、准备状态、开局配置、落子转发、掉线清理任何一处走偏都会红。
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

const errs = [];
function ok(cond, label) {
  if (!cond) errs.push(label);
}

/* ─── 假网络：一个主机 + 若干客户端 ─── */
const net = {
  peerId: 1,
  hostInbox: [],
  clientInbox: [],   // 单客户端场景够用
  closed: false,
};

function makeEl(id) {
  const el = {
    id,
    style: {},
    value: "",
    textContent: "",
    innerHTML: "",
    children: [],
    appendChild(c) { this.children.push(c); },
    onclick: null,
    className: "",
  };
  return el;
}

function makeSandbox(role) {
  const els = new Map();
  const document = {
    getElementById(id) {
      if (!els.has(id)) els.set(id, makeEl(id));
      return els.get(id);
    },
    createElement(tag) { return makeEl(tag); },
    body: { style: {}, classList: { add(){}, remove(){} } },
  };
  const messages = [];
  const localClicks = [];

  const invoke = async (cmd, args = {}) => {
    switch (cmd) {
      case "lan_host":
        return 43210;
      case "lan_local_ip":
        return "192.168.1.5";
      case "lan_join":
        if (!/^.+:\d+$/.test(args.addr)) throw new Error("bad addr");
        return null;
      case "lan_send":
        if (role === "host") {
          net.clientInbox.push({ type: "msg", id: null, data: args.msg });
        } else {
          net.hostInbox.push({ type: "msg", id: net.peerId, data: args.msg });
        }
        return null;
      case "lan_send_to":
        net.clientInbox.push({ type: "msg", id: args.id, data: args.msg });
        return null;
      case "lan_poll": {
        const q = role === "host" ? net.hostInbox : net.clientInbox;
        const out = q.splice(0, q.length);
        return out;
      }
      case "lan_stop":
        return null;
      case "lan_discover":
        return [];
      default:
        throw new Error("unexpected cmd: " + cmd);
    }
  };

  const env = {
    document,
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
  };

  const prelude = `
    const { document, window, localStorage, tauriInvoke, showMsg, Router, COLORS, COLORS_LIGHT,
            setInterval: mockSetInterval, clearInterval: mockClearInterval } = __env;
    const setInterval = mockSetInterval, clearInterval = mockClearInterval;
    let gameMode=null, curPlayer=0, gameOver=false, isPaused=false, animating=false, aiThinking=false;
    let board=[], size=7, cols=7, maxPlayers=2, boardCells=null, borderMode='default', capMode='4', firstMovePos=null;
    let _colorNames={}, _originPage=null;
    let aiPlayers=new Set(), aiConfigs={}, eliminatedPlayers=new Set();
    let eliminationRounds=[], eliminationInfo={}, gameHistory=[], chainStats={};
    let maxChainOverall={player:null,length:0}, undoStack=[];
    const mkBoard=(s,c)=>Array.from({length:s},()=>Array.from({length:c},()=>({owner:null,count:0})));
    const resetRoundHistory=()=>{gameHistory=[]};
    const clearSavedGameState=()=>{};
    const recordHistory=()=>{gameHistory.push({turn:gameHistory.length})};
    const renderBoard=()=>{}; const show=()=>{}; const renderPlayerBar=()=>{};
    const updateFastFinishBtn=()=>{}; const exitGame=()=>{ gameMode=null; };
    async function localClick(x,y,seed){ __env.localClicks.push({x,y,seed,curPlayer}); }
    const hasPieces=(p,b)=>b.some(r=>r.some(c=>c.owner===p));
    const isInRestrictedZone=()=>false;
  `;
  const epilogue = `
    return {
      get role(){return lanRole}, get myPid(){return lanMyPid},
      get players(){return lanPlayers}, get addr(){return lanRoomAddr},
      get gameMode(){return gameMode}, get curPlayer(){return curPlayer},
      get maxPlayers(){return maxPlayers}, get names(){return _colorNames},
      lanCreateRoom, lanJoinRoom, lanHostStart, lanToggleReady, lanDrainEvents,
      lanSendMove, lanLeaveRoom,
      setCurPlayer:(p)=>{curPlayer=p},
      setGameMode:(m)=>{gameMode=m},
      localClicks: __env.localClicks,
      messages: __env.messages,
    };
  `;
  const fn = new Function("__env", prelude + code + epilogue);
  return fn(env);
}

async function main() {
  const host = makeSandbox("host");
  const client = makeSandbox("client");

  /* 1. 主机建房 */
  await host.lanCreateRoom();
  ok(host.role === "host", "主机角色没设上");
  ok(host.addr === "192.168.1.5:43210", "房间地址不对: " + host.addr);
  ok(host.players.filter(Boolean).length === 1, "房主自己没在玩家列表里");

  /* 2. 客户端加入（hello → welcome/roster） */
  await client.lanJoinRoom("192.168.1.5:43210");
  ok(client.role === "client", "客户端角色没设上");
  await host.lanDrainEvents();     // host 收 hello，分配座位
  await client.lanDrainEvents();   // client 收 welcome + roster
  ok(client.myPid === 1, "客户端没拿到座位号: " + client.myPid);
  ok(host.players.filter(Boolean).length === 2, "主机没记上第二位玩家");
  ok(client.players.filter(Boolean).length === 2, "客户端玩家列表不完整");

  /* 3. 准备状态 */
  client.lanToggleReady();
  await host.lanDrainEvents();
  await client.lanDrainEvents();
  ok(host.players[1] && host.players[1].ready === true, "准备状态没传到主机");

  /* 4. 开局 */
  host.lanHostStart();
  await client.lanDrainEvents();
  ok(host.gameMode === "lan" && client.gameMode === "lan", "开局后没有进入 lan 模式");
  ok(host.maxPlayers === 2 && client.maxPlayers === 2, "人数不一致");
  ok(host.names[1] && host.names[1] === client.names[1], "玩家名两端不一致");

  /* 5. 客户端落子 → 主机校验并应用；发起者不收回自己那一份 */
  host.setCurPlayer(1);
  client.setCurPlayer(1);
  client.lanSendMove(2, 3, 777);   // 客户端落子 → 发给主机
  await host.lanDrainEvents();
  ok(host.localClicks.length === 1, "主机没应用客户端的落子");
  ok(host.localClicks[0].x === 2 && host.localClicks[0].y === 3, "落子坐标传丢了");
  ok(host.localClicks[0].seed === 777, "随机种子传丢了（会导致两边棋盘不一致）");

  /* 6. 主机落子广播 → 客户端按 seed 应用 */
  net.clientInbox.length = 0;
  host.setCurPlayer(0);
  client.setCurPlayer(0);
  host.lanSendMove(4, 5, 12345);
  await client.lanDrainEvents();
  ok(client.localClicks.length === 1, "客户端没应用主机的落子");
  ok(client.localClicks[0].seed === 12345, "主机落子的种子丢了");

  /* 7. 轮到谁才准落子：客户端收到不属于当前回合的 move 应忽略 */
  client.setCurPlayer(1);
  net.clientInbox.push({ type: "msg", id: null, data: { t: "move", player: 0, x: 0, y: 0, seed: 1 } });
  await client.lanDrainEvents();
  ok(client.localClicks.length === 1, "非当前回合的 move 被错误应用了");

  /* 7.5 越界 / 非法坐标不能把两端搞崩，也不能被应用 */
  net.clientInbox.push({ type: "msg", id: null, data: { t: "move", player: 1, x: -1, y: 999, seed: 2 } });
  await client.lanDrainEvents();
  ok(client.localClicks.length === 1, "客户端的越界坐标被错误应用了");

  host.setCurPlayer(1);   // 让越界检查真正被执行到（不是被回合校验提前挡掉）
  net.hostInbox.push({ type: "msg", id: net.peerId, data: { t: "move", x: -1, y: 999, seed: 3 } });
  await host.lanDrainEvents();
  ok(host.localClicks.length === 1, "主机的越界坐标被错误应用了");

  /* 8. 掉线：主机把该座位清掉 */
  net.hostInbox.push({ type: "peer", event: "leave", id: net.peerId });
  await host.lanDrainEvents();
  ok(host.players.filter(Boolean).length === 1, "掉线玩家的座位没清理");

  if (errs.length) {
    console.error("❌ 局域网联机自检失败：");
    for (const e of errs) console.error("   - " + e);
    process.exit(1);
  }
  console.log("✅ 局域网联机自检通过（建房 / 加入 / 准备 / 开局 / 落子转发 / 掉线）");
}

main().catch((e) => {
  console.error("❌ 自检脚本异常:", e);
  process.exit(1);
});
