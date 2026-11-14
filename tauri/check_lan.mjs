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

function clientInbox(peerId) {
  const c = net.clients.get(peerId);
  return c ? c.inbox : [];
}

function makeEl(id) {
  return {
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
}

function makeSandbox(role, peerId = null, port = 43210) {
  const els = new Map();
  const document = {
    getElementById(id) {
      if (!els.has(id)) els.set(id, makeEl(id));
      return els.get(id);
    },
    createElement(tag) { return makeEl(tag); },
    createTextNode(t) { return { nodeValue: t }; },
    body: { style: {}, classList: { add() {}, remove() {} } },
  };
  const messages = [];
  const localClicks = [];
  const sent = [];
  // 每个客户端一条独立的收件队列，按 peerId 注册到假网络
  const inbox = [];
  if (role === "client") net.clients.set(peerId, { inbox });

  const invoke = async (cmd, args = {}) => {
    switch (cmd) {
      case "lan_host":
        return port;
      case "lan_local_ip":
        return "192.168.1.5";
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
    const hasPieces=(p,b)=>b.some(r=>r.some(c=>c.owner===p));
    const isInRestrictedZone=()=>false;
    async function localClick(x,y,seed){
      if(gameOver || animating) return;             // 与真实实现同样的门禁
      __env.localClicks.push({x,y,seed,curPlayer});
      if(board[x] && board[x][y]) board[x][y].owner = curPlayer;
      curPlayer = (curPlayer + 1) % maxPlayers;
    }
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
      lanCreateRoom, lanJoinRoom, lanHostStart, lanToggleReady, lanDrainEvents,
      lanSendMove, lanLeaveRoom, lanApplyRemote,
      setCurPlayer:(p)=>{curPlayer=p},
      setGameMode:(m)=>{gameMode=m},
      localClicks: __env.localClicks,
      messages: __env.messages,
      ...__set,
    };
  `;
  const fn = new Function("__env", prelude + code + epilogue);
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

  if (errs.length) {
    console.error("❌ 局域网联机自检失败：");
    for (const e of errs) console.error("   - " + e);
    process.exit(1);
  }
  console.log("✅ 局域网联机自检通过（建房 / 加入 / 准备 / 掉线 / 重编号开局 / 落子转发 / 动画排队 / 越界 / 名字清洗）");
}

main().catch((e) => {
  console.error("❌ 自检脚本异常:", e);
  process.exit(1);
});
