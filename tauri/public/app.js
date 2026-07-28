/* ═══════ 页面初始化：确保暂停层隐藏、欢迎页显示 ═══════ */
(function(){
  // 防止 Tauri Android WebView 恢复旧状态导致暂停层误显示
  try{
    var _po=document.getElementById('pauseOverlay');
    if(_po)_po.style.display='none';
  }catch(e){}
  // 确保只有 welcome 页面 active
  try{
    document.querySelectorAll('.screen').forEach(function(s){s.classList.remove('active')});
    var _w=document.getElementById('welcome');
    if(_w)_w.classList.add('active');
  }catch(e){}
})();

/* ═══════ 跟随系统颜色模式（仅读取 prefers-color-scheme） ═══════ */
(function(){
  const meta=document.getElementById('metaThemeColor');
  const mq=window.matchMedia('(prefers-color-scheme: light)');
  function sync(){meta.content=mq.matches?'#eae8e0':'#0f0f13'}
  sync();
  mq.addEventListener('change',sync);
})();

/* ==================== CONSTANTS ==================== */
const COLORS=['#E74C3C','#F1C40F','#3498DB','#2ECC71','#9B59B6','#E91E63','#1ABC9C','#F39C12','#8B5E3C','#5D6D7E'];
const COLORS_LIGHT=['rgba(231,76,60,.08)','rgba(241,196,15,.08)','rgba(52,152,219,.08)','rgba(46,204,113,.08)','rgba(155,89,182,.08)','rgba(233,30,99,.08)','rgba(26,188,156,.08)','rgba(243,156,18,.08)','rgba(139,94,60,.08)','rgba(93,109,126,.08)'];
const COLOR_NAMES = ['红色','黄色','蓝色','绿色','紫色','粉色','青色','橙色','棕色','深灰色'];
// 根据棋盘大小返回最大允许玩家人数
function getMaxPlayersBySize(boardSize){
  if(boardSize===5)return 5;
  if(boardSize===6)return 7;
  return 10;
}
// 双向联动：棋盘大小 ↔ 人数/AI数量，互相扣掉不合法的按钮

 
// ========== 音效 ==========
let audioCtx = null;
function getAudioCtx(){
  if(!audioCtx){
    try{audioCtx=new(window.AudioContext||window.webkitAudioContext)()}catch(e){}
  }
  return audioCtx;
}
function playTone(f,dur,type,vol){
  try{
    let ctx=getAudioCtx();if(!ctx)return;
    let osc=ctx.createOscillator(),gain=ctx.createGain();
    osc.type=type||'sine';
    osc.frequency.setValueAtTime(f,ctx.currentTime);
    gain.gain.setValueAtTime(vol||0.15,ctx.currentTime);
    gain.gain.exponentialRampToValueAtTime(0.001,ctx.currentTime+dur);
    osc.connect(gain);gain.connect(ctx.destination);
    osc.start();osc.stop(ctx.currentTime+dur);
  }catch(e){}
}
function playClick(){playTone(800,0.08,'sine',0.12);vibrate(12)}
function playExplosion(){playTone(150,0.25,'sawtooth',0.15);vibrate(25)}
function playElim(){
  playTone(400,0.15,'square',0.08);
  setTimeout(()=>playTone(300,0.15,'square',0.08),120);
  setTimeout(()=>playTone(200,0.2,'square',0.08),240);
  vibrate([40,30,50]);
}
function playGameOver(){
  playTone(523,0.15,'sine',0.12);
  setTimeout(()=>playTone(659,0.15,'sine',0.12),150);
  setTimeout(()=>playTone(784,0.3,'sine',0.15),300);
  vibrate([80,40,80,40,100]);
}
// ─── 震动反馈（调用系统 API，Android WebView 原生支持） ───
// 不可用环境（桌面、iOS）静默降级
function vibrate(pattern){
  try{if(navigator.vibrate)navigator.vibrate(pattern)}catch(e){}
}

/* ==================== STATE ==================== */
let board=[],curPlayer=0,size=7,maxPlayers=2;
let cells=[];
let gameMode=null; // 'ai'|'local'|'eve'
let _originPage=null; // 游戏从哪个 lobby 页面发起（用于「再来一局」返回）
let gameOver=false;
let aiPlayers=new Set();
let aiThinking=false;
let aiAlgorithm='strategy';
let aiDepth=2;
let aiConfigs={}; // per-AI configs for eve mode
let gameCount=0;
let eliminatedPlayers=new Set();
let gameHistory=[];
let chainStats={};
let maxChainOverall={player:null,length:0};

// 连炸跳过标志
let chainSkipAll=false;
// 自动跳过连爆动画（持久开关，true=跳过所有连爆动画，false=行为不变）
let autoSkipChain=false;
 
// 首子落位（用于限制其他玩家落子）
let firstMovePos=null;

// 暂停状态
let isPaused=false;

// 悔棋状态栈
let undoStack=[];

// 图表上下文缓存（用于全屏重绘）
let _chartCtx={};

// 最后游戏配置（用于「再来一局」直接重开，不再读 DOM）
let _lastGameConfig=null;

/* ═══════════════ 自定义控件事件绑定（文档级委托） ═══════════════ */
// 通用：获取自定义控件的选中值
function getSel(containerId){
  const c=document.getElementById(containerId);
  if(!c)return 7;
  const s=c.querySelector('.selected');
  return s?parseInt(s.dataset.value):7;
}
function getSelStr(containerId){
  const c=document.getElementById(containerId);
  if(!c)return 'strategy';
  const s=c.querySelector('.selected');
  return s?s.dataset.value:'strategy';
}
// 设置选中状态
function setSelected(container, target){
  const sel = target.matches('.size-btn,.gb') ? '.size-btn,.gb' : '.' + target.className.split(' ')[0];
  container.querySelectorAll(sel).forEach(b=>b.classList.remove('selected'));
  target.classList.add('selected');
}
// 文档级委托：处理所有自定义控件点击（兼容 display:none 内的元素）
document.addEventListener('click',function(e){
  const t=e.target.closest('.size-btn,.gb');
  if(!t)return;
  // 查找最近的自定义控件容器
  const container=t.closest('.size-grid,.btn-group');
  if(!container)return;
  setSelected(container, t);
  // 触发副作用
  if(container.id==='setupSizeGrid'||container.id==='setupPlayersGroup'){
    setTimeout(setupLobbySync,10);
  }else if(t.classList.contains('size-btn')||t.classList.contains('gb'))setTimeout(setupLobbySync,10);
});


/* ==================== ROUTER ==================== */
const Router = {
  _registry: {},
  _current: null,
  _prev: null,

  register(id, config) {
    this._registry[id] = config;
  },

  navigate(id, ...args) {
    const page = this._registry[id];
    if (!page || this._current === id) return;

    const doSwitch = () => {
      // 按钮 active 状态清理
      try{document.activeElement?.blur()}catch(e){}
      // Leave current page
      if (this._current && this._registry[this._current]?.leave) {
        this._registry[this._current].leave();
      }

      // Update state
      this._prev = this._current;
      this._current = id;

      // Remove old leaving class, deactivate all, activate new
      document.querySelectorAll('.screen').forEach(e => e.classList.remove('leaving','active'));
      const el = document.getElementById(id);
      if (el) el.classList.add('active');

      // Enter new page
      if (page.enter) page.enter(...args);

      // Sync hash（双 pushState 保证侧滑时始终有历史条目可 pop）
      if (location.hash !== '#' + id) {
        history.pushState(null, '', '#' + id);
        history.pushState(null, '', '#' + id);
      }
    };

    // Animate leave → switch
    const currentEl = this._current ? document.getElementById(this._current) : null;
    if (currentEl) {
      currentEl.classList.add('leaving');
      currentEl.addEventListener('animationend', () => { doSwitch(); }, { once: true });
      // 超时保护
      setTimeout(() => { if (currentEl.classList.contains('leaving')) doSwitch(); }, 400);
    } else {
      doSwitch();
    }
  },

  // 侧滑/返回导航专用（使用 replaceState，不会创建新历史条目）
  switchPage(id, ...args) {
    const page = this._registry[id];
    if (!page || this._current === id) return;

    const doSwitch = () => {
      try{document.activeElement?.blur()}catch(e){}
      if (this._current && this._registry[this._current]?.leave) {
        this._registry[this._current].leave();
      }
      this._prev = this._current;
      this._current = id;
      document.querySelectorAll('.screen').forEach(e => e.classList.remove('leaving','active'));
      const el = document.getElementById(id);
      if (el) el.classList.add('active');
      if (page.enter) page.enter(...args);
      if (location.hash !== '#' + id) {
        history.pushState(null, "", "#" + id);
        history.pushState(null, "", "#" + id);
      }
    };

    const currentEl = this._current ? document.getElementById(this._current) : null;
    if (currentEl) {
      currentEl.classList.add('leaving');
      currentEl.addEventListener('animationend', () => { doSwitch(); }, { once: true });
      setTimeout(() => { if (currentEl.classList.contains('leaving')) doSwitch(); }, 400);
    } else {
      doSwitch();
    }
  },

  back() {
    const page = this._registry[this._current];
    if (page && page.back) {
      this.navigate(typeof page.back === 'function' ? page.back() : page.back);
    } else {
      this.navigate('welcome');
    }
  },

  getPrev() { return this._prev; }
};

// ─── 页面注册 ───
Router.register('welcome', {
  back: null,
  enter() { document.body.style.background='' },
  leave() {}
});

Router.register('gameSetup', {
  back: 'welcome',
  enter() { document.body.style.background=''; setTimeout(setupLobbySync, 20); },
  leave() {}
});

Router.register('history', {
  back: 'welcome',
  enter() { document.body.style.background=''; loadHistoryList(); },
  leave() { if(_multiSelectActive)exitMultiSelect(); }
});




Router.register('game', {
  back: null,
  enter() {},
  leave() { document.body.style.background='' }
});

Router.register('about', {
  back: 'welcome',
  enter() { document.body.style.background=''; },
  leave() {}
});
Router.register('about-ai', {
  back: 'about',
  enter() { document.body.style.background=''; },
  leave() {}
});
Router.register('about-changelog', {
  back: 'about',
  enter() { document.body.style.background=''; renderChangelogCards(); },
  leave() { var cc=document.getElementById('changelogContainer');if(cc)cc.querySelectorAll('.cl-card').forEach(function(e){e.remove()}); }
});
Router.register('about-license', {
  back: 'about',
  enter() { document.body.style.background=''; },
  leave() {}
});

Router.register('checkout', {
  back() { return Router._checkoutPrev || 'welcome'; },
  enter(prevPage) { document.body.style.background=''; Router._checkoutPrev = prevPage || 'welcome'; },
  leave() { document.getElementById('checkoutContent').innerHTML = ''; document.getElementById('chkOuterTitle').textContent=''; },
  _checkoutPrev: 'welcome'
});

// ─── 兼容旧函数（逐步迁移） ───
function show(s) { Router.navigate(s); }
function goHome() {
  document.querySelectorAll('.settlement').forEach(e => { if (e.id !== 'pauseOverlay') e.remove() });
  document.getElementById('pauseOverlay').style.display = 'none';
  Router.navigate('welcome');
}
function exitApp(){
  try { window.__TAURI_INTERNALS__?.invoke('exit_app').catch(e=>logWarn('Exit app failed:', e)); } catch(e){logWarn('Exit app error:', e)}
  exitGame();
}
function showExitConfirm(){
  openModal('exitConfirm');
}
function confirmExitApp(){
  closeModal('exitConfirm');
  exitApp();
}


function showHistory(){ Router.navigate('history'); }
// Tauri invoke helper
/* debug: set DEBUG=false to silence all console */
const DEBUG = true;
function logWarn(){if(DEBUG)console.warn.apply(console,arguments)}

function tauriInvoke(cmd, args){
  return window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke 
    ? window.__TAURI_INTERNALS__.invoke(cmd, args || {})
    : Promise.reject(new Error('Not in Tauri'));
}
function loadHistoryList(){
  let el=document.getElementById('historyList');
  el.innerHTML='<div class="empty">加载中...</div>';
  tauriInvoke('load_game_history').then(list=>{
    if(!list||list.length===0){
      el.innerHTML='<div class="empty">暂无历史记录</div>';
      return;
    }
    el.innerHTML='';
    // 倒序显示（最新的在前），未完成的排在前面
    let unfinishedRecords = [];
    let finishedRecords = [];
    for(let i=list.length-1;i>=0;i--){
      if(list[i].finished === false){
        unfinishedRecords.push(list[i]);
      }else{
        finishedRecords.push(list[i]);
      }
    }
    // 先渲染未完成的（可继续）
    for(let r of unfinishedRecords){
      _historyRecords[r.id]=r;
      let div=document.createElement('div');div.className='history-item';
      div.dataset.recordId=r.id;
      div.style.cssText = 'border-color:var(--accent2);cursor:default';
      let modeLabel=r.mode==='ai'?'AI对战':(r.mode==='eve'?'AI斗蛐蛐':'本地对战');
      let innerHtml=`
        <span class="sel-chk"></span>
        <div class="h-time" style="color:var(--accent2)">进行中</div>
        <div class="h-info">${modeLabel} · ${r.boardSize}×${r.boardSize} · ${r.playerCount}人${r.aiCount>0?(' · AI×'+r.aiCount):''}</div>
        <div class="h-winner" style="color:var(--accent2)">进行中 · ${r.playerCount} 人模式</div>
        <div style="margin-top:4px;display:flex;gap:6px">
          <button class="glass-btn primary" id="continueFromHistoryBtn" style="flex:1;padding:6px 8px;font-size:.78rem">▶ 继续游戏</button>
          <button class="glass-btn danger" id="discardHistoryBtn" style="padding:6px 10px;font-size:.78rem">放弃</button>
        </div>
      `;
      div.innerHTML = innerHtml;
      let btn = div.querySelector('#continueFromHistoryBtn');
      if(btn){
        btn.id = '';
        btn.onclick = function(e){e.stopPropagation(); continueFromHistory(r); };
      }
      let dbtn = div.querySelector('#discardHistoryBtn');
      if(dbtn){
        dbtn.id = '';
        dbtn.onclick = function(e){e.stopPropagation(); discardHistoryRecord(r); };
      }
      // 复选框点击
      const chk=div.querySelector('.sel-chk');
      if(chk)chk.onclick=function(e){e.stopPropagation();if(!_multiSelectActive)enterMultiSelect();toggleSelection(r.id)};
      initLongPress(div,r.id);
      el.appendChild(div);
    }
    // 已完成的历史记录
    for(let r of finishedRecords){
      _historyRecords[r.id]=r;
      let div=document.createElement('div');div.className='history-item';
      div.dataset.recordId=r.id;
      let modeLabel=r.mode==='ai'?'AI对战':(r.mode==='eve'?'AI斗蛐蛐':'本地对战');
      let winnerName=(r.colorNames&&r.colorNames[r.winner])||(r.winner!==null&&r.winner!==undefined?`玩家 ${r.winner+1}`:'平局');
      let winnerColor=r.winner!==null&&r.winner!==undefined?COLORS[r.winner]||'#888':'#888';
      div.style.cursor='pointer';
      div.onclick=function(){const rid=parseInt(this.dataset.recordId);const r=_historyRecords[rid];if(r)showHistoryDetail(r)};
      div.innerHTML=`
        <span class="sel-chk"></span>
        <div class="h-time">${r.time}</div>
        <div class="h-info">${modeLabel} · ${r.boardSize}×${r.boardSize} · ${r.playerCount}人${r.aiCount>0?(' · AI×'+r.aiCount):''}</div>
        <div class="h-winner" style="color:${winnerColor}">${winnerName}</div>
        <div class="h-details">${r.aiAlgorithm?('算法: '+r.aiAlgorithm):''}${r.aiDepth>0?(' · 深度: '+r.aiDepth):''}</div>
      `;
      // 复选框点击
      const chk=div.querySelector('.sel-chk');
      if(chk)chk.onclick=function(e){e.stopPropagation();if(!_multiSelectActive)enterMultiSelect();toggleSelection(r.id)};
      initLongPress(div,r.id);
      el.appendChild(div);
    }
  }).catch(()=>{
    el.innerHTML='<div class="empty">暂无历史记录</div>';
  }).finally(() => {
    // 检查是否有已保存的未完成游戏（可继续）
    // 优先读取同步 localStorage（可靠，不依赖异步 IPC）
    let backupState = null;
    try {
      const raw = localStorage.getItem('unfinishedGameState');
      if(raw) backupState = JSON.parse(raw);
    } catch(e) {}
    // 尝试从 Tauri 后端加载
    loadSavedGameState().then(saved => {
      const effectiveState = saved || backupState;
      if(!effectiveState) return;
      const el = document.getElementById('historyList');
      if(!el) return;
      const sep = document.createElement('div');
      sep.style.cssText = 'border-top:1px solid var(--glass-w-08);margin:12px 0 8px;width:100%';
      el.appendChild(sep);
      const div = document.createElement('div');
      div.className = 'history-item';
      div.style.cssText = 'border-color:var(--accent2);cursor:default';
      const modeLabel = effectiveState.gameMode === 'ai' ? 'AI对战' : (effectiveState.gameMode === 'eve' ? 'AI斗蛐蛐' : '本地对战');
      const elimArr = effectiveState.eliminatedPlayers;
      const liveCount = effectiveState.maxPlayers - (elimArr ? (Array.isArray(elimArr) ? elimArr.length : (elimArr.size || 0)) : 0);
      div.innerHTML = `
        <div class="h-time" style="color:var(--accent2)">进行中</div>
        <div class="h-info">${modeLabel} · ${effectiveState.size}×${effectiveState.size} · ${effectiveState.maxPlayers}人${(effectiveState.aiCount||0)>0?(' · AI×'+effectiveState.aiCount):''}</div>
        <div class="h-winner" style="color:var(--accent2)">存活 ${liveCount} 人 · 轮到 ${(effectiveState.colorNames&&effectiveState.colorNames[effectiveState.curPlayer])||'玩家 '+(effectiveState.curPlayer+1)}</div>
        <div style="margin-top:4px;display:flex;gap:6px">
          <button class="glass-btn primary" onclick="continueGame()" style="flex:1;padding:6px 8px;font-size:.78rem">▶ 继续游戏</button>
          <button class="glass-btn danger" onclick="discardSavedGame(event)" style="padding:6px 10px;font-size:.78rem">放弃</button>
        </div>
      `;
      el.appendChild(div);
    }).catch(() => {
      if(!backupState) return;
      const el = document.getElementById('historyList');
      if(!el) return;
      const sep = document.createElement('div');
      sep.style.cssText = 'border-top:1px solid var(--glass-w-08);margin:12px 0 8px;width:100%';
      el.appendChild(sep);
      const div = document.createElement('div');
      div.className = 'history-item';
      div.style.cssText = 'border-color:var(--accent2);cursor:default';
      const modeLabel = backupState.gameMode === 'ai' ? 'AI对战' : (backupState.gameMode === 'eve' ? 'AI斗蛐蛐' : '本地对战');
      const elimArr = backupState.eliminatedPlayers;
      const liveCount = backupState.maxPlayers - (elimArr ? (Array.isArray(elimArr) ? elimArr.length : (elimArr.size || 0)) : 0);
      div.innerHTML = `
        <div class="h-time" style="color:var(--accent2)">进行中</div>
        <div class="h-info">${modeLabel} · ${backupState.size}×${backupState.size} · ${backupState.maxPlayers}人${(backupState.aiCount||0)>0?(' · AI×'+backupState.aiCount):''}</div>
        <div class="h-winner" style="color:var(--accent2)">存活 ${liveCount} 人 · 轮到 ${(backupState.colorNames&&backupState.colorNames[backupState.curPlayer])||'玩家 '+(backupState.curPlayer+1)}</div>
        <div style="margin-top:4px;display:flex;gap:6px">
          <button class="glass-btn primary" onclick="continueGame()" style="flex:1;padding:6px 8px;font-size:.78rem">▶ 继续游戏</button>
          <button class="glass-btn danger" onclick="discardSavedGame(event)" style="padding:6px 10px;font-size:.78rem">放弃</button>
        </div>
      `;
      el.appendChild(div);
    });
  });
}
function discardSavedGame(e){
  if(e)e.stopPropagation();
  if(!confirm('确定要放弃这个未完成的游戏吗？'))return;
  clearSavedGameState();
  loadHistoryList();
}
function discardHistoryRecord(record){
  if(!confirm('确定要放弃这个进行中的游戏记录吗？'))return;
  if(record.id){
    tauriInvoke("delete_game_history_record", {recordId: record.id})
      .then(() => loadHistoryList())
      .catch(e => logWarn("Delete history record failed:", e));
  }
  delete _historyRecords[record.id];
}
function exportHistory(){
  tauriInvoke('load_game_history').then(list=>{
    if(!list||list.length===0){
      alert('暂无历史记录可导出');
      return;
    }
    const jsonData = JSON.stringify(list, null, 2);
    return tauriInvoke('export_game_history_dialog', {jsonData}).then(res=>{
      if(res){
        const isFallback = res.startsWith('fallback:');
        const bytes = isFallback ? res.slice(9) : res;
        const info = document.createElement('div');
        info.className = 'history-item';
        info.style.borderColor = 'var(--accent2)';
        info.style.animation = 'fadeIn .3s';
        info.innerHTML = '<div class="h-time">导出成功</div><div class="h-info"></div>';
        info.querySelector('.h-info').textContent = bytes + ' 字节';
        document.getElementById('historyList').insertAdjacentElement('afterbegin', info);
      }
    });
  }).catch(e=>{
    logWarn('Export failed:', e);
    alert('导出失败: ' + (e||'未知错误'));
  });
}
function importHistory(event){
  const file = event.target.files[0];
  if(!file) return;
  const reader = new FileReader();
  reader.onload = function(e){
    try{
      const jsonData = e.target.result;
      // 先验证 JSON 格式
      JSON.parse(jsonData);
      tauriInvoke('import_game_history', {jsonData}).then(count=>{
        if(count>0){
          alert(`成功导入 ${count} 条记录`);
          loadHistoryList();
        }else{
          alert('没有新记录需要导入（全部已存在）');
        }
      }).catch(err=>{
        alert('导入失败: ' + (err||'未知错误'));
      });
    }catch(err){
      alert('文件格式错误，请选择有效的 JSON 文件');
    }
  };
  reader.readAsText(file);
  // 重置 input 以便重复选择同一文件
  event.target.value = '';
}
// ─── 多选模式 ───
let _multiSelectActive=false;
let _selectedIds=new Set();
let _historyRecords={}; // id -> record
let _longPressTimer=null;
function enterMultiSelect(){
  _multiSelectActive=true;
  document.getElementById('selBar').style.display='flex';
  document.getElementById('selExportBtn').style.display='';
  document.getElementById('selDeleteBtn').style.display='';
  document.getElementById('selCancelBtn').style.display='';
  document.querySelectorAll('#historyList .history-item').forEach(el=>{
    el.classList.add('sel-mode');
    el.onclick=function(e){
      if(e.target.closest('.sel-chk')||e.target.closest('button'))return;
      const id=parseInt(this.dataset.recordId);
      toggleSelection(id);
    };
  });
  updateSelBar();
  // 进入多选 → 禁用所有"继续游戏"按钮，避免误触
  document.querySelectorAll('#historyList .glass-btn.primary').forEach(b=>{
    b.style.pointerEvents='none';b.style.opacity='.35';
  });
}
function exitMultiSelect(){
  _multiSelectActive=false;
  _selectedIds.clear();
  document.getElementById('selBar').style.display='none';
  document.getElementById('selExportBtn').style.display='none';
  document.getElementById('selDeleteBtn').style.display='none';
  document.getElementById('selCancelBtn').style.display='none';
  document.querySelectorAll('#historyList .history-item').forEach(el=>{
    el.classList.remove('sel-mode','sel-selected');
    const chk=el.querySelector('.sel-chk');
    if(chk)chk.classList.remove('checked');
    el.onclick=function(){
      const rid=parseInt(this.dataset.recordId);
      const r=_historyRecords[rid];
      if(r)showHistoryDetail(r);
    };
  });
  updateSelBar();
  // 退出多选 → 恢复继续游戏按钮
  document.querySelectorAll('#historyList .glass-btn.primary').forEach(b=>{
    b.style.pointerEvents='';b.style.opacity='';
  });
}
function toggleSelection(id){
  if(!_multiSelectActive)return;
  vibrate(8);
  if(_selectedIds.has(id))_selectedIds.delete(id);else _selectedIds.add(id);
  const el=document.querySelector(`#historyList .history-item[data-record-id="${id}"]`);
  if(el){
    el.classList.toggle('sel-selected',_selectedIds.has(id));
    const chk=el.querySelector('.sel-chk');
    if(chk)chk.classList.toggle('checked',_selectedIds.has(id));
  }
  updateSelBar();
}
function updateSelBar(){
  const cnt=_selectedIds.size;
  document.getElementById('selCount').textContent=`已选择 ${cnt} 项`;
  document.getElementById('selExportBtn').style.display=cnt>0?'':'none';
  document.getElementById('selDeleteBtn').style.display=cnt>0?'':'none';
}
function initLongPress(el,id){
  let pressed=false;
  el.addEventListener('touchstart',function(){
    _longPressTimer=setTimeout(()=>{
      pressed=true;
      vibrate(15);
      if(!_multiSelectActive)enterMultiSelect();
      toggleSelection(id);
    },500);
  },{passive:true});
  el.addEventListener('touchend',function(){
    clearTimeout(_longPressTimer);
    pressed=false;
  },{passive:true});
  el.addEventListener('touchmove',function(){clearTimeout(_longPressTimer);pressed=false},{passive:true});
  el.addEventListener('mousedown',function(){
    _longPressTimer=setTimeout(()=>{
      vibrate(15);
      if(!_multiSelectActive)enterMultiSelect();
      toggleSelection(id);
    },500);
  });
  el.addEventListener('mouseup',function(){clearTimeout(_longPressTimer)});
  el.addEventListener('mouseleave',function(){clearTimeout(_longPressTimer)});
}
async function exportSelectedRecords(){
  if(_selectedIds.size===0)return;
  const selected=[];
  for(let id of _selectedIds){
    if(_historyRecords[id])selected.push(_historyRecords[id]);
  }
  if(selected.length===0){alert('没有选中的记录');return}
  const jsonData=JSON.stringify(selected,null,2);
  try{
    const res=await tauriInvoke('export_game_history_dialog',{jsonData});
    if(res){
      const bytes=res.startsWith('fallback:')?res.slice(9):res;
      alert(`成功导出 ${selected.length} 条记录（${bytes} 字节）`);
      exitMultiSelect();
    }
  }catch(e){
    logWarn('Export selected failed:',e);
    alert('导出失败: '+(e||'未知错误'));
  }
}
async function deleteSelectedRecords(){
  if(_selectedIds.size===0)return;
  if(!confirm(`确定要删除选中的 ${_selectedIds.size} 条记录吗？此操作不可恢复。`))return;
  const ids=[..._selectedIds];
  try{
    await tauriInvoke('delete_game_history_records',{recordIds:ids});
    alert(`已删除 ${ids.length} 条记录`);
    exitMultiSelect();
    loadHistoryList();
  }catch(e){
    logWarn('Delete selected failed:',e);
    alert('删除失败: '+(e||'未知错误'));
  }
}
function clearHistory(){
  if(!confirm('确定要清空所有历史记录吗？此操作不可恢复。'))return;
  tauriInvoke('clear_game_history').then(()=>{
    document.getElementById('historyList').innerHTML='<div class="empty">历史记录已清空</div>';
    exitMultiSelect();
  }).catch(e=>{
    logWarn('Clear history failed:', e);
    alert('清空历史失败');
  });
}
// 格式化时间为 YYYY/MM/DD HH:mm:ss
function formatTime(d){
  let y=d.getFullYear(),m=d.getMonth()+1,dd=d.getDate();
  let h=d.getHours(),mi=d.getMinutes(),s=d.getSeconds();
  return y+'/'+(m<10?'0'+m:m)+'/'+(dd<10?'0'+dd:dd)+' '+(h<10?'0'+h:h)+':'+(mi<10?'0'+mi:mi)+':'+(s<10?'0'+s:s);
}
// ─── 紧凑历史格式转换 ───
// 将逐回合快照 [{turn,snapshot:{pid:{pieces,points}}}] 压缩为 {c:true,t:回合数,p:[[pieces..],[pieces..]],pt:[[points..],[points..]]}
function compactHistory(history, playerCount){
  if(!history||history.length===0)return null;
  const n=history.length;
  const pieces=Array.from({length:playerCount},()=>new Array(n).fill(0));
  const points=Array.from({length:playerCount},()=>new Array(n).fill(0));
  for(let t=0;t<n;t++){
    const snap=history[t].snapshot||{};
    for(let p=0;p<playerCount;p++){
      const d=snap[String(p)];
      if(d){pieces[p][t]=d.pieces||0;points[p][t]=d.points||0}
    }
  }
  return {c:true,t:n,p:pieces,pt:points};
}
// 将紧凑格式展开为逐回合快照格式（供图表渲染用）
function expandHistory(compact, playerCount){
  if(!compact||!compact.c)return compact; // 已经是旧格式或空
  const n=compact.t||0;
  const history=[];
  for(let t=0;t<n;t++){
    const snapshot={};
    for(let p=0;p<playerCount;p++){
      snapshot[String(p)]={
        pieces:(compact.p&&compact.p[p]?compact.p[p][t]:0),
        points:(compact.pt&&compact.pt[p]?compact.pt[p][t]:0),
      };
    }
    history.push({turn:t, snapshot});
  }
  return history;
}
// Save game history for Tauri（紧凑存储）
// historyArg 可选：若传入则用其代替 gameHistory（用于 gameHistory 已被清空的场景）
function saveGameHistory(winner, mode, aiAlg, aiDp, historyArg){
  const src = historyArg || gameHistory;
  const compact = compactHistory(src, maxPlayers);
  tauriInvoke('save_game_history', {
    record: {
      id: Date.now(),
      time: formatTime(new Date()),
      mode: mode || gameMode || 'local',
      aiAlgorithm: aiAlg || aiAlgorithm || '',
      aiDepth: aiDp || aiDepth || 0,
      gameCount: gameCount,
      playerCount: maxPlayers,
      aiCount: aiPlayers.size,
      boardSize: size,
      winner: winner !== null && winner !== undefined ? winner : null,
      colorNames: _colorNames || COLOR_NAMES,
      chainStats: chainStats,
      maxChain: maxChainOverall,
      finished: true,
      history: compact,
    }
  }).then(() => {
    // 保存成功后清理磁盘上的回合数据（不再需要）
    tauriInvoke('clear_round_history').catch(e=>logWarn('Clear round history after save failed:', e));
  }).catch(e=>logWarn('Save history failed:', e));
}// ─── 保存未完成游戏历史记录（用于异常退出后继续游戏） ───
async function saveUnfinishedGameHistory(){
  if(!gameMode||gameOver)return;
  // ★ 在 await 之前捕获所有全局变量的快照（避免 exitGame 在 await 期间重置全局变量后读取到空值）
  const _board = board;
  const _size = size;
  const _maxPlayers = maxPlayers;
  const _curPlayer = curPlayer;
  const _firstMovePos = firstMovePos;
  const _gameMode = gameMode;
  const _aiPlayers = Array.from(aiPlayers);
  const _aiConfigs = aiConfigs || {};
  const _eliminatedPlayers = Array.from(eliminatedPlayers);
  const _chainStats = JSON.parse(JSON.stringify(chainStats || {}));
  const _maxChainOverall = maxChainOverall ? {...maxChainOverall} : {player:null,length:0};
  const _gameCount = gameCount || 0;
  const _aiAlgorithm = aiAlgorithm || "";
  const _aiDepth = aiDepth || 0;
  const _colorNames = window._colorNames || COLOR_NAMES;
  // 先同步保存当前游戏状态（确保 exitGame 等后续清理前已保存）
  saveCurrentGameState();
  // 再检查存活人数（保持与原代码一致的顺序）
  const _liveCount = _maxPlayers - _eliminatedPlayers.length;
  if(_liveCount < 2) return;
  // 再异步合并磁盘历史得到完整历史，专用于 history 记录的 gameState
  let fullHistory = [...gameHistory];
  try {
    const disk = await tauriInvoke('load_round_history');
    if (disk && Array.isArray(disk) && disk.length > 0) {
      const merged = [...disk];
      const offset = merged.length;
      const memAdjusted = fullHistory.map((h, i) => ({...h, turn: offset + i}));
      fullHistory = [...merged, ...memAdjusted];
    }
  } catch(e) {
    logWarn('Load round history for save failed:', e);
  }
  // 用之前捕获的局部快照构建 state（避免 exitGame 重置全局变量后的空值问题）
  const state = {
    board: _board,
    size: _size,
    maxPlayers: _maxPlayers,
    curPlayer: _curPlayer,
    firstMovePos: _firstMovePos,
    gameMode: _gameMode,
    aiPlayers: _aiPlayers,
    aiConfigs: _aiConfigs,
    eliminatedPlayers: _eliminatedPlayers,
    chainStats: _chainStats,
    maxChainOverall: _maxChainOverall,
    gameCount: _gameCount,
    aiAlgorithm: _aiAlgorithm,
    aiDepth: _aiDepth,
    colorNames: _colorNames,
    aiCount: _aiPlayers.length,
    undoStack: [],
    savedHistory: fullHistory.length > 0 ? compactHistory(fullHistory, _maxPlayers) : null,
  };
  const stateJson = JSON.stringify(state);
  const compact = fullHistory.length > 0 ? compactHistory(fullHistory, _maxPlayers) : {};
  tauriInvoke("save_game_history", {
    record: {
      id: Date.now(),
      time: formatTime(new Date()),
      mode: _gameMode,
      aiAlgorithm: _aiAlgorithm,
      aiDepth: _aiDepth,
      gameCount: _gameCount,
      playerCount: _maxPlayers,
      aiCount: _aiPlayers.length,
      boardSize: _size,
      winner: null,
      colorNames: _colorNames,
      chainStats: _chainStats,
      maxChain: _maxChainOverall,
      finished: false,
      history: compact,
      gameState: stateJson,
    }
  }).catch(e => logWarn("Save unfinished history failed:", e));
}

// ─── 从未完成历史记录继续游戏 ───
async function continueFromHistory(record){
  if(!record || !record.gameState){
    showMsg("该记录缺少游戏状态，无法继续","");
    return;
  }
  let saved;
  try { saved = JSON.parse(record.gameState); } catch(e) {
    showMsg("游戏状态解析失败","");
    return;
  }
  if(!saved || !saved.board || !saved.size){
    showMsg("未找到可继续的游戏记录","");
    return;
  }
  // 消耗此历史记录（从磁盘删除，避免重复恢复）
  if(record.id){
    tauriInvoke("delete_game_history_record", {recordId: record.id}).catch(()=>{});
  }
  clearSavedGameState();
  loadGameFromState(saved);
}

// ─── 从状态对象恢复游戏（内部函数，continueGame 和 continueFromHistory 共用） ───
function loadGameFromState(saved){
  if(!saved || !saved.board || !saved.size) return;
  clearBoardDOM();
  location.hash = "#game";
  resetRoundHistory();
  undoStack = [];

  size = saved.size;
  maxPlayers = saved.maxPlayers;
  curPlayer = saved.curPlayer;
  firstMovePos = saved.firstMovePos || null;
  gameMode = saved.gameMode || "local";
  gameOver = false;
  isPaused = false;
  aiThinking = false;
  gameCount = (saved.gameCount || 0) + 1;
  aiAlgorithm = saved.aiAlgorithm || "";
  aiDepth = saved.aiDepth || 0;

  document.getElementById("pauseBtn").textContent = "暂停";
  _originPage = gameMode === "ai" ? "aiLobby" : (gameMode === "eve" ? "eveLobby" : "localLobby");

  board = saved.board;

  aiPlayers = new Set();
  if(saved.aiPlayers && saved.aiPlayers.length > 0){
    saved.aiPlayers.forEach(p => aiPlayers.add(p));
  }

  aiConfigs = saved.aiConfigs || {};

  eliminatedPlayers = new Set();
  if(saved.eliminatedPlayers && saved.eliminatedPlayers.length > 0){
    saved.eliminatedPlayers.forEach(p => eliminatedPlayers.add(p));
  }

  chainStats = saved.chainStats || {};
  maxChainOverall = saved.maxChainOverall || {player:null,length:0};

  gameHistory = [];
  if(saved.savedHistory){
    const expanded = expandHistory(saved.savedHistory, maxPlayers);
    if(expanded && expanded.length > 0){
      gameHistory = expanded;
    }
  }
  recordHistory();

  renderBoard(true);
  show("game");
  document.body.style.background = "";
  renderPlayerBar();
  if(aiPlayers.has(curPlayer) && !gameOver) setTimeout(() => triggerAI(), 400);
}


// ─── 已保存游戏状态管理（用于继续游戏） ───
function saveCurrentGameState(historyOverride){
  if(!gameMode||gameOver)return;
  // historyOverride 可选：外部传入完整历史（解决刷盘后 gameHistory 不全的问题）
  const hist = historyOverride || gameHistory;
  // 构建用于恢复的完整游戏状态
  const state = {
    board: board,
    size: size,
    maxPlayers: maxPlayers,
    curPlayer: curPlayer,
    firstMovePos: firstMovePos,
    gameMode: gameMode,
    aiPlayers: Array.from(aiPlayers),
    aiConfigs: aiConfigs || {},
    eliminatedPlayers: Array.from(eliminatedPlayers),
    chainStats: JSON.parse(JSON.stringify(chainStats || {})),
    maxChainOverall: maxChainOverall ? {...maxChainOverall} : {player:null,length:0},
    gameCount: gameCount || 0,
    aiAlgorithm: aiAlgorithm || '',
    aiDepth: aiDepth || 0,
    colorNames: _colorNames || COLOR_NAMES,
    aiCount: (aiPlayers && aiPlayers.size) || 0,
    undoStack: [],
    // 保存回合历史（紧凑格式，用于图表渲染）
    savedHistory: hist.length > 0 ? compactHistory(hist, maxPlayers) : null,
  };
  const json = JSON.stringify(state);
  tauriInvoke('save_game_state', {stateJson: json}).catch(e => logWarn('Save game state failed:', e));
}
function loadSavedGameState(){
  return tauriInvoke('load_game_state').then(json => {
    if(!json) return null;
    try { return JSON.parse(json); } catch(e) { return null; }
  }).catch(() => null);
}
function clearSavedGameState(){
  _unfinishedHistorySaved = false;
  // 清除 localStorage 中的未完成游戏备份
  try {
    localStorage.removeItem('unfinishedGameRecord');
    localStorage.removeItem('unfinishedGameState');
  } catch(e) {}
  tauriInvoke('clear_game_state').catch(e => logWarn('Clear game state failed:', e));
}
// 自动保存：页面隐藏/关闭时保存当前游戏状态
// 标记当前游戏是否已经保存过未完成历史（避免重复保存）
let _unfinishedHistorySaved = false;
function setupAutoSave(){
  if(typeof document !== 'undefined'){
    document.addEventListener('visibilitychange', function(){
      if(document.hidden && !gameOver && gameMode && !_unfinishedHistorySaved){
        _unfinishedHistorySaved = true;
        saveUnfinishedGameHistory();
      }
    });
    window.addEventListener('beforeunload', function(){
      if(!gameOver && gameMode && !_unfinishedHistorySaved){
        _unfinishedHistorySaved = true;
        saveUnfinishedGameHistory();
      }
    });
  }
}
setupAutoSave();


let selectedPlayerColor = 0;
function toggleAISettings(){
  let v=parseInt(document.getElementById('localAI').value)||0;
  let maxP=parseInt(document.getElementById('localPlayers').value)||2;
  document.getElementById('aiSettings').style.display=v>0?'block':'none';
  let humanCount = maxP - v;
  let colorRow = document.getElementById('colorPickerRow');
  if(humanCount === 1 && v > 0){
    colorRow.style.display = 'block';
    generateColorOptions();
  } else {
    colorRow.style.display = 'none';
  }
}
function generateColorOptions(){
  let container = document.getElementById('colorOptions');
  if(!container)return;
  container.innerHTML = '';
  let maxP=parseInt(document.getElementById('localPlayers').value)||2;
  let aiCount=parseInt(document.getElementById('localAI').value)||0;
  for(let i=0;i<maxP;i++){
    let isAi = i >= maxP - aiCount;
    if(isAi) continue;
    let btn = document.createElement('button');
    btn.style.cssText = `width:36px;height:36px;border-radius:50%;border:2px solid transparent;background:${COLORS[i]};cursor:pointer;transition:.15s;`;
    btn.dataset.idx = i;
    if(i === selectedPlayerColor) btn.style.border = '2px solid #fff';
    btn.onclick = function(){
      document.querySelectorAll('#colorOptions button').forEach(b => b.style.border = '2px solid transparent');
      this.style.border = '2px solid #fff';
      selectedPlayerColor = parseInt(this.dataset.idx);
    };
    container.appendChild(btn);
  }
}

/* ==================== GAME LOGIC ==================== */
function cap(i,j,s){return 4}
function nbrs(i,j,s){
  let r=[];
  if(i>0)r.push([i-1,j]);if(i<s-1)r.push([i+1,j]);
  if(j>0)r.push([i,j-1]);if(j<s-1)r.push([i,j+1]);
  return r;
}
function nbrs8(i,j,s){
  let r=[];
  for(let di=-1;di<=1;di++)for(let dj=-1;dj<=1;dj++){
    if(di===0&&dj===0)continue;
    let ni=i+di,nj=j+dj;
    if(ni>=0&&ni<s&&nj>=0&&nj<s)r.push([ni,nj]);
  }
  return r;
}
function mkBoard(s){
  return Array.from({length:s},()=>Array.from({length:s},()=>({owner:null,count:0})));
}
function hasPieces(p,b){
  for(let r of b)for(let c of r)if(c.owner===p)return true;
  return false;
}
function nearAny(i,j,s,b){
  for(let[ni,nj]of nbrs8(i,j,s)){if(b[ni][nj].owner!==null)return true}
  return false;
}
const sleep=ms=>new Promise(r=>setTimeout(r,ms));

// 判断是否在首子限制区域内（上下各2格、左右各2格、斜角各1格，共12格）
function isInFirstMoveRestricted(x,y,fx,fy){
  let dx=x-fx, dy=y-fy;
  if(dx===2&&dy===0)return true;
  if(dx===-2&&dy===0)return true;
  if(dx===1&&dy>=-1&&dy<=1)return true;
  if(dx===-1&&dy>=-1&&dy<=1)return true;
  if(dx===0&&dy>=-2&&dy<=2&&!(dx===0&&dy===0))return true;
  return false;
}
// 检查 (x,y) 是否在棋盘上任何已有棋子周围的12格限制区内
function isInRestrictedZone(x,y,sz,b){
  for(let i=0;i<sz;i++)for(let j=0;j<sz;j++){
    if(b[i][j].owner!==null&&isInFirstMoveRestricted(x,y,i,j))return true;
  }
  return false;
}

// ─── 历史存储管理（内存+磁盘两层） ───
const HISTORY_MEM_MAX=500;  // 内存上限，超出则刷盘
const HISTORY_MEM_KEEP=100; // 刷盘后保留条数

function recordHistory(){
  let sn={};
  for(let p=0;p<maxPlayers;p++){
    let pieces=0,points=0;
    for(let r of board)for(let c of r)if(c.owner===p){pieces++;points+=c.count}
    sn[p]={pieces,points};
  }
  gameHistory.push({turn:gameHistory.length,snapshot:sn});
  // 超过内存上限→异步溢出到磁盘（不阻塞落子）
  if(gameHistory.length>HISTORY_MEM_MAX){
    setTimeout(()=>flushOverflowHistory(),0);
  }
  // 每步落子后保存游戏状态，确保异常退出时可继续
  saveCurrentGameState();
}

async function flushOverflowHistory(){
  const overflow=gameHistory.splice(0,gameHistory.length-HISTORY_MEM_KEEP);
  for(let i=0;i<gameHistory.length;i++)gameHistory[i].turn=i;
  let existing=[];
  try{existing=await tauriInvoke('load_round_history')}catch(e){}
  const merged=[...existing,...overflow];
  for(let i=0;i<merged.length;i++)merged[i].turn=i;
  await tauriInvoke('save_round_history',{data:merged}).catch(e=>logWarn('Flush overflow history failed:', e));
}

/// 把所有内存历史刷到磁盘，返回完整历史（磁盘+内存合并排序）
async function flushAndGetFullHistory(){
  if(gameHistory.length>0){
    let existing=[];
    try{existing=await tauriInvoke('load_round_history')}catch(e){}
    const offset=existing.length;
    const memOff=gameHistory.map((h,i)=>({...h,turn:offset+i}));
    const merged=[...existing,...memOff];
    await tauriInvoke('save_round_history',{data:merged}).catch(e=>logWarn('Save full history failed:', e));
    gameHistory=[];
  }
  let disk=[];
  try{disk=await tauriInvoke('load_round_history')}catch(e){}
  return disk;
}

/// 新游戏时清空历史
function resetRoundHistory(){
  gameHistory=[];
  tauriInvoke('clear_round_history').catch(e=>logWarn('Clear round history failed:', e));
}

async function processClick(b,s,x,y,pl,anim,playerColor){
  let c=b[x][y];
  if(c.owner===null){
    let anyPieces=false;
    for(let r of b)for(let cl of r)if(cl.owner!==null)anyPieces=true;
    if(!anyPieces) firstMovePos=[x,y];
    let first = !hasPieces(pl,b);
    c.owner=pl;
    c.count=first?3:1;
  }
  else if(c.owner===pl)c.count++
  else return[];
  let had=new Set();
  for(let row of b)for(let cl of row)if(cl.owner!==null)had.add(cl.owner);
  let chain=[[x,y]];
  let chainCount=0;

  if(anim==='explode'){
    // ★ 逐格爆炸动画：每次只炸一个格子，用户可跳过
    chainSkipAll=false;
    if(autoSkipChain)chainSkipAll=true;
    if(!autoSkipChain)showSkipBtn(true);
    while(chain.length){
      if(chainSkipAll){
        // 跳过剩余动画 → 一口气处理完
        while(chain.length){
          let[cx,cy]=chain.shift(),cell=b[cx][cy],capv=cap(cx,cy,s);
          if(cell.count>=capv){
            cell.count=0;cell.owner=null;
            chainCount++;
            playExplosion();
            const cl=cells?.[cx]?.[cy];
            if(cl){addShockwave(cl,playerColor);addParticles(cl,playerColor,8)}
            for(let[nx,ny]of nbrs(cx,cy,s)){
              let nc=b[nx][ny];nc.owner=pl;nc.count++;chain.push([nx,ny]);
            }
          }
        }
        break;
      }
      let[cx,cy]=chain.shift(),cell=b[cx][cy],capv=cap(cx,cy,s);
      if(cell.count>=capv){
        cell.count=0;cell.owner=null;
        chainCount++;
        playExplosion();
        const el2=cells?.[cx]?.[cy];
        if(el2){addShockwave(el2,playerColor);addParticles(el2,playerColor,8)}
        for(let[nx,ny]of nbrs(cx,cy,s)){
          let nc=b[nx][ny];nc.owner=pl;nc.count++;chain.push([nx,ny]);
        }
        // 渲染当前棋盘 + 爆炸动画
        renderBoard(true);
        let el=cells?.[cx]?.[cy];
        if(el)el.classList.add('explode');
        await sleep(220);
        if(el)el.classList.remove('explode');
      }
    }
    showSkipBtn(false);
  }else{
    // 原快速处理（非动画模式）
    while(chain.length){
      let[cx,cy]=chain.shift(),cell=b[cx][cy],capv=cap(cx,cy,s);
      if(cell.count>=capv){
        cell.count=0;cell.owner=null;
        chainCount++;
        playExplosion();
        for(let[nx,ny]of nbrs(cx,cy,s)){
          let nc=b[nx][ny];nc.owner=pl;nc.count++;chain.push([nx,ny]);
        }
        if(anim&&!autoSkipChain){
          let el=cells?.[cx]?.[cy];if(el)el.classList.add('explode');
          renderBoard(false,anim==='pop'?cx:undefined);
          await sleep(180);
          if(el)el.classList.remove('explode');
        }
        else if(autoSkipChain&&anim){
          // 自动跳过也要渲染棋盘
          renderBoard(false);
        }
      }
    }
  }
  // 记录连爆统计
  if(chainCount > 0){
    if(!chainStats[pl]) chainStats[pl] = {triggered: 0, maxChain: 0};
    chainStats[pl].triggered++;
    if(chainCount > chainStats[pl].maxChain) chainStats[pl].maxChain = chainCount;
    if(chainCount > maxChainOverall.length){
      maxChainOverall = {player: pl, length: chainCount};
    }
  }
  let now=new Set();
  for(let row of b)for(let cl of row)if(cl.owner!==null)now.add(cl.owner);
  return[...had].filter(p=>!now.has(p));
}

function showSkipBtn(v){
  const el=document.getElementById('skipChainBtn');
  if(el)el.style.display=v?'block':'none';
}

function toggleAutoSkip(){
  autoSkipChain=!autoSkipChain;
  const btn=document.getElementById('autoSkipBtn');
  if(btn)btn.classList.toggle('on',autoSkipChain);
  if(autoSkipChain)chainSkipAll=true;
}

/* ==================== RENDER ==================== */
// 棋盘状态缓存，用于增量更新
let _boardCache=null;
let _lastRenderPlayer=-1;

function clearBoardDOM(){
  // 强制清空棋盘 DOM（用于游戏结束/重开时确保画面刷新）
  const bd=document.getElementById('board');
  if(bd){
    bd.replaceChildren();
    void bd.offsetHeight;
    void document.body.offsetHeight;
  }
  cells=[];
  _boardCache=null;
}

/* ═══ 视觉特效辅助函数 ═══ */
function addRipple(el){
  if(!el)return;
  const rip=document.createElement('div');rip.className='ripple';
  el.appendChild(rip);
  setTimeout(()=>rip.remove(),600);
}
function addShockwave(el,color){
  if(!el)return;
  const sw=document.createElement('div');sw.className='shockwave';
  sw.style.borderColor=color||'rgba(255,255,255,.5)';
  el.appendChild(sw);
  setTimeout(()=>sw.remove(),600);
}
function addParticles(el,color,count){
  if(!el)return;
  const rect=el.getBoundingClientRect();
  const cx=rect.width/2,cy=rect.height/2;
  for(let i=0;i<(count||6);i++){
    const p=document.createElement('div');p.className='particle';
    const angle=Math.random()*Math.PI*2;
    const dist=30+Math.random()*50;
    p.style.cssText=`left:${cx}px;top:${cy}px;background:${color||'rgba(255,255,255,.7)'};--dx:${Math.cos(angle)*dist}px;--dy:${Math.sin(angle)*dist}px`;
    el.appendChild(p);
    setTimeout(()=>p.remove(),700);
  }
}
function renderBoard(force,popX,popY){
  let bd=document.getElementById('board');
  bd.setAttribute('role','grid');
  bd.setAttribute('aria-label','棋盘');
  if(force||cells.length!==size){
    bd.replaceChildren();
    void bd.offsetHeight;
    bd.style.gridTemplateColumns=`repeat(${size},1fr)`;
    cells=[];
    for(let i=0;i<size;i++){
      let row=[];
      for(let j=0;j<size;j++){
        let el=document.createElement('div');el.className='cell';
        el.setAttribute('role','gridcell');
        el.setAttribute('tabindex','-1');
        const posLabel = `第${i+1}行第${j+1}列`;
        el.setAttribute('aria-label', `${posLabel}，空位`);
        el.onclick=()=>handleClick(i,j);
        bd.appendChild(el);row.push(el);
      }
      cells.push(row);
    }
    void bd.offsetHeight;
    void document.body.offsetHeight;
    _boardCache=null;
  }
  for(let i=0;i<size;i++)for(let j=0;j<size;j++){
    let el=cells[i][j],d=board[i][j];
    let prev=_boardCache?.[i]?.[j];
    // 更新 aria-label（仅当格子状态变化时）
    const posLabel = `第${i+1}行第${j+1}列`;
    if(force||!prev||prev.owner!==d.owner||prev.count!==d.count){
      if(d.owner!==null){
        el.setAttribute('aria-label', `${posLabel}，玩家${d.owner+1}的${d.count}级棋子`);
      }else{
        el.setAttribute('aria-label', `${posLabel}，空位`);
      }
    }
    // 只有变化或有动画时才更新
    if(force||!prev||prev.owner!==d.owner||prev.count!==d.count||_lastRenderPlayer!==curPlayer){
      el.innerHTML='';
      if(d.owner!==null){
        if(d.owner===curPlayer){
          let bg=document.createElement('div');bg.className='bg p'+d.owner;
          el.appendChild(bg);
        }
        let p=document.createElement('div');p.className='piece p'+d.owner;
        el.appendChild(p);
        drawDots(p,d.count);
        if(popX!==undefined&&i===popX&&j===popY)p.classList.add('pop');
      }
    }
  }
  // 缓存当前状态（curPlayer 全局一致，不逐格存储以节省内存）
  _boardCache=board.map(row=>row.map(c=>({owner:c.owner,count:c.count})));
  _lastRenderPlayer=curPlayer;
}
function drawDots(p,n){
  let cx=50,cy=50,r=30;
  if(n===1)addDot(p,cx,cy);
  else if(n===2){addDot(p,cx-20,cy);addDot(p,cx+20,cy)}
  else if(n===3){let h=Math.sqrt(3)/2;addDot(p,cx,cy-r*h/2);addDot(p,cx-r/2,cy+r*h/2);addDot(p,cx+r/2,cy+r*h/2)}
  else{addDot(p,34,34);addDot(p,66,34);addDot(p,34,66);addDot(p,66,66)}
}
function addDot(p,x,y){let d=document.createElement('div');d.className='dot';d.style.left=x+'%';d.style.top=y+'%';p.appendChild(d)}
function setBg(pl){let h=COLORS[pl];let r=parseInt(h.substr(1,2),16)*.85,g=parseInt(h.substr(3,2),16)*.85,b=parseInt(h.substr(5,2),16)*.85;document.body.style.background=`radial-gradient(ellipse 80% 50% at 50% -20%, rgba(240,179,75,.04) 0%, transparent 70%),radial-gradient(ellipse 60% 40% at 80% 100%, rgba(95,195,195,.03) 0%, transparent 70%),rgb(${r|0},${g|0},${b|0})`}
function renderPlayerBar(){
  let el=document.getElementById('playerBar');el.innerHTML='';
  let cnt={};
  for(let p=0;p<maxPlayers;p++)cnt[p]=0;
  for(let row of board)for(let c of row)if(c.owner!==null)cnt[c.owner]++;
  for(let p=0;p<maxPlayers;p++){
    let t=document.createElement('span');t.className='player-tag';
    if(p===curPlayer)t.classList.add('active');
    let alive=cnt[p]>0;
    if(!alive&&board.some(r=>r.some(c=>c.owner!==null)))t.classList.add('elim');
    t.style.background=COLORS_LIGHT[p];
    t.style.color=COLORS[p];
    let label=(_colorNames&&_colorNames[p])||(aiPlayers.has(p)?`AI ${p+1}`:`玩家 ${p+1}`);
    t.innerHTML=`${label} <span class="cnt">${cnt[p]}</span>`;
    el.appendChild(t);
  }
}
function cloneBoard(b){
  return b.map(row=>row.map(c=>({owner:c.owner,count:c.count})));
}

async function triggerAI(){
  if(gameOver||aiThinking||isPaused)return;
  if(!aiPlayers.has(curPlayer))return;
  // AI 首子由玩家放置：AI 无棋子时，由玩家点击落子
  if(!hasPieces(curPlayer,board))return;
  saveUndoState();
  aiThinking=true;
  await sleep(50);
  let move;
  // 获取当前 AI 的配置（eve 模式用 per-AI config，其他模式用全局配置）
  let alg = aiConfigs[curPlayer] ? aiConfigs[curPlayer].algorithm : aiAlgorithm;
  let dep = aiConfigs[curPlayer] ? aiConfigs[curPlayer].depth : aiDepth;
  let randomScale = aiConfigs[curPlayer] ? (aiConfigs[curPlayer].randomScale ?? 10) : 10;
  let useMlEval = aiConfigs[curPlayer] ? (aiConfigs[curPlayer].useMlEval ?? true) : true;
  // 所有算法统一走 Rust 引擎
  let cmd;
  if(alg==='mcts'){cmd='ai_move_mcts';}
  else if(alg==='pvs'){cmd='ai_move_v2';}
  else if(alg==='alphabeta'){cmd='ai_move';}
  else{cmd='ai_move_strategy';}
  const args = {
    board: board,
    size: size,
    player: curPlayer,
    depth: dep,
    eliminated: [...eliminatedPlayers],
    maxPlayers: maxPlayers,
    gameCount: gameCount,
    firstMovePos: firstMovePos,
    randomScale: randomScale,
    useMlEval: useMlEval,
  };
  if(alg==='pvs')args.algorithm=alg;
  try {
    const result = await window.__TAURI_INTERNALS__.invoke(cmd, args);
    if (result && result.length === 2) {
      move = [result[0], result[1]];
    }
  } catch(e) {
    logWarn('AI move failed:', e);
  }
  // 缓存 AI 走法到 undo 栈顶，悔棋时直接重放无需重算
  if(undoStack.length>0)undoStack[undoStack.length-1].aiMove=move;
  if(!move){
    showMsg(`${_colorNames?.[curPlayer]||'AI '+(curPlayer+1)} 无合法落子，跳过`,'error');
    aiThinking=false;
    let alive=[];
    for(let p=0;p<maxPlayers;p++){if(!eliminatedPlayers.has(p))alive.push(p);}
    if(alive.length<=1){
      if(alive.length===1){gameOver=true;playGameOver();showSettlement(alive[0],_colorNames||COLOR_NAMES,gameHistory);}
      return;
    }
    let idx=alive.indexOf(curPlayer);
    if(idx<0)idx=0;
    curPlayer=alive[(idx+1)%alive.length];
    renderPlayerBar();setBg(curPlayer);
    if(aiPlayers.has(curPlayer)&&!gameOver)setTimeout(()=>triggerAI(),400);
    return;
  }
  let[x,y]=move;
  let el=cells[x][y];
  if(el)el.style.background='rgba(255,255,255,.15)';
  await new Promise(r=>setTimeout(r,350));
  if(el)el.style.background='';
  aiThinking=false;
  let c=board[x][y];
  if(c.owner!==null&&c.owner!==curPlayer){return}
  // AI 落子也显示波纹
  const aiClickEl=cells?.[x]?.[y];
  if(aiClickEl)addRipple(aiClickEl);
  playClick();
  let elim;
  try {
    let result=await tauriInvoke('process_move',{
      board:board,size:size,x:x,y:y,
      player:curPlayer,maxPlayers:maxPlayers
    });
    elim=await processClick(board,size,x,y,curPlayer,'explode',COLORS[curPlayer]);
    board=result.board;
    elim=result.eliminated||[];
  }catch(e){
    elim=await processClick(board,size,x,y,curPlayer,'explode',COLORS[curPlayer]);
  }
  for(let e of elim){eliminatedPlayers.add(e);playElim()}
  recordHistory();
  renderBoard(true);
  let alive=[];
  for(let p=0;p<maxPlayers;p++){
    if(eliminatedPlayers.has(p))continue;
    alive.push(p);
  }
  if(alive.length<=1){
    gameOver=true;
    playGameOver();
    showSettlement(alive[0],_colorNames||COLOR_NAMES,gameHistory);
    return;
  }
  let idx=alive.indexOf(curPlayer);
  if(idx<0)idx=0;
  curPlayer=alive[(idx+1)%alive.length];
  renderPlayerBar();
  setBg(curPlayer);
  if(aiPlayers.has(curPlayer)&&!gameOver)setTimeout(()=>triggerAI(),400);
}

/* ==================== LOCAL GAME ==================== */




// 设置 ML/手写评估函数


function togglePause(){
  if(gameOver)return;
  isPaused=!isPaused;
  let btn=document.getElementById('pauseBtn');
  if(isPaused){
    btn.textContent='▶ 继续';
    showPauseOverlay();
  } else {
    btn.textContent='暂停';
    hidePauseOverlay();
    updateUndoBtn();
  }
}
async function showPauseOverlay(){
  let overlay=document.getElementById('pauseOverlay');
  overlay.style.display='flex';

  // 追加快照：从板面实时状态
  let liveSnap={};
  for(let p=0;p<maxPlayers;p++){
    let pieces=0,points=0;
    for(let r of board)for(let c of r)if(c.owner===p){pieces++;points+=c.count}
    liveSnap[p]={pieces,points};
  }
  let lastSnap=gameHistory.length>0?gameHistory[gameHistory.length-1].snapshot:{};
  let stale=false;
  for(let p=0;p<maxPlayers;p++){
    let old=lastSnap[String(p)],cur=liveSnap[String(p)];
    if(!old||old.pieces!==cur.pieces||old.points!==cur.points){stale=true;break}
  }
  if(stale||gameHistory.length===0){
    gameHistory.push({turn:gameHistory.length,snapshot:liveSnap});
  }

  // 刷盘获取完整历史
  const fullHistory=await flushAndGetFullHistory();

  // 统一渲染
  let statsGrid=document.getElementById('pauseStats');
  statsGrid.innerHTML='';
  let chartArea=document.getElementById('pauseCharts');
  chartArea.innerHTML='';
  renderGameCharts(statsGrid,fullHistory,{
    colorNames:_colorNames||COLOR_NAMES,colors:COLORS,eliminated:eliminatedPlayers,
    _noBack:true
  });
  // 图表额外放进 chartArea（因为 pauseOverlay 布局分 stats 和 charts 两个容器）
  // 移动图表到 chartArea
  let chartBoxes=statsGrid.querySelectorAll('.chart-box');
  chartBoxes.forEach(b=>chartArea.appendChild(b));
  // 被动监听：ResizeObserver 替代 requestAnimationFrame 轮询
  chartBoxes.forEach(b=>{
    let cv=b.querySelector('canvas');
    if(cv&&cv.id&&_chartCtx[cv.id]){
      let ro=new ResizeObserver(()=>{
        let r=cv.getBoundingClientRect();
        if(r.width>0&&r.height>0){
          drawLineChart(cv,_chartCtx[cv.id].history,_chartCtx[cv.id].colorNames,_chartCtx[cv.id].colors,_chartCtx[cv.id].valueKey);
          ro.disconnect();
        }
      });
      ro.observe(cv);
    }
  });
}
/* ═══ 悔棋 ═══ */
function saveUndoState(){
  undoStack.push({
    board: cloneBoard(board),
    curPlayer: curPlayer,
    firstMovePos: firstMovePos,
    eliminatedPlayers: new Set(eliminatedPlayers),
    chainStats: JSON.parse(JSON.stringify(chainStats)),
    maxChainOverall: {...maxChainOverall},
    aiMove: null,  // AI 缓存走法，悔棋时无需重算
  });
  // 最多保留 50 步防止内存溢出
  if(undoStack.length>50)undoStack.shift();
  updateUndoBtn();
}
function undoLastMove(){
  if(undoStack.length===0||gameOver||aiThinking)return;
  let state=undoStack.pop();
  board=state.board;
  curPlayer=state.curPlayer;
  firstMovePos=state.firstMovePos;
  eliminatedPlayers=state.eliminatedPlayers;
  chainStats=state.chainStats;
  maxChainOverall=state.maxChainOverall;
  // 移除 gameHistory 最后一条
  if(gameHistory.length>0)gameHistory.pop();
  renderBoard(true);
  renderPlayerBar();
  setBg(curPlayer);
  updateUndoBtn();
  showMsg('已撤销上一步','');
  // 如果当前轮到 AI 且有缓存走法，直接重放无需重算
  if(aiPlayers.has(curPlayer)&&!gameOver&&state.aiMove){
    replayAiMove(state.aiMove);
    return;
  }
  // 如果当前轮到 AI（无缓存），触发 AI
  if(aiPlayers.has(curPlayer)&&!gameOver)setTimeout(()=>triggerAI(),300);
}

/** 重放缓存 AI 走法（直接应用，节约 Rust 计算开销） */
async function replayAiMove(move){
  if(!move||move.length!==2)return;
  // 给用户反应时间，看清悔棋后的局面
  await sleep(300);
  let[x,y]=move;
  let el=cells?.[x]?.[y];
  if(el)addRipple(el);
  playClick();
  let elim;
  try{
    let result=await tauriInvoke('process_move',{
      board:board,size:size,x:x,y:y,
      player:curPlayer,maxPlayers:maxPlayers
    });
    elim=await processClick(board,size,x,y,curPlayer,null,COLORS[curPlayer]);
    board=result.board;
    elim=result.eliminated||[];
  }catch(e){
    elim=await processClick(board,size,x,y,curPlayer,null,COLORS[curPlayer]);
  }
  for(let e of elim){eliminatedPlayers.add(e);playElim()}
  recordHistory();
  renderBoard(true);
  let alive=[];
  for(let p=0;p<maxPlayers;p++){
    if(eliminatedPlayers.has(p))continue;
    alive.push(p);
  }
  if(alive.length<=1){
    gameOver=true;
    playGameOver();
    showSettlement(alive[0],_colorNames||COLOR_NAMES,gameHistory);
    return;
  }
  let idx=alive.indexOf(curPlayer);
  if(idx<0)idx=0;
  curPlayer=alive[(idx+1)%alive.length];
  renderPlayerBar();
  setBg(curPlayer);
  if(aiPlayers.has(curPlayer)&&!gameOver)setTimeout(()=>triggerAI(),400);
}
function updateUndoBtn(){
  let btn=document.getElementById('undoBtn');
  if(btn)btn.disabled=undoStack.length===0||gameOver||aiThinking;
}
function hidePauseOverlay(){
  let overlay=document.getElementById('pauseOverlay');
  overlay.style.display='none';
  // 清理图表 DOM 释放内存
  document.getElementById('pauseStats').innerHTML='';
  document.getElementById('pauseCharts').innerHTML='';
  // 回收 Canvas 资源
  ['pauseChartPieces','pauseChartPoints'].forEach(id=>{
    let el=document.getElementById(id);
    if(el){el.width=0;el.height=0}
  });
}
function resumeGame(){
  isPaused=false;
  document.getElementById('pauseBtn').textContent='暂停';
  hidePauseOverlay();
  updateUndoBtn();
  // 如果是 AI 回合，继续触发 AI
  if(aiPlayers.has(curPlayer)&&!gameOver)setTimeout(()=>triggerAI(),300);
}
async function endGameNow(){
  // 关闭暂停覆盖层
  hidePauseOverlay();
  isPaused=false;
  clearSavedGameState();
  if(gameOver){
    exitGame();
    return;
  }
  // 统计存活人数
  let survivorCount = 0;
  for(let p=0;p<maxPlayers;p++){
    if(!eliminatedPlayers.has(p) && hasPieces(p, board)){
      survivorCount++;
    }
  }
  if(survivorCount >= 2){
    // 2+ 存活 → 保存未完成记录，退回主菜单
    saveUnfinishedGameHistory();
    gameOver = true;
    exitGame();
    return;
  }
  // 仅 1 人存活 → 正常结束，结算
  gameOver=true;
  updateUndoBtn();
  let bestP = 0, bestCnt = -1;
  for(let p=0;p<maxPlayers;p++){
    if(eliminatedPlayers.has(p))continue;
    let cnt=0;
    for(let r of board)for(let c of r)if(c.owner===p)cnt++;
    if(cnt>bestCnt){bestCnt=cnt;bestP=p}
  }
  recordHistory();
  playGameOver();
  const fullHistory = await flushAndGetFullHistory();
  showSettlement(bestCnt>0?bestP:null, _colorNames||COLOR_NAMES, fullHistory);
}

async function animateBoardDiff(oldBoard, newBoard, sz){
  // 对比老棋盘和新棋盘，找出变化的格子逐个展示爆炸动画
  let changed=[];
  for(let i=0;i<sz;i++)for(let j=0;j<sz;j++){
    let o=oldBoard[i][j],n=newBoard[i][j];
    if(o.owner!==null&&(n.owner===null||n.count<o.count||n.owner!==o.owner)){
      changed.push([i,j]);
    }
  }
  if(changed.length===0){renderBoard(true);return}
  chainSkipAll=false;
  if(autoSkipChain){renderBoard(true);return}
  showSkipBtn(true);
  for(let[cx,cy]of changed){
    if(chainSkipAll)break;
    renderBoard(true);
    let el=cells?.[cx]?.[cy];
    if(el)el.classList.add('explode');
    await sleep(180);
    if(el)el.classList.remove('explode');
  }
  showSkipBtn(false);
  renderBoard(true);
}

async function localClick(x,y){
  if(gameOver||aiThinking||isPaused)return;
  // AI 首子由玩家放置：轮到 AI 且 AI 无棋子时，人类可点击为空 AI 放置首子
  if(aiPlayers.has(curPlayer) && hasPieces(curPlayer,board)){
    showMsg('轮到 AI 思考，请等待', '');
    return;
  }
  if(aiPlayers.has(curPlayer)){
    // AI 无棋子：人类点击为空 AI 放置首子
    let c=board[x][y];
    if(c.owner!==null){showMsg('该位置已有棋子','');return}
    if(isInRestrictedZone(x,y,size,board)){
      showMsg('该位置在已有棋子限制区域内','');return
    }
    saveUndoState();
    playClick();
    const clickEl=cells?.[x]?.[y];
    if(clickEl)addRipple(clickEl);
    let elim;
    try {
      let result=await tauriInvoke('process_move',{
        board:board,size:size,x:x,y:y,
        player:curPlayer,maxPlayers:maxPlayers
      });
      elim=await processClick(board,size,x,y,curPlayer,'explode',COLORS[curPlayer]);
      board=result.board;
      elim=result.eliminated||[];
    }catch(e){
      elim=await processClick(board,size,x,y,curPlayer,'explode',COLORS[curPlayer]);
    }
    for(let e of elim){eliminatedPlayers.add(e);playElim()}
    recordHistory();
    renderBoard(true);
    let alive=[];
    for(let p=0;p<maxPlayers;p++){
      if(eliminatedPlayers.has(p))continue;
      alive.push(p);
    }
    if(alive.length<=1){
      gameOver=true;
      playGameOver();
      showSettlement(alive[0],_colorNames||COLOR_NAMES,gameHistory);
      return;
    }
    let idx=alive.indexOf(curPlayer);
    if(idx<0)idx=0;
    curPlayer=alive[(idx+1)%alive.length];
    renderPlayerBar();
    setBg(curPlayer);
    if(aiPlayers.has(curPlayer)&&!gameOver)setTimeout(()=>triggerAI(),400);
    return;
  }
  saveUndoState();
  let c=board[x][y];
  let mine=hasPieces(curPlayer,board);
  if(mine){
    if(c.owner!==curPlayer){showMsg('只能点击自己的棋子','');return}
  }else{
    if(c.owner!==null){showMsg('不能抢占别人的格子','');return}
    if(isInRestrictedZone(x,y,size,board)){
      showMsg('该位置在已有棋子限制区域内','');return
    }
  }
  playClick();
  // 落子波纹视觉反馈
  const clickEl=cells?.[x]?.[y];
  if(clickEl)addRipple(clickEl);
  let elim;
  try {
    let result=await tauriInvoke('process_move',{
      board:board,size:size,x:x,y:y,
      player:curPlayer,maxPlayers:maxPlayers
    });
    elim=await processClick(board,size,x,y,curPlayer,'explode',COLORS[curPlayer]);
    board=result.board;
    elim=result.eliminated||[];
  }catch(e){
    elim=await processClick(board,size,x,y,curPlayer,'explode',COLORS[curPlayer]);
  }
  for(let e of elim){eliminatedPlayers.add(e);playElim()}
  recordHistory();
  renderBoard(true);
  let alive=[];
  for(let p=0;p<maxPlayers;p++){
    if(eliminatedPlayers.has(p))continue;
    alive.push(p);
  }
  if(alive.length<=1){
    gameOver=true;
    playGameOver();
    showSettlement(alive[0],_colorNames||COLOR_NAMES,gameHistory);
    return;
  }
  let idx=alive.indexOf(curPlayer);
  if(idx<0)idx=0;
  curPlayer=alive[(idx+1)%alive.length];
  renderPlayerBar();
  setBg(curPlayer);
  if(aiPlayers.has(curPlayer)&&!gameOver)setTimeout(()=>triggerAI(),400);
}

function handleClick(x,y){
  if(!aiThinking&&!isPaused)localClick(x,y);
}

/* ==================== SETTLEMENT ==================== */
function getPlayerLabel(p){
  if(p===undefined||p===null)return '未知';
  if(_colorNames&&_colorNames[p])return _colorNames[p];
  return aiPlayers.has(p)?`AI ${p+1}`:`玩家 ${p+1}`;
}

function drawLineChart(canvas,history,colorNames,colors,valueKey){
  if(!canvas||history.length<1)return;
  let dpr=window.devicePixelRatio||1;
  let rect=canvas.getBoundingClientRect();
  canvas.width=rect.width*dpr;
  canvas.height=rect.height*dpr;
  let ctx=canvas.getContext('2d');
  ctx.scale(dpr,dpr);
  let w=rect.width,h=rect.height;
  let pad={top:18,right:8,bottom:22,left:32};
  let cw=w-pad.left-pad.right,ch=h-pad.top-pad.bottom;
  let players=new Set();
  let maxVal=0;
  for(let turn of history){
    for(let pidStr in turn.snapshot){
      let pid=parseInt(pidStr);
      players.add(pid);
      let v=turn.snapshot[pidStr][valueKey];
      if(v>maxVal)maxVal=v;
    }
  }
  // 总数计算
  let totalByTurn=[];
  for(let turn of history){
    let total=0;
    for(let pidStr in turn.snapshot)total+=turn.snapshot[pidStr][valueKey];
    totalByTurn.push(total);
    if(total>maxVal)maxVal=total;
  }
  if(maxVal===0)maxVal=1;
  let sortedPlayers=[...players].sort((a,b)=>a-b);
  ctx.strokeStyle='rgba(128,128,128,.25)';
  ctx.lineWidth=1;
  for(let i=0;i<=4;i++){
    let y=pad.top+(ch*i)/4;
    ctx.beginPath();ctx.moveTo(pad.left,y);ctx.lineTo(w-pad.right,y);ctx.stroke();
  }
  ctx.fillStyle='rgba(128,128,128,.55)';
  ctx.font='9px sans-serif';
  ctx.textAlign='right';
  for(let i=0;i<=4;i++){
    let y=pad.top+(ch*i)/4;
    let val=Math.round(maxVal*(1-i/4));
    ctx.fillText(val,pad.left-4,y+3);
  }
  ctx.textAlign='center';
  let turnCount=history.length;
  for(let i=0;i<turnCount;i++){
    if(turnCount>15&&i%Math.ceil(turnCount/10)!==0)continue;
    let x=pad.left+(cw*i)/(turnCount-1||1);
    ctx.fillText(i,x,h-4);
  }
  // 总数灰线
  ctx.strokeStyle='rgba(128,128,128,.55)';
  ctx.lineWidth=2;ctx.setLineDash([4,3]);
  ctx.beginPath();
  let startedLine=false;
  for(let i=0;i<history.length;i++){
    let total=totalByTurn[i];
    let x=pad.left+(cw*i)/(history.length-1||1);
    let y=pad.top+ch*(1-total/maxVal);
    if(!startedLine){ctx.moveTo(x,y);startedLine=true;}
    else ctx.lineTo(x,y);
  }
  ctx.stroke();ctx.setLineDash([]);
  for(let pid of sortedPlayers){
    let color=colors[pid]||'#888';
    ctx.strokeStyle=color;
    ctx.lineWidth=2;
    ctx.beginPath();
    let started=false;
    for(let i=0;i<history.length;i++){
      let v=history[i].snapshot[String(pid)]?.[valueKey]||0;
      let x=pad.left+(cw*i)/(history.length-1||1);
      let y=pad.top+ch*(1-v/maxVal);
      if(!started){ctx.moveTo(x,y);started=true;}
      else ctx.lineTo(x,y);
    }
    ctx.stroke();
  }
  // 保存图表上下文供全屏使用
  if(canvas.id){
    _chartCtx[canvas.id]={history,colorNames:colorNames||colors,colors,valueKey};
  }
}

// ─── 统一图表渲染（暂停/结算/checkout 共用） ───
function renderGameCharts(container, history, opts){
  const {winner,colorNames,colors,eliminated,chainStats,maxChain,showTitle,onReplay,_noBack}=opts||{};
  const cNames=colorNames||colors||[];
  const lastSnap=history.length>0?history[history.length-1].snapshot:{};
  if(showTitle&&winner!==null&&winner!==undefined){
    let t=document.createElement('h2');
    t.innerHTML=`<span class="winner-color" style="background:${colors[winner]||'#888'}"></span>${cNames[winner]||getPlayerLabel(winner)} 获胜`;
    container.appendChild(t);
  }else if(showTitle&&winner===undefined){
    let t=document.createElement('h2');t.textContent='对局统计';container.appendChild(t);
  }
  let grid=document.createElement('div');grid.className='stats-grid';
  let active=[];
  for(let p=0;p<maxPlayers;p++){
    if(eliminated&&eliminated.has(p))continue;
    let snap=lastSnap[String(p)];
    if(snap&&snap.pieces>0)active.push(p);
  }
  if(active.length===0&&winner!==null&&winner!==undefined)active=[winner];
  for(let pid of active){
    let snap=lastSnap[String(pid)],pieces=snap?snap.pieces:0,points=snap?snap.points:0;
    let card=document.createElement('div');card.className='stat-card';
    if(pid===winner)card.style.border=`1px solid ${colors[pid]||'#888'}`;
    let cs=chainStats&&chainStats[pid];
    let csStr=cs&&cs.triggered>0?`<div class="lbl" style="margin-top:4px">连爆 ${cs.triggered} 次 · 最高 ${cs.maxChain} 连</div>`:'';
    card.innerHTML=`<div class="name"><span class="color-dot" style="background:${colors[pid]||'#888'}"></span>${cNames[pid]||getPlayerLabel(pid)} ${pid===winner?' ★':''}</div>
      <div class="num" style="color:${colors[pid]||'#888'}">${pieces}</div><div class="lbl">棋子</div>
      <div class="num" style="color:${colors[pid]||'#888'}">${points}</div><div class="lbl">点数</div>${csStr}`;
    grid.appendChild(card);
  }
  container.appendChild(grid);
  if(maxChain&&maxChain.player!==null&&maxChain.length>0){
    let ci=document.createElement('p');
    ci.style.cssText='font-size:.75rem;color:var(--dim);margin:4px 0;text-align:center;';
    // 使用 DOM 方法避免 XSS
    ci.innerHTML='最高连爆：<span id="maxChainColor"></span> 触发 <strong></strong> 连爆';
    const colorSpan=ci.querySelector('span');
    if(colorSpan)colorSpan.style.color=colors[maxChain.player]||'#888';
    const strongEl=ci.querySelector('strong');
    if(strongEl)strongEl.textContent=String(maxChain.length);
    const nameText=document.createTextNode(cNames[maxChain.player]||getPlayerLabel(maxChain.player));
    if(colorSpan)colorSpan.appendChild(nameText);
    container.appendChild(ci);
  }
  if(history&&history.length>=2){
    let id1='cP_'+Date.now()+'_'+Math.random().toString(36).slice(2,6);
    let id2='cPt_'+Date.now()+'_'+Math.random().toString(36).slice(2,6);
    let b1=document.createElement('div');b1.className='chart-box';
    b1.innerHTML=`<h4 style="cursor:pointer" onclick="showFullscreenChart('${id1}')">棋子数变化 🔍</h4><canvas id="${id1}" onclick="showFullscreenChart('${id1}')" style="cursor:pointer"></canvas>`;
    container.appendChild(b1);
    let b2=document.createElement('div');b2.className='chart-box';
    b2.innerHTML=`<h4 style="cursor:pointer" onclick="showFullscreenChart('${id2}')">点数变化 🔍</h4><canvas id="${id2}" onclick="showFullscreenChart('${id2}')" style="cursor:pointer"></canvas>`;
    container.appendChild(b2);
    // 等待 Canvas 可见后再绘制（兼容 Router 页面过渡延迟）
    function drawWhenReady(cid,h,cn,co,k){
      const el=document.getElementById(cid);
      if(!el)return;
      (function poll(){
        const r=el.getBoundingClientRect();
        if(r.width>0&&r.height>0)drawLineChart(el,h,cn,co,k);
        else requestAnimationFrame(poll);
      })();
    }
    drawWhenReady(id1,history,cNames,colors,'pieces');
    drawWhenReady(id2,history,cNames,colors,'points');
  }
  if(onReplay){let btn=document.createElement('button');btn.className='glass-btn primary';btn.textContent='再来一局';btn.onclick=onReplay;container.appendChild(btn)}
  if(!_noBack){
    let bh=document.createElement('button');bh.className='glass-btn primary';bh.textContent='返回主界面';bh.style.marginTop='6px';bh.onclick=()=>Router.navigate('welcome');container.appendChild(bh);
  }
}

// 根据保存的配置直接重开游戏（不再读 DOM，确保与上次配置完全一致）
function replayGame(){
  const c=_lastGameConfig;
  if(!c)return;
  _colorNames=c.colorNames||null;
  if(c.mode==='ai'){
    clearBoardDOM();
    location.hash='#game';
    resetRoundHistory();
    undoStack=[];
    size=c.size;
    maxPlayers=c.aiCount+1;
    aiAlgorithm=c.aiAlgorithm||'strategy';
    selectedPlayerColor=c.humanIdx;
    board=mkBoard(size);curPlayer=0;gameOver=false;isPaused=false;firstMovePos=null;
    document.getElementById('pauseBtn').textContent='暂停';
    gameMode='ai';
    aiPlayers=new Set();
    aiConfigs=c.aiConfigs?JSON.parse(JSON.stringify(c.aiConfigs)):{};
    for(let p=0;p<maxPlayers;p++){if(p!==c.humanIdx)aiPlayers.add(p)}
    aiThinking=false;
    eliminatedPlayers=new Set();
    gameHistory=[];
    chainStats={};
    maxChainOverall={player:null,length:0};
    gameCount=(gameCount||0)+1;
    recordHistory();
    renderBoard(true);
    show('game');
    document.body.style.background='';
    renderPlayerBar();
    if(aiPlayers.has(0))setTimeout(()=>triggerAI(),400);
  }else if(c.mode==='local'){
    clearBoardDOM();
    location.hash='#game';
    resetRoundHistory();
    undoStack=[];
    size=c.size;
    maxPlayers=c.maxPlayers;
    board=mkBoard(size);curPlayer=0;gameOver=false;isPaused=false;firstMovePos=null;
    document.getElementById('pauseBtn').textContent='暂停';
    gameMode='local';
    aiPlayers=new Set();
    aiThinking=false;
    eliminatedPlayers=new Set();
    gameHistory=[];
    chainStats={};
    maxChainOverall={player:null,length:0};
    recordHistory();
    renderBoard(true);
    show('game');
    document.body.style.background='';
    renderPlayerBar();
  }else if(c.mode==='eve'){
    clearBoardDOM();
    location.hash='#game';
    resetRoundHistory();
    undoStack=[];
    size=c.size;
    maxPlayers=c.maxPlayers||c.aiCount;
    board=mkBoard(size);curPlayer=0;gameOver=false;isPaused=false;firstMovePos=null;
    document.getElementById('pauseBtn').textContent='暂停';
    gameMode='eve';
    aiPlayers=new Set();
    aiConfigs={};
    for(let p=0;p<maxPlayers;p++){
      aiPlayers.add(p);
      aiConfigs[p]={algorithm:(c.aiConfigs[p]||{}).algorithm||'strategy',depth:(c.aiConfigs[p]||{}).depth||2,randomScale:(c.aiConfigs[p]||{}).randomScale??10};
    }
    aiThinking=false;
    eliminatedPlayers=new Set();
    gameHistory=[];
    chainStats={};
    maxChainOverall={player:null,length:0};
    gameCount=(gameCount||0)+1;
    recordHistory();
    renderBoard(true);
    show('game');
    document.body.style.background='';
    renderPlayerBar();
    if(aiPlayers.has(0))setTimeout(()=>triggerAI(),400);
  }
}

// ─── 继续未完成的游戏 ───
async function continueGame(){
  let saved = await loadSavedGameState();
  if(!saved || !saved.board || !saved.size){
    try {
      const raw = localStorage.getItem('unfinishedGameState');
      if(raw) saved = JSON.parse(raw);
    } catch(e) {}
  }
  if(!saved || !saved.board || !saved.size){
    showMsg('未找到可继续的游戏记录','');
    return;
  }
  // 消耗旧记录（清除 localStorage + 后端，标记历史记录为已用）
  let recordIdToDelete = null;
  try {
    const rawRec = localStorage.getItem('unfinishedGameRecord');
    if(rawRec){
      const rec = JSON.parse(rawRec);
      if(rec && rec.id) recordIdToDelete = rec.id;
    }
  } catch(e) {}
  clearSavedGameState();
  if(recordIdToDelete){
    tauriInvoke('delete_game_history_record', {recordId: recordIdToDelete}).catch(()=>{});
  }
  loadGameFromState(saved);
}

async function showSettlement(winner,colorNames,history){
  // 清理动态 settlement overlay（如果有残留）
  document.querySelectorAll('.settlement').forEach(e=>{if(e.id!=='pauseOverlay')e.remove()});
  clearSavedGameState();
  const fullHistory = await flushAndGetFullHistory();
  // 设置外框标题（放在玻璃框上方，作为页面固定标题）
  const titleEl = document.getElementById('chkOuterTitle');
  if(winner!==null&&winner!==undefined){
    titleEl.innerHTML = `<span class="winner-color" style="background:${COLORS[winner]||'#888'}"></span> ${colorNames?.[winner]||getPlayerLabel(winner)} 获胜`;
  }else{
    titleEl.textContent = '对局统计';
  }
  // 渲染到 #checkout 页面（包 settlement 容器使 CSS 选择器生效）
  const prevPage = _originPage || 'welcome';
  const content = document.getElementById('checkoutContent');
  content.innerHTML = '<div class="settlement"><div class="settlement-inner" id="chkInner"></div></div>';
  const inner = document.getElementById('chkInner');
  renderGameCharts(inner, fullHistory, {
    winner, colorNames, colors: COLORS, eliminated: eliminatedPlayers,
    chainStats, maxChain: maxChainOverall, showTitle: false, _noBack: true,
    onReplay: () => { Router.navigate(_originPage || 'welcome'); }
  });
  // 返回主菜单按钮
  const homeBtn = document.createElement('button');
  homeBtn.className = 'glass-btn primary';
  homeBtn.textContent = '返回主菜单';
  homeBtn.style.marginTop = '6px';
  homeBtn.onclick = () => { Router.navigate('welcome'); };
  inner.appendChild(homeBtn);
  Router.navigate('checkout', prevPage);
  saveGameHistory(winner, undefined, undefined, undefined, fullHistory);
}

function showHistoryDetail(r){
  const prevPage = 'history';
  // 设置外框标题
  const titleEl = document.getElementById('chkOuterTitle');
  if(r.winner!==null&&r.winner!==undefined){
    // 使用 DOM 方法避免 XSS：用户数据只通过 textContent 插入
    titleEl.innerHTML = '<span class="winner-color"></span> 获胜';
    const wSpan = titleEl.querySelector('.winner-color');
    if(wSpan) wSpan.style.background = COLORS[r.winner]||'#888';
    const wName = document.createTextNode(r.colorNames?.[r.winner]||'玩家 '+(r.winner+1));
    titleEl.insertBefore(wName, titleEl.lastChild);
  }else{
    titleEl.textContent = '对局统计';
  }
  const content = document.getElementById('checkoutContent');
  content.innerHTML = '<div class="settlement"><div class="settlement-inner" id="chkHistoryInner"></div></div>';
  const inner = document.getElementById('chkHistoryInner');
  // 展开紧凑格式（若为旧版全量格式则原样返回）
  const historyData = expandHistory(r.history, r.playerCount||maxPlayers);
  if(!historyData||historyData.length<2){
    inner.innerHTML = '<p style="text-align:center;color:var(--dim);margin-top:12px">该记录无完整快照数据，无法展示图表</p>';
    Router.navigate('checkout', prevPage);
    return;
  }
  // 用记录中 playerCount 临时覆盖 maxPlayers 以复用统一渲染，不显示默认"返回主界面"
  const origMax = maxPlayers;
  maxPlayers = r.playerCount || maxPlayers;
  renderGameCharts(inner, historyData, {
    winner: r.winner, colorNames: r.colorNames, colors: COLORS,
    chainStats: r.chainStats, maxChain: r.maxChain,
    showTitle: false, _noBack: true,
  });
  // 添加"关闭"按钮（还原 maxPlayers）
  const closeBtn = document.createElement('button');
  closeBtn.className = 'glass-btn primary';
  closeBtn.textContent = '关闭';
  closeBtn.onclick = () => { maxPlayers = origMax; Router.back(); };
  inner.appendChild(closeBtn);
  Router.navigate('checkout', prevPage);
}

function showFullscreenChart(canvasId){
  let ctx=_chartCtx[canvasId];
  if(!ctx)return;
  // ---------- 创建全屏覆盖层 ----------
  let overlay=document.createElement('div');
  overlay.dataset.fschart='1';
  overlay.style.cssText='position:fixed;inset:0;z-index:2000;background:rgba(0,0,0,.93);display:flex;justify-content:center;align-items:center;padding:48px 20px 20px;animation:fadeIn .2s ease';
  // 压入一条虚拟历史，确保安卓侧滑/返回键触发 popstate 时先关闭预览（12 => 7,8,9）
  history.pushState(null,'');
  window._chartFsState=1;
  // 上方多留空间，避免图标紧贴边缘
  // overflow 不设 hidden，缩放后的内容可溢出显示
  let closeBtn=document.createElement('button');
  closeBtn.textContent='✕';
  closeBtn.style.cssText='position:fixed;top:16px;right:16px;z-index:2002;width:36px;height:36px;border-radius:50%;border:1px solid rgba(255,255,255,.15);background:rgba(0,0,0,.5);color:#fff;font-size:1.1rem;cursor:pointer;display:flex;align-items:center;justify-content:center;transition:.15s';
  closeBtn.onmouseover=()=>{closeBtn.style.background='rgba(255,255,255,.15)'};
  closeBtn.onmouseout=()=>{closeBtn.style.background='rgba(0,0,0,.5)'};
  // 容器：初始无 max 限制（transform 会让视觉尺寸变化）
  let container=document.createElement('div');
  container.style.cssText='display:flex;justify-content:center;align-items:center;transform-origin:center center';
  let bigCanvas=document.createElement('canvas');
  bigCanvas.style.cssText='max-width:90vw;max-height:90vh;border-radius:10px;box-shadow:0 8px 40px rgba(0,0,0,.5);cursor:grab;user-select:none;-webkit-user-select:none';
  bigCanvas.onclick=(e)=>e.stopPropagation();
  overlay.appendChild(container);
  container.appendChild(bigCanvas);
  overlay.appendChild(closeBtn);
  document.body.appendChild(overlay);

  // ----- 侧滑/触摸：阻止事件穿透到下层页面 -----
  let swipeStartX=null;
  overlay.addEventListener('touchstart',e=>{
    // 阻止 touch 事件穿透到下方页面
    e.stopPropagation();
    // 记录背景区域的触摸起点用于侧滑关闭检测
    if(e.target===overlay||e.target===closeBtn){
      swipeStartX=e.touches[0].clientX;
    }else{
      swipeStartX=null;
    }
  },{passive:true});

  overlay.addEventListener('touchmove',e=>{
    // 阻止 touch 事件穿透
    e.stopPropagation();
  },{passive:true});

  overlay.addEventListener('touchend',e=>{
    e.stopPropagation();
    // 检测水平侧滑关闭（背景区域或关闭按钮上的滑动）
    if(swipeStartX!==null){
      const endX=e.changedTouches[0].clientX;
      const dx=endX-swipeStartX;
      // 向右滑动超过 40px 视为侧滑关闭
      if(dx>40){
        closeFsOverlay();
      }
    }
    swipeStartX=null;
  },{passive:true});

  // ---------- 从数据重绘 ----------
  let baseW=0,baseH=0;
  setTimeout(()=>{
    let rect=bigCanvas.getBoundingClientRect();
    if(rect.width===0||rect.height===0)return;
    baseW=rect.width;baseH=rect.height;
    let dpr=window.devicePixelRatio||1;
    bigCanvas.width=baseW*dpr;
    bigCanvas.height=baseH*dpr;
    bigCanvas.style.width=baseW+'px';
    bigCanvas.style.height=baseH+'px';
    drawLineChart(bigCanvas,ctx.history,ctx.colorNames,ctx.colors,ctx.valueKey);
  },50);
  // ---------- 缩放 + 平移状态 ----------
  let scale=1,tx=0,ty=0;
  let isDragging=false;
  let dragStartX=0,dragStartY=0,panStartX=0,panStartY=0;
  let lastDist=0;
  function applyTransform(){
    container.style.transform=`translate(${tx}px,${ty}px) scale(${scale})`;
  }
  // ----- 统一关闭函数（清理所有事件监听） -----
  function closeFsOverlay(){
    document.removeEventListener('mousemove',onMouseMove);
    document.removeEventListener('mouseup',onMouseUp);
    if(overlay.parentNode)overlay.remove();
    // 弹回虚拟历史，并标记下次 popstate 跳过页面导航
    if(window._chartFsState){
      window._chartFsState=0;
      window._chartFsManualClose=1;  // 标记手动关闭，防止 popstate 误导航
      history.back();
    }
  }
  // ------- 鼠标拖拽平移（缩放后才可拖动） -------
  function onMouseDown(e){
    if(scale>1){
      isDragging=true;
      dragStartX=e.clientX;dragStartY=e.clientY;
      panStartX=tx;panStartY=ty;
      bigCanvas.style.cursor='grabbing';
    }
  }
  function onMouseMove(e){
    if(isDragging){
      tx=panStartX+(e.clientX-dragStartX);
      ty=panStartY+(e.clientY-dragStartY);
      applyTransform();
    }
  }
  function onMouseUp(){
    if(isDragging){
      isDragging=false;
      bigCanvas.style.cursor='grab';
    }
  }
  bigCanvas.addEventListener('mousedown',onMouseDown);
  document.addEventListener('mousemove',onMouseMove);
  document.addEventListener('mouseup',onMouseUp);
  // 背景点击或关闭按钮 -> 关闭
  overlay.onclick=function(e){
    if(e.target===overlay||e.target===closeBtn){
      closeFsOverlay();
    }
  };
  // ------- 双指缩放 + 单指平移（touch）-------
  bigCanvas.addEventListener('touchstart',e=>{
    e.preventDefault();
    if(e.touches.length===2){
      let dx=e.touches[0].clientX-e.touches[1].clientX;
      let dy=e.touches[0].clientY-e.touches[1].clientY;
      lastDist=Math.sqrt(dx*dx+dy*dy);
    }else if(e.touches.length===1&&scale>1){
      let t=e.touches[0];
      isDragging=true;
      dragStartX=t.clientX;dragStartY=t.clientY;
      panStartX=tx;panStartY=ty;
    }
  },{passive:false});
  bigCanvas.addEventListener('touchmove',e=>{
    e.preventDefault();
    if(e.touches.length===2){
      let dx=e.touches[0].clientX-e.touches[1].clientX;
      let dy=e.touches[0].clientY-e.touches[1].clientY;
      let dist=Math.sqrt(dx*dx+dy*dy);
      if(lastDist>0){
        let delta=dist/lastDist;
        scale=Math.max(0.5,Math.min(5,scale*delta));
        applyTransform();
      }
      lastDist=dist;
    }else if(e.touches.length===1&&isDragging&&scale>1){
      let t=e.touches[0];
      tx=panStartX+(t.clientX-dragStartX);
      ty=panStartY+(t.clientY-dragStartY);
      applyTransform();
    }
  },{passive:false});
  bigCanvas.addEventListener('touchend',e=>{
    e.preventDefault();
    lastDist=0;isDragging=false
  },{passive:false});
  // ------- 鼠标滚轮缩放 -------
  overlay.addEventListener('wheel',e=>{
    e.preventDefault();
    let newScale=Math.max(0.5,Math.min(5,scale+(e.deltaY>0?-0.1:0.1)));
    scale=newScale;
    applyTransform();
  },{passive:false});
}

/* ==================== GAME ACTIONS ==================== */
function exitGame(){
  // 退出时清理已保存状态（不再自动保存，由 endGameNow 管理）
  clearSavedGameState();
  // 刷盘残留历史再退出，清理磁盘回合数据
  if(gameHistory.length>0){flushAndGetFullHistory();tauriInvoke('clear_round_history').catch(e=>logWarn('Clear round history on exit failed:', e))}
  gameMode=null;gameOver=false;isPaused=false;firstMovePos=null;curPlayer=0;aiThinking=false;
  aiPlayers=new Set();eliminatedPlayers=new Set();gameHistory=[];chainStats={};
  maxChainOverall={player:null,length:0};turnCount=0;gameCount=0;undoStack=[];
  cells=[];board=[];
  try{document.getElementById('board').innerHTML='';}catch(e){}
  document.body.style.background='';
  // ★ 只移除动态 settlement，保留静态 #pauseOverlay，关闭模态
  document.querySelectorAll('.settlement').forEach(e=>{if(e.id!=='pauseOverlay')e.remove()});
  try{document.getElementById('pauseOverlay').style.display='none';}catch(e){}
  document.querySelectorAll('.modal').forEach(e=>e.classList.remove('show'));
  // ★ 重新显示 game screen 中的暂停按钮和消息，确保 DOM 干净
  try{document.getElementById('pauseBtn').textContent='暂停';}catch(e){}
  showMsg('','');
  Router.navigate('welcome');
}

/* ==================== ABOUT ==================== */
function renderChangelogCards(){
  var container=document.getElementById('changelogContainer');
  if(!container)return;
  var versions=[
    {v:'v3.1.1 · 第 24 版',desc:'ML 模型迭代自对弈训练（16维改进特征，800树，AUC 0.898），ML vs 手写 83% 胜率，颜色称谓修复，去除 beta 标记'},
    {v:'v3.1.1-beta · 第 23 版',desc:'增强版 AI 评估函数（位置权重+威胁评分+爆发势能），nbrs() 零分配性能优化，死代码清理，AI 首子由玩家放置，README/About 页面重写'},
    {v:'v3.0.0-beta · 第 21 版',desc:'排版变量体系重构，自定义AI选择弹窗，rand库随机化AI引擎，跳过按钮纯按钮化，README+About重写'},
    {v:'v2.3.9-beta · 第 19 版',desc:'开始游戏卡片和按钮放大、进行中游戏可放弃、历史图表修复、版本号回退 2.3.9-beta'},
  {v:'v3.0.0-beta · 第 18 版',desc:'v3.0 全新架构重写：全按钮统一 glass-btn 风格、可点击块统一 mode-card 风格、AI 对战支持逐个 AI 独立配置算法/深度、颜色选择排除 AI 使用'},
    {v:'v2.3.8 · 第 17 版',desc:'配色优化（奶白/深灰色）、震动反馈、双向棋盘人数限制、Cargo 编译优化'},
    {v:'v2.3.8 · 第 17 版',desc:'PVS 搜索性能优化：QSearch 深度收窄、Cell Copy、邻域展开去 Vec 分配、Null Window 跳过 QSearch'},
    {v:'v2.3.6-beta · 第 15 版',desc:'AI 代码全量迁移至 Rust 引擎，终局判断优化，开场随机性调整'},
    {v:'v2.3.6-beta · 第 14 版',desc:'中断游戏持久化：异常/暂停退出自动存为历史记录，历史页可继续未完成对局'},
    {v:'v2.3.4-beta · 第 12 版',desc:'安卓侧滑手势优化 & 退出确认：图表预览侧滑关闭、游戏暂停切换、退出确认弹窗'},
    {v:'v2.3.3-beta · 第 11 版',desc:'UI 统一：动画开关与悔棋/暂停按钮样式统一，版本号更新至 2.3.3-beta'},
    {v:'v2.3.1-alpha · 第 10 版',desc:'版本号更新至 2.3.1-alpha'},
    {v:'v2.3.0-beta · 第 9 版',desc:'Liquid Glass 主题优化：虹彩渐变玻璃、流体动画曲线、统一圆角与阴影体系、环境光晕背景'},
    {v:'v2.3.0-beta · 第 8 版',desc:'关于页重构：行内显示版本信息，长内容改为二级页面导航，统一玻璃主题'},
    {v:'v2.3.0-beta · 第 7 版',desc:'界面精简：减少 emoji 使用量（33 处替换），保留品牌标识和标准 UI 符号'},
    {v:'v2.3.0-beta · 第 6 版',desc:'暂停页显示修复：防止 Android WebView 恢复旧状态，新增返回主菜单按钮'},
    {v:'v2.3.0-beta · 第 5 版',desc:'UI/CSS 修复：补全缺失 CSS 变量 --glass-w-015，模态框背景适配浅色模式'},
    {v:'v2.3.0-beta · 第 4 版',desc:'无障碍优化：移除禁止缩放，新增 prefers-reduced-motion，触摸达标，屏幕阅读器支持'},
    {v:'v2.3.0-beta · 第 3 版',desc:'前端优化：消除 7 处静默错误吞噬，优化棋盘缓存内存使用'},
    {v:'v2.3.0-beta · 第 2 版',desc:'安全性增强：密码移除硬编码、启用 CSP、修复 3 处 XSS 向量'},
    {v:'v2.3.0-beta · 第 1 版',desc:'Rust 后端：修复 unset 空实现、u8 溢出防护、文件写入原子化、版本号统一'}
  ];
  for(var i=0;i<versions.length;i++){
    var card=document.createElement('div');card.className='cl-card';
    var vEl=document.createElement('div');vEl.className='cl-version';vEl.textContent=versions[i].v;
    var dEl=document.createElement('div');dEl.className='cl-desc';dEl.textContent=versions[i].desc;
    card.appendChild(vEl);card.appendChild(dEl);
    container.appendChild(card);
  }
}


/* ==================== UTILS ==================== */
function openModal(id){document.getElementById(id).classList.add('show')}
function closeModal(id){document.getElementById(id).classList.remove('show')}
function showMsg(t,c){
  let el=document.getElementById('msg');el.textContent=t;el.className=c||'';
  if(t)setTimeout(()=>{el.textContent='';el.className=''},3000);
}

let playerConfigs=[];
let editingPlayerIdx=-1;
// 使用已有的 COLOR_NAMES 常量

function openPlayerModal(idx){
  editingPlayerIdx=idx;
  const cfg=playerConfigs[idx];
  const pName=playerConfigs[idx]?.name||COLOR_NAMES[idx]||('玩家'+(idx+1));
  document.getElementById('playerModalTitle').textContent=pName+' 设置';
  document.getElementById('playerNameInput').value=cfg.name||'';
  document.getElementById('depthValue').textContent=String(cfg.depth);
  document.getElementById('randomScaleSlider').value=String(cfg.randomScale);
  document.getElementById('randomValueLabel').textContent=cfg.randomScale+'%';
  // 初始化类型按钮（updateModalRows 必须在此之后调用，否则读到上一轮残留状态）
  var btns=document.getElementById('playerTypeBtns');
  btns.querySelectorAll('.am-btn').forEach(function(b){b.classList.toggle('selected',b.dataset.value===cfg.type)});
  btns.querySelectorAll('.am-btn').forEach(function(b){b.onclick=function(){btns.querySelectorAll('.am-btn').forEach(function(x){x.classList.remove('selected')});this.classList.add('selected');updateModalRows();}});
  updateModalRows();
  document.getElementById('depthDecBtn').onclick=function(){var e=document.getElementById('depthValue');var v=parseInt(e.textContent)||2;if(v>1){v--;e.textContent=v}};
  document.getElementById('depthIncBtn').onclick=function(){var e=document.getElementById('depthValue');var v=parseInt(e.textContent)||2;if(v<10){v++;e.textContent=v}};
  document.getElementById('randomScaleSlider').oninput=function(){document.getElementById('randomValueLabel').textContent=this.value+'%'};
  // 初始化评估函数按钮
  var evalBtns=document.getElementById('playerEvalToggle');
  if(evalBtns){
    evalBtns.querySelectorAll('.tg-btn').forEach(function(b){b.classList.toggle('selected',b.dataset.value===String(cfg.useMlEval!==false))});
  }
  openModal('playerConfigModal');
}
function updateModalRows(){
  var btns=document.getElementById('playerTypeBtns');
  if(!btns)return;
  var sel=btns.querySelector('.am-btn.selected');
  var val=sel?sel.dataset.value:'human';
  var isAI=val!=='human';
  var needDepth=val==='alphabeta'||val==='pvs'||val==='mcts';
  var needEval=val==='alphabeta'||val==='pvs';
  document.getElementById('playerDepthRow').style.display=needDepth?'block':'none';
  document.getElementById('playerRandomRow').style.display=needDepth?'block':'none';
  document.getElementById('playerEvalRow').style.display=needEval?'block':'none';
}
function closePlayerModal(){closeModal('playerConfigModal')}
function savePlayerConfig(){
  const cfg=playerConfigs[editingPlayerIdx];
  cfg.name=document.getElementById('playerNameInput').value.trim();
  var selBtn=document.querySelector('#playerTypeBtns .am-btn.selected');
  cfg.type=selBtn?selBtn.dataset.value:'human';
  cfg.depth=parseInt(document.getElementById('depthValue').textContent)||2;
  cfg.randomScale=parseInt(document.getElementById('randomScaleSlider').value)??10;
  var evalSel=document.querySelector('#playerEvalToggle .tg-btn.selected');
  cfg.useMlEval=evalSel?evalSel.dataset.value==='true':true;
  closePlayerModal();
  generatePlayerConfigs();
}
function generatePlayerConfigs(){
  const cnt=getSel('setupPlayersGroup')||2;
  const sz=getSel('setupSizeGrid')||7;
  const maxP=getMaxPlayersBySize(sz);
  const realCnt=Math.min(cnt,maxP);
  while(playerConfigs.length<realCnt){
    const i=playerConfigs.length;
    playerConfigs.push({name:'',type:i===0?'human':'strategy',depth:2,randomScale:10,useMlEval:true});
  }
  if(playerConfigs.length>realCnt)playerConfigs.length=realCnt;
  const container=document.getElementById('playerConfigList');
  container.innerHTML='';
  for(let i=0;i<realCnt;i++){
    const cfg=playerConfigs[i];
    const div=document.createElement('div');div.className='player-config-item';
    const dot=document.createElement('span');dot.className='color-dot';dot.style.background=COLORS[i%COLORS.length];
    const info=document.createElement('div');info.className='player-info';
    const nameText=cfg.name||('玩家 '+(i+1));
    const typeLabels={human:'人类',strategy:'AI·策略',alphabeta:'AI·A-B',pvs:'AI·PVS',mcts:'AI·MCTS'};
    const typeText=typeLabels[cfg.type]||'人类';
    let detail=typeText;
    if(cfg.type!=='human'&&cfg.type!=='strategy')detail+=' · 深度 '+cfg.depth;
    if(cfg.type!=='human'&&cfg.type!=='strategy')detail+=' · 随机 '+cfg.randomScale+'%';
    if(cfg.type==='alphabeta'||cfg.type==='pvs')detail+=' · '+(cfg.useMlEval!==false?'ML模型':'手写');
    info.innerHTML='<strong>'+nameText+'</strong><span>'+detail+'</span>';
    const hint=document.createElement('span');hint.className='edit-hint';hint.textContent='编辑';
    div.appendChild(dot);div.appendChild(info);div.appendChild(hint);
    div.onclick=function(){openPlayerModal(i)};
    container.appendChild(div);
  }
  // 显示/隐藏评估函数行（有 AI 时显示）
  const er=document.getElementById('setupEvalRow');
  if(er){er.style.display=playerConfigs.some(c=>c.type!=='human')?'flex':'none'}
}

function setupLobbySync(){
  const sz=getSel('setupSizeGrid')||7;
  const cnt=getSel('setupPlayersGroup')||2;
  const maxP=getMaxPlayersBySize(sz);
  const pg=document.getElementById('setupPlayersGroup');
  if(pg){
    pg.querySelectorAll('.gb').forEach(b=>{b.style.display=parseInt(b.dataset.value)<=maxP?'':'none'});
    const sc=pg.querySelector('.selected');
    if(sc&&parseInt(sc.dataset.value)>maxP){
      const f=pg.querySelector('.gb:not([style*="display: none"])');
      if(f)setSelected(pg,f);
    }
  }
  const sg=document.getElementById('setupSizeGrid');
  if(sg){
    sg.querySelectorAll('.size-btn').forEach(b=>{b.style.display=cnt<=getMaxPlayersBySize(parseInt(b.dataset.value))?'':'none'});
    const ss=sg.querySelector('.selected');
    if(ss&&cnt>getMaxPlayersBySize(parseInt(ss.dataset.value))){
      const f=sg.querySelector('.size-btn:not([style*="display: none"])');
      if(f)setSelected(sg,f);
    }
  }
  setTimeout(generatePlayerConfigs,10);
}

function startUnifiedGame(){
  const sz=getSel('setupSizeGrid')||7;
  const cnt=getSel('setupPlayersGroup')||2;
  generatePlayerConfigs();
  let aiCount=0;
  for(let i=0;i<cnt;i++){
    if(playerConfigs[i].type!=='human')aiCount++;
  }
  if(aiCount===0)startLocalFromSetup(sz,cnt);
  else if(aiCount===cnt)startEveFromSetup(sz,cnt);
  else startAIFromSetup(sz,cnt);
}
function startLocalFromSetup(sz,cnt){
  clearSavedGameState();
  location.hash='#game';
  resetRoundHistory();
  undoStack=[];
  size=sz;maxPlayers=cnt;
  board=mkBoard(size);curPlayer=0;gameOver=false;isPaused=false;firstMovePos=null;
  document.getElementById('pauseBtn').textContent='暂停';
  gameMode='local';_originPage='gameSetup';
  aiPlayers=new Set();aiThinking=false;
  eliminatedPlayers=new Set();gameHistory=[];chainStats={};
  maxChainOverall={player:null,length:0};
  let colorNames={};
  for(let i=0;i<cnt;i++){
    const cfg=playerConfigs[i];
    colorNames[i]=cfg.name||COLOR_NAMES[i]||('玩家'+(i+1));
  }
  _colorNames=colorNames;
  recordHistory();
  renderBoard(true);
  show('game');
  document.body.style.background='';
  renderPlayerBar();
  _lastGameConfig={mode:'local',size,maxPlayers,colorNames};
}
function startAIFromSetup(sz,cnt){
  clearSavedGameState();
  location.hash='#game';
  resetRoundHistory();
  undoStack=[];
  size=sz;maxPlayers=cnt;
  board=mkBoard(size);curPlayer=0;gameOver=false;isPaused=false;firstMovePos=null;
  document.getElementById('pauseBtn').textContent='暂停';
  gameMode='ai';_originPage='gameSetup';
  aiPlayers=new Set();aiConfigs={};aiThinking=false;
  eliminatedPlayers=new Set();gameHistory=[];chainStats={};
  maxChainOverall={player:null,length:0};
  let colorNames={},humanIdx=-1;
  for(let i=0;i<cnt;i++){
    const cfg=playerConfigs[i];
    colorNames[i]=cfg.name||COLOR_NAMES[i]||('玩家'+(i+1));
    if(cfg.type==='human'){
      if(humanIdx<0)humanIdx=i;
    }else{
      aiPlayers.add(i);
      aiConfigs[i]={algorithm:cfg.type,depth:cfg.depth,randomScale:cfg.randomScale??10,useMlEval:cfg.useMlEval!==false};
    }
  }
  if(humanIdx<0)humanIdx=0;
  _colorNames=colorNames;
  selectedPlayerColor=humanIdx;
  recordHistory();
  renderBoard(true);
  show('game');
  document.body.style.background='';
  renderPlayerBar();
  if(aiPlayers.has(0))setTimeout(()=>triggerAI(),400);
  _lastGameConfig={mode:'ai',size,aiCount:cnt-1,humanIdx,aiConfigs:JSON.parse(JSON.stringify(aiConfigs)),colorNames};
}
function startEveFromSetup(sz,cnt){
  clearSavedGameState();
  location.hash='#game';
  resetRoundHistory();
  undoStack=[];
  size=sz;maxPlayers=cnt;
  board=mkBoard(size);curPlayer=0;gameOver=false;isPaused=false;firstMovePos=null;
  document.getElementById('pauseBtn').textContent='暂停';
  gameMode='eve';_originPage='gameSetup';
  aiPlayers=new Set();aiConfigs={};aiThinking=false;
  eliminatedPlayers=new Set();gameHistory=[];chainStats={};
  maxChainOverall={player:null,length:0};
  let colorNames={};
  for(let i=0;i<cnt;i++){
    const cfg=playerConfigs[i];
    colorNames[i]=cfg.name||COLOR_NAMES[i]||('AI '+(i+1));
    aiPlayers.add(i);
    aiConfigs[i]={algorithm:cfg.type,depth:cfg.depth,randomScale:cfg.randomScale??10,useMlEval:cfg.useMlEval!==false};
  }
  _colorNames=colorNames;
  recordHistory();
  renderBoard(true);
  show('game');
  document.body.style.background='';
  renderPlayerBar();
  if(aiPlayers.has(0))setTimeout(()=>triggerAI(),400);
  _lastGameConfig={mode:'eve',size,maxPlayers:cnt,aiConfigs:JSON.parse(JSON.stringify(aiConfigs)),colorNames};
}
window.addEventListener('popstate',()=>{
  const cur = Router._current;
  let handled = false;  // 标记是否处理了图层/模态（未切换页面）

  // ── 手动关闭图表预览的 popstate 回弹 ──
  if(window._chartFsManualClose){
    window._chartFsManualClose=0;
    handled = true;
  }

  // ── 图层/模态优先检查 ──
  if(!handled){
    // 1) 全屏图表覆盖层 (ChartPreview · 12) — 关闭预览
    var fsOverlay=document.querySelector('[data-fschart="1"]');
    if(fsOverlay){
      window._chartFsState=0;
      fsOverlay.remove();
      handled = true;
    }
  }

  if(!handled){
    // 2) 暂停覆盖层 (PausePage · 9) — 回到游戏 (13)
    var pauseLayer=document.getElementById('pauseOverlay');
    if(pauseLayer && pauseLayer.style.display==='flex'){
      if(typeof resumeGame==='function') resumeGame();
      handled = true;
    }
  }

  if(!handled){
    // 3) 退出确认弹窗 (ConfirmPopUp · 14) — 取消操作
    var exitModal=document.getElementById('exitConfirm');
    if(exitModal && exitModal.classList.contains('show')){
      closeModal('exitConfirm');
      handled = true;
    }
  }

  // ── 页面导航（查表跳转） ──
  if(!handled){
    const SWIPE_TABLE = {
      'welcome':       () => showExitConfirm(),                     // 1 => 14
      'gameSetup':     () => Router.switchPage('welcome'),          // new => welcome
      'history':       () => Router.switchPage('welcome'),          // 5 => 1
      'about':         () => Router.switchPage('welcome'),          // 6 => 1
      'about-ai':      () => Router.switchPage('about'),            // 10 => 6
      'about-changelog': () => Router.switchPage('about'),          // 11 => 6
      'about-license': () => Router.switchPage('about'),            // 11 => 6
      'checkout':      () => {                                      // 7 => 5, 8 => 2/3/4
        const prev = Router._checkoutPrev;
        if(prev === 'history') Router.switchPage('history');        // EachHistoryPage => history
        else Router.switchPage(prev || 'welcome');                  // CheckoutPage => 对应大厅
      },
      'game':          () => {                                      // 13 => 9
        if(gameOver) exitGame(true);
        else if(isPaused) resumeGame();
        else togglePause();
      },
    };
    const action = SWIPE_TABLE[cur];
    if(action) action();
  }

  // ── 历史栈永不枯竭 ──
  // 每次触发侧滑，不论是否切换页面，都压入两条历史状态。
  // 消耗一条补充两条，确保栈始终有足够条目，安卓不会因无历史记录而退出应用。
  history.pushState(null, '');
  history.pushState(null, '');
})();