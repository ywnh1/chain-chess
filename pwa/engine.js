/* engine.js — 连锁棋 PWA 引擎桥接层
 *
 * 在浏览器环境模拟 Tauri 的 invoke() 语义：
 *  - 游戏引擎命令（process_move / ai_move* / simulate_to_end）→ WASM 引擎
 *  - 存储命令（settings / history / round history / game state）→ localStorage
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

  // ── localStorage 工具 ──
  function lsGet(key, fallback) {
    try {
      const v = localStorage.getItem(LS_PREFIX + key);
      return v === null ? fallback : JSON.parse(v);
    } catch (e) {
      return fallback;
    }
  }
  function lsSet(key, val) {
    try {
      localStorage.setItem(LS_PREFIX + key, JSON.stringify(val));
    } catch (e) {
      console.warn('[engine.js] localStorage 写入失败:', e);
    }
  }
  function lsDel(key) {
    try {
      localStorage.removeItem(LS_PREFIX + key);
    } catch (e) { /* ignore */ }
  }

  // ── 历史记录 ──
  function loadHistory() {
    const list = lsGet(HISTORY_KEY, []);
    return Array.isArray(list) ? list : [];
  }
  function persistHistory(list) {
    lsSet(HISTORY_KEY, list);
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

      // 设置
      case 'load_settings':
        return lsGet(SETTINGS_KEY, null);
      case 'save_settings':
        lsSet(SETTINGS_KEY, args.settings);
        return undefined;

      // 历史记录
      case 'load_game_history':
        return loadHistory();
      case 'save_game_history': {
        const list = loadHistory();
        list.push(args.record);
        persistHistory(list);
        return undefined;
      }
      case 'delete_game_history_record': {
        const id = args.recordId;
        persistHistory(loadHistory().filter(function (r) { return r.id !== id; }));
        return undefined;
      }
      case 'delete_game_history_records': {
        const ids = new Set(args.recordIds || []);
        persistHistory(loadHistory().filter(function (r) { return !ids.has(r.id); }));
        return undefined;
      }
      case 'clear_game_history':
        lsDel(HISTORY_KEY);
        return undefined;
      case 'import_game_history': {
        let arr;
        try {
          arr = JSON.parse(args.jsonData);
        } catch (e) {
          throw new Error('JSON 解析失败: ' + e.message);
        }
        if (!Array.isArray(arr)) throw new Error('导入数据不是数组');
        const list = loadHistory();
        const known = new Set(list.map(function (r) { return r.id; }));
        let added = 0;
        for (const rec of arr) {
          if (rec && typeof rec.id === 'number' && !known.has(rec.id)) {
            list.push(rec);
            known.add(rec.id);
            added++;
          }
        }
        persistHistory(list);
        return added;
      }
      case 'export_game_history_dialog': {
        downloadJson(args.jsonData);
        const bytes = new Blob([args.jsonData]).size;
        return 'fallback:' + bytes;   // 与 Tauri 端 fallback 返回格式一致
      }

      // 回合历史（溢出存储）
      case 'load_round_history': {
        const v = lsGet(ROUND_KEY, []);
        return Array.isArray(v) ? v : [];
      }
      case 'save_round_history':
        lsSet(ROUND_KEY, args.data || []);
        return undefined;
      case 'clear_round_history':
        lsDel(ROUND_KEY);
        return undefined;

      // 未完成游戏存档
      case 'load_game_state': {
        const v = lsGet(GAME_STATE_KEY, '');
        return typeof v === 'string' ? v : '';
      }
      case 'save_game_state':
        lsSet(GAME_STATE_KEY, args.stateJson);
        return undefined;
      case 'clear_game_state':
        lsDel(GAME_STATE_KEY);
        return undefined;

      // 无操作命令
      case 'exit_app':
        return undefined;

      default:
        console.warn('[engine.js] 未实现的命令:', cmd);
        return undefined;
    }
  }

  window.ChainEngine = {
    isTauri: !!(window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke),
    ready: ensureWasm(),   // 预加载 wasm
    webInvoke: webInvoke
  };

  // ── Service Worker 注册（仅 HTTPS 或 localhost 下生效） ──
  if ('serviceWorker' in navigator && location.protocol !== 'file:') {
    window.addEventListener('load', function () {
      navigator.serviceWorker.register('./sw.js').catch(function (e) {
        console.warn('[engine.js] Service Worker 注册失败:', e);
      });
    });
  }
})();
