/* engine.js — 连锁棋 PWA 引擎桥接层
 *
 * 在浏览器环境模拟 Tauri 的 invoke() 语义：
 *  - 游戏引擎命令（process_move / ai_move* / simulate_to_end）→ WASM 引擎
 *  - 存储命令（settings / history / round history / game state）→ IndexedDB（不可用时降级 localStorage）
 *  - 导出对话框 → 浏览器 Blob 下载
 *  - 更新 / 退出命令 → 降级为无操作
 *
 * 全局暴露 window.ChainEngine = { isTauri, ready, webInvoke }。
 * app.js 的 tauriInvoke() 在非 Tauri 环境下会转调 webInvoke。
 */
(function () {
  'use strict';

  const LS_PREFIX = 'chainchess:';
  const HISTORY_KEY = 'history';
  const ROUND_KEY = 'roundHistory';
  const SETTINGS_KEY = 'settings';
  const GAME_STATE_KEY = 'gameState';

  // ── 存储层常量 ──
  const DB_NAME = 'chainchess';
  const DB_VERSION = 1;
  const STORE_NAME = 'kv';
  const OPEN_TIMEOUT_MS = 6000;   // indexedDB.open 迟迟不回调时不阻塞应用

  // ══ 存储层：IndexedDB 优先，localStorage 降级 ══
  //
  // 对外仍是 lsGet / lsSet / lsDel 三个名字（内部异步，返回 Promise）。
  // 值在 IndexedDB 里统一包一层 { v: val }，以区分「存在的 undefined」与「键不存在」。
  // 打开失败 / 事务失败 / 被浏览器禁用（Safari 隐私模式等）时整体退回 localStorage，
  // 并通过 legacyFallbackKeys 记录「该键的权威数据仍在 localStorage」，
  // 保证迁移失败的键不会因为读不到而丢失。
  const LEGACY_READ_FAILED = { __legacyReadFailed: true };
  const legacyFallbackKeys = Object.create(null);

  function safeParse(raw, fallback) {
    if (raw === null || raw === undefined) return fallback;
    try {
      return JSON.parse(raw);
    } catch (e) {
      return fallback;
    }
  }

  // — 底层：localStorage —
  function legacyGetRaw(key) {
    try {
      return localStorage.getItem(LS_PREFIX + key);
    } catch (e) {
      return null;
    }
  }
  function legacySetRaw(key, json) {
    try {
      localStorage.setItem(LS_PREFIX + key, json);
      return true;
    } catch (e) {
      console.warn('[engine.js] localStorage 写入失败:', e);
      return false;
    }
  }
  function legacyDelRaw(key) {
    try {
      localStorage.removeItem(LS_PREFIX + key);
    } catch (e) { /* ignore */ }
  }
  function collectLegacyKeys() {
    const keys = [];
    try {
      for (let i = 0; i < localStorage.length; i++) {
        const full = localStorage.key(i);
        if (full && full.indexOf(LS_PREFIX) === 0) keys.push(full.slice(LS_PREFIX.length));
      }
    } catch (e) { /* ignore */ }
    return keys;
  }

  // — 底层：IndexedDB —
  function openDatabase() {
    return new Promise(function (resolve) {
      let settled = false;
      function giveUp(why) {
        if (settled) return;
        settled = true;
        console.warn('[engine.js] IndexedDB 不可用，退回 localStorage:', why);
        resolve(null);
      }
      const idb = window.indexedDB || window.mozIndexedDB || window.webkitIndexedDB;
      if (!idb) { giveUp('缺少 indexedDB'); return; }

      let req;
      try {
        req = idb.open(DB_NAME, DB_VERSION);
      } catch (e) {
        giveUp(e && e.message ? e.message : e);
        return;
      }
      setTimeout(function () {
        if (settled) return;
        settled = true;
        try { if (req.result) req.result.close(); } catch (e) { /* ignore */ }
        console.warn('[engine.js] IndexedDB 打开超时，退回 localStorage');
        resolve(null);
      }, OPEN_TIMEOUT_MS);

      req.onupgradeneeded = function () {
        const db = req.result;
        try {
          if (!db.objectStoreNames.contains(STORE_NAME)) db.createObjectStore(STORE_NAME);
        } catch (e) {
          console.warn('[engine.js] 创建 object store 失败:', e);
        }
      };
      req.onsuccess = function () {
        if (settled) { try { req.result.close(); } catch (e) { /* ignore */ } return; }
        settled = true;
        resolve(req.result);
      };
      req.onerror = function () {
        giveUp(req.error && req.error.message ? req.error.message : 'indexedDB.open 失败');
      };
      req.onblocked = function () {
        giveUp('indexedDB.open 被其他标签页阻塞');
      };
    });
  }

  function idbGet(db, key) {
    return new Promise(function (resolve, reject) {
      let tx;
      try {
        tx = db.transaction(STORE_NAME, 'readonly');
      } catch (e) { reject(e); return; }
      const req = tx.objectStore(STORE_NAME).get(key);
      req.onsuccess = function () { resolve(req.result); };
      req.onerror = function () { reject(req.error || new Error('IndexedDB 读取失败')); };
      tx.onabort = function () { reject(tx.error || new Error('IndexedDB 事务中止')); };
    });
  }

  function idbPut(db, key, val) {
    return new Promise(function (resolve, reject) {
      let tx;
      try {
        tx = db.transaction(STORE_NAME, 'readwrite');
      } catch (e) { reject(e); return; }
      const req = tx.objectStore(STORE_NAME).put(val, key);
      req.onsuccess = function () { resolve(); };
      req.onerror = function () { reject(req.error || new Error('IndexedDB 写入失败')); };
      tx.onabort = function () { reject(tx.error || new Error('IndexedDB 事务中止')); };
    });
  }

  function idbDel(db, key) {
    return new Promise(function (resolve, reject) {
      let tx;
      try {
        tx = db.transaction(STORE_NAME, 'readwrite');
      } catch (e) { reject(e); return; }
      const req = tx.objectStore(STORE_NAME).delete(key);
      req.onsuccess = function () { resolve(); };
      req.onerror = function () { reject(req.error || new Error('IndexedDB 删除失败')); };
      tx.onabort = function () { reject(tx.error || new Error('IndexedDB 事务中止')); };
    });
  }

  // — 一次性迁移：读旧 → 写新 → 读回校验 → 校验通过才删旧键 —
  // 任一环节失败都保留 localStorage 旧数据，并把该键标记为「仍以 localStorage 为准」。
  // 幂等：IndexedDB 里已有数据的键不覆盖，冗余旧键直接清理；旧键搬完即删，重复运行无操作。
  function migrateOneKey(db, key) {
    return idbGet(db, key).catch(function (e) {
      // 读不到新旧数据 => 无从判断，保持旧数据不动
      console.warn('[engine.js] 迁移读取失败，保留 localStorage 旧数据:', key, e && e.message ? e.message : e);
      return LEGACY_READ_FAILED;
    }).then(function (row) {
      if (row === LEGACY_READ_FAILED) { legacyFallbackKeys[key] = true; return; }

      const hasNewData = row !== undefined && row !== null && typeof row === 'object' && 'v' in row;
      if (hasNewData) {
        // IndexedDB 已有新数据：不覆盖，旧键只是冗余，清掉避免以后被误搬回
        legacyDelRaw(key);
        return;
      }

      const raw = legacyGetRaw(key);
      if (raw === null) return;                 // 没有旧数据
      let val;
      try {
        val = JSON.parse(raw);
      } catch (e) {
        console.warn('[engine.js] 旧数据不是合法 JSON，原样保留在 localStorage:', key);
        legacyFallbackKeys[key] = true;
        return;
      }

      const envelope = { v: val };
      return idbPut(db, key, envelope).then(function () {
        return idbGet(db, key);
      }).then(function (back) {
        const ok = back !== undefined && back !== null && typeof back === 'object' && 'v' in back &&
          JSON.stringify(back.v) === JSON.stringify(envelope.v);
        if (!ok) throw new Error('回读校验不一致');
        legacyDelRaw(key);                      // 校验通过才删旧键
        console.info('[engine.js] 已迁移旧数据到 IndexedDB:', key);
      }).catch(function (e) {
        legacyFallbackKeys[key] = true;          // 旧数据保留，并继续按旧数据读
        console.warn('[engine.js] 迁移失败，保留 localStorage 旧数据:', key, e && e.message ? e.message : e);
      });
    });
  }

  function migrateLegacy(db) {
    let chain = Promise.resolve();
    collectLegacyKeys().forEach(function (key) {
      chain = chain.then(function () { return migrateOneKey(db, key); });
    });
    return chain.catch(function (e) {
      console.warn('[engine.js] 迁移过程异常（旧数据保留）:', e && e.message ? e.message : e);
    });
  }

  // — 存储层初始化（只跑一次）：返回 Promise<IDBDatabase|null> —
  let storeReady = null;
  function initStore() {
    if (storeReady) return storeReady;
    storeReady = openDatabase().then(function (db) {
      if (!db) return null;
      // 探针事务：object store 不可用时直接整体降级，避免后续每次读写都失败
      return idbGet(db, '__probe__').then(function () {
        return migrateLegacy(db);
      }).then(function () {
        console.info('[engine.js] 存储层：IndexedDB');
        return db;
      });
    }).catch(function (e) {
      console.warn('[engine.js] 存储层初始化失败，退回 localStorage:', e && e.message ? e.message : e);
      return null;
    });
    return storeReady;
  }

  // — 对外存储 API（名字与调用方式保持 lsGet / lsSet / lsDel） —
  async function lsGet(key, fallback) {
    let db = null;
    try {
      db = await initStore();
    } catch (e) { /* initStore 内部已兜底 */ }
    if (db && !legacyFallbackKeys[key]) {
      try {
        const row = await idbGet(db, key);
        if (row !== undefined && row !== null && typeof row === 'object' && 'v' in row) return row.v;
        return fallback;
      } catch (e) {
        legacyFallbackKeys[key] = true;
        console.warn('[engine.js] IndexedDB 读取失败，该键退回 localStorage:', key, e && e.message ? e.message : e);
      }
    }
    return safeParse(legacyGetRaw(key), fallback);
  }

  async function lsSet(key, val) {
    let db = null;
    try {
      db = await initStore();
    } catch (e) { /* initStore 内部已兜底 */ }
    if (db) {
      try {
        await idbPut(db, key, { v: val });
        delete legacyFallbackKeys[key];         // 新数据已落 IndexedDB
        legacyDelRaw(key);                      // 清理可能残留在 localStorage 的旧键
        return;
      } catch (e) {
        console.warn('[engine.js] IndexedDB 写入失败，该键退回 localStorage:', key, e && e.message ? e.message : e);
      }
    }
    legacyFallbackKeys[key] = true;
    legacySetRaw(key, JSON.stringify(val));
  }

  async function lsDel(key) {
    let db = null;
    try {
      db = await initStore();
    } catch (e) { /* initStore 内部已兜底 */ }
    if (db) {
      try {
        await idbDel(db, key);
        delete legacyFallbackKeys[key];
      } catch (e) {
        legacyFallbackKeys[key] = true;         // 删不掉就别让旧值被读回来
      }
    }
    legacyDelRaw(key);
  }

  // ── 历史记录 ──
  async function loadHistory() {
    const list = await lsGet(HISTORY_KEY, []);
    return Array.isArray(list) ? list : [];
  }
  function persistHistory(list) {
    return lsSet(HISTORY_KEY, list);
  }
  // 读-改-写串行化：并发调用（例如快速连点保存）不会互相覆盖
  let historyChain = Promise.resolve();
  function mutateHistory(mutator) {
    const next = historyChain.then(function () {
      return loadHistory();
    }).then(function (list) {
      const result = mutator(list);
      return persistHistory(list).then(function () { return result; });
    });
    historyChain = next.catch(function () { /* 保持队列可用 */ });
    return next;
  }

  // ── WASM 引擎加载（动态 import，懒加载） ──
  let wasmModule = null;
  let wasmPromise = null;
  function ensureWasm() {
    if (wasmModule) return Promise.resolve(wasmModule);
    if (!wasmPromise) {
      wasmPromise = import('./pkg/chain_chess_engine.js')
        .then(async (m) => {
          await m.default();           // init（fetch wasm）
          wasmModule = m;
          return m;
        })
        .catch((e) => {
          wasmPromise = null;          // 允许下次重试
          throw new Error('WASM 引擎加载失败: ' + (e && e.message ? e.message : e));
        });
    }
    return wasmPromise;
  }

  // ── 引擎命令（WASM） ──
  async function engineInvoke(cmd, args) {
    const m = await ensureWasm();
    let json;
    if (cmd === 'process_move') {
      json = m.process_move_cmd(JSON.stringify(args || {}));
    } else if (cmd === 'simulate_to_end') {
      json = m.simulate_to_end_cmd(JSON.stringify(args || {}));
    } else if (cmd === 'bench_ai_game') {
      json = m.bench_ai_game_cmd(JSON.stringify(args || {}));
    } else {
      // ai_move / ai_move_v2 / ai_move_mcts / ai_move_strategy
      const a = Object.assign({}, args);
      if (!a.algorithm) {
        a.algorithm = cmd === 'ai_move_mcts' ? 'mcts'
          : cmd === 'ai_move_v2' ? 'pvs'
          : cmd === 'ai_move' ? 'alphabeta'
          : 'strategy';
      }
      json = m.ai_move_cmd(JSON.stringify(a));
    }
    const r = JSON.parse(json);
    if (r.ok) return r.data;
    throw new Error(r.error || '引擎执行失败');
  }

  // ── 导出：浏览器下载 JSON 文件 ──
  function downloadJson(jsonData) {
    const blob = new Blob([jsonData], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    const ts = new Date().toISOString().slice(0, 19).replace(/[:T]/g, '-');
    a.href = url;
    a.download = 'chain-chess-history-' + ts + '.json';
    document.body.appendChild(a);
    a.click();
    a.remove();
    setTimeout(function () { URL.revokeObjectURL(url); }, 2000);
  }

  // ── webInvoke：命令分发（模拟 Tauri invoke 语义：失败 reject） ──
  async function webInvoke(cmd, args) {
    args = args || {};
    switch (cmd) {
      // 引擎命令
      case 'process_move':
      case 'ai_move':
      case 'ai_move_v2':
      case 'ai_move_mcts':
      case 'ai_move_strategy':
      case 'simulate_to_end':
        return engineInvoke(cmd, args);

      // 设备性能检测：AI 单局基准（前端传 {config, gameCount}，扁平化后转 WASM；
      // wasm32 无标准时钟，由 JS 端 performance.now() 计时）
      case 'bench_ai_game': {
        const flat = Object.assign({}, args.config || {}, { gameCount: args.gameCount });
        const t0 = performance.now();
        return engineInvoke('bench_ai_game', flat).then(function (data) {
          data.elapsedMs = performance.now() - t0;
          return data;
        });
      }

      // 设置
      case 'load_settings':
        return lsGet(SETTINGS_KEY, null);
      case 'save_settings':
        await lsSet(SETTINGS_KEY, args.settings);
        return undefined;

      // 历史记录
      case 'load_game_history':
        return loadHistory();
      case 'save_game_history':
        return mutateHistory(function (list) {
          list.push(args.record);
          return undefined;
        });
      case 'delete_game_history_record': {
        const id = args.recordId;
        return mutateHistory(function (list) {
          for (let i = list.length - 1; i >= 0; i--) {
            if (list[i] && list[i].id === id) list.splice(i, 1);
          }
          return undefined;
        });
      }
      case 'delete_game_history_records': {
        const ids = new Set(args.recordIds || []);
        return mutateHistory(function (list) {
          for (let i = list.length - 1; i >= 0; i--) {
            if (list[i] && ids.has(list[i].id)) list.splice(i, 1);
          }
          return undefined;
        });
      }
      case 'clear_game_history':
        await lsDel(HISTORY_KEY);
        return undefined;
      case 'import_game_history': {
        let arr;
        try {
          arr = JSON.parse(args.jsonData);
        } catch (e) {
          throw new Error('JSON 解析失败: ' + e.message);
        }
        if (!Array.isArray(arr)) throw new Error('导入数据不是数组');
        return mutateHistory(function (list) {
          const known = new Set(list.map(function (r) { return r.id; }));
          let added = 0;
          for (const rec of arr) {
            if (rec && typeof rec.id === 'number' && !known.has(rec.id)) {
              list.push(rec);
              known.add(rec.id);
              added++;
            }
          }
          return added;
        });
      }
      case 'export_game_history_dialog': {
        downloadJson(args.jsonData);
        const bytes = new Blob([args.jsonData]).size;
        return 'fallback:' + bytes;   // 与 Tauri 端 fallback 返回格式一致
      }

      // 回合历史（溢出存储）
      case 'load_round_history': {
        const v = await lsGet(ROUND_KEY, []);
        return Array.isArray(v) ? v : [];
      }
      case 'save_round_history':
        await lsSet(ROUND_KEY, args.data || []);
        return undefined;
      case 'clear_round_history':
        await lsDel(ROUND_KEY);
        return undefined;

      // 未完成游戏存档
      case 'load_game_state': {
        const v = await lsGet(GAME_STATE_KEY, '');
        return typeof v === 'string' ? v : '';
      }
      case 'save_game_state':
        await lsSet(GAME_STATE_KEY, args.stateJson);
        return undefined;
      case 'clear_game_state':
        await lsDel(GAME_STATE_KEY);
        return undefined;

      // 无操作命令
      case 'exit_app':
        return undefined;

      default:
        console.warn('[engine.js] 未实现的命令:', cmd);
        return undefined;
    }
  }

  // 预加载 wasm：附加消费链记录失败警告，避免未处理的 rejected promise（unhandledrejection）。
  // ready 保留原始 promise（resolve 为模块）；实际调用走 ensureWasm()，失败会重置 wasmPromise 允许重试。
  const wasmReady = ensureWasm();
  wasmReady.catch((e) => {
    console.warn('[engine.js] WASM 预加载失败（首次使用时将重试）:', e && e.message ? e.message : e);
  });

  // 存储层预热：启动时就打开 IndexedDB 并完成一次性迁移（迁移失败不影响应用启动）
  initStore().catch(function (e) {
    console.warn('[engine.js] 存储层预热异常:', e && e.message ? e.message : e);
  });

  window.ChainEngine = {
    isTauri: !!(window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke),
    ready: wasmReady,   // 预加载 wasm
    webInvoke: webInvoke
  };

  // ══ Service Worker：注册 + 原地更新（不再跳下载中心） ══
  //
  // sw.js 安装新版后不再无条件 skipWaiting，而是停在 waiting 状态等页面确认。
  // 这里发现 waiting worker 就弹提示条；用户点击才 postMessage({type:'SKIP_WAITING'})，
  // 新 worker 激活触发 controllerchange 后再 reload —— 用户始终停留在应用内。
  let updateRegistration = null;
  let updateBarEl = null;
  let reloadingForUpdate = false;

  function hideUpdateBar() {
    if (updateBarEl && updateBarEl.parentNode) updateBarEl.parentNode.removeChild(updateBarEl);
    updateBarEl = null;
  }

  function applyUpdate() {
    if (reloadingForUpdate) return;
    reloadingForUpdate = true;
    hideUpdateBar();
    navigator.serviceWorker.addEventListener('controllerchange', function () {
      location.reload();
    });
    const waiting = updateRegistration && updateRegistration.waiting;
    if (waiting) {
      waiting.postMessage({ type: 'SKIP_WAITING' });
      // 兜底：若干秒内没等到 controllerchange 也刷新一次，避免卡在旧版本
      setTimeout(function () { location.reload(); }, 5000);
    } else {
      location.reload();   // 没有 waiting worker（例如已被其它标签页激活），直接刷新即可
    }
  }

  function showUpdateBar(registration) {
    updateRegistration = registration || updateRegistration;
    if (updateBarEl || !document.body) return;

    const bar = document.createElement('div');
    bar.setAttribute('role', 'button');
    bar.setAttribute('tabindex', '0');
    bar.setAttribute('aria-label', '有新版本，点击刷新');
    bar.style.cssText = [
      'position:fixed',
      'left:50%',
      'bottom:calc(env(safe-area-inset-bottom, 0px) + 12px)',
      'transform:translateX(-50%)',
      'z-index:2147483000',
      'display:flex',
      'align-items:center',
      'gap:10px',
      'max-width:calc(100vw - 24px)',
      'padding:9px 10px 9px 14px',
      'border-radius:999px',
      'border:1px solid var(--glass-border, rgba(255,255,255,.12))',
      'background:var(--surface, rgba(26,26,34,.85))',
      'backdrop-filter:blur(var(--liquid-blur, 24px))',
      '-webkit-backdrop-filter:blur(24px)',
      'color:var(--text, #e8e6e3)',
      'font:600 var(--text-sm, .78rem)/1.4 var(--font, system-ui, -apple-system, sans-serif)',
      'box-shadow:var(--shadow, 0 8px 40px rgba(0,0,0,.5))',
      'cursor:pointer',
      'user-select:none',
      '-webkit-user-select:none'
    ].join(';');

    const dot = document.createElement('span');
    dot.style.cssText = 'flex:none;width:7px;height:7px;border-radius:50%;background:var(--accent, #f0b34b);box-shadow:0 0 8px var(--accent, #f0b34b)';

    const label = document.createElement('span');
    label.textContent = '有新版本，点击刷新';
    label.style.cssText = 'white-space:nowrap;pointer-events:none';

    const close = document.createElement('button');
    close.type = 'button';
    close.textContent = '✕';
    close.setAttribute('aria-label', '关闭更新提示');
    close.style.cssText = [
      'flex:none',
      'width:22px',
      'height:22px',
      'padding:0',
      'border:0',
      'border-radius:50%',
      'background:var(--glass, rgba(255,255,255,.06))',
      'color:var(--dim, #7a7885)',
      'font:400 11px/1 var(--font, system-ui, sans-serif)',
      'cursor:pointer'
    ].join(';');
    close.addEventListener('click', function (ev) {
      ev.stopPropagation();
      hideUpdateBar();
    });

    bar.appendChild(dot);
    bar.appendChild(label);
    bar.appendChild(close);
    bar.addEventListener('click', applyUpdate);
    bar.addEventListener('keydown', function (ev) {
      if (ev.key === 'Enter' || ev.key === ' ') {
        ev.preventDefault();
        applyUpdate();
      }
    });

    document.body.appendChild(bar);
    updateBarEl = bar;
  }

  function watchRegistration(registration) {
    updateRegistration = registration;
    // 注册返回时已经有 waiting worker（例如页面开着时后台已完成更新）
    if (registration.waiting) showUpdateBar(registration);
    registration.addEventListener('updatefound', function () {
      const installing = registration.installing;
      if (!installing) return;
      installing.addEventListener('statechange', function () {
        // installed 且页面已被旧 SW 控制 ⇒ 这是「新版本在等待」，不是首次安装
        if (installing.state === 'installed' && navigator.serviceWorker.controller) {
          showUpdateBar(registration);
        }
      });
    });
  }

  // ── Service Worker 注册（仅 HTTPS 或 localhost 下生效） ──
  if ('serviceWorker' in navigator && location.protocol !== 'file:') {
    window.addEventListener('load', function () {
      navigator.serviceWorker.register('./sw.js').then(function (registration) {
        watchRegistration(registration);
      }).catch(function (e) {
        console.warn('[engine.js] Service Worker 注册失败:', e);
      });
    });
  }
})();
