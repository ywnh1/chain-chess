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

/* ═══════ 背景光球随机初始化（大小/方向/速度/位置/颜色随机，更快更明显） ═══════ */
function randOrbGradient(){
  const h=Math.floor(Math.random()*360);
  const h2=(h+40+Math.floor(Math.random()*80))%360;
  const s=85+Math.floor(Math.random()*15);
  const l=55+Math.floor(Math.random()*15);
  return 'radial-gradient(circle, hsla('+h+','+s+'%,'+l+'%,.85) 0%, hsla('+h2+','+s+'%,'+l+'%,.38) 60%, transparent)';
}
function initOrbs(){
  const orbs=document.querySelectorAll('.orb');
  const light=window.matchMedia('(prefers-color-scheme: light)').matches;
  orbs.forEach(function(el){
    const sz=120+Math.random()*300;            // 大小 120~420px
    const dx=(Math.random()*2-1)*160;          // 方向 -160~160
    const dy=(Math.random()*2-1)*160;
    const dur=(4+Math.random()*8).toFixed(2);  // 速度 4~12s（更快）
    const delay=(-Math.random()*dur).toFixed(2); // 负延迟错开相位
    el.style.width=sz+'px';
    el.style.height=sz+'px';
    el.style.top=(Math.random()*120-10)+'%';
    el.style.left=(Math.random()*120-10)+'%';
    el.style.setProperty('--dx',dx+'px');
    el.style.setProperty('--dy',dy+'px');
    el.style.animationDuration=dur+'s';
    el.style.animationDelay=delay+'s';
    el.style.background=randOrbGradient();
    el.style.opacity=light?(0.45+Math.random()*0.15).toFixed(2):(0.75+Math.random()*0.2).toFixed(2);
  });
}
initOrbs();

/* ==================== CONSTANTS ==================== */
const COLORS=['#E74C3C','#F1C40F','#3498DB','#2ECC71','#9B59B6','#E91E63','#1ABC9C','#F39C12','#8B5E3C','#5D6D7E'];
const COLORS_LIGHT=['rgba(231,76,60,.08)','rgba(241,196,15,.08)','rgba(52,152,219,.08)','rgba(46,204,113,.08)','rgba(155,89,182,.08)','rgba(233,30,99,.08)','rgba(26,188,156,.08)','rgba(243,156,18,.08)','rgba(139,94,60,.08)','rgba(93,109,126,.08)'];
const COLOR_NAMES = ['红色','黄色','蓝色','绿色','紫色','粉色','青色','橙色','棕色','深灰色'];
// 棋盘边界模式描述
const BORDER_MODE_DESC = {
  'default': '标准模式：达到 4 子即爆，边界处正常扩散',
  'wrap': '回环模式：棋子达到 4 子即爆，爆炸会穿过边界到达对面，棋盘变成甜甜圈',
  'bounce': '反弹模式：达到 4 子即爆，边界处能量反弹集中：边上出一颗二级棋子和两颗一级棋子，角上出两颗二级棋子',
  'degrade': '降级模式：中央区域 4 子即爆，边界处降为 3 子即爆，角落处仅需 2 子即爆（与 3/5 级爆炸互斥）',
  'random': '随机边界：开局时随机确定一种边界模式，整局不再改变（默认 / 回环 / 反弹 / 降级）',
};
const CAP_MODE_DESC = {
  '3': '速爆：3 级即爆，爆炸时随机一个方向加 0（该格不变），其余方向加 1；首子为 2 级（与降级边界互斥）',
  '4': '标准规则：达到 4 子即爆，向上下左右各扩散一个棋子；首子为 3 级',
  '5': '重炮：5 级才爆，爆炸时随机一个方向加 2（空格变 2 级、有棋子升 2 级），其余方向加 1；首子为 4 级（与降级边界互斥）',
  'random': '随机阈值：开局时随机确定 3 / 4 / 5 级之一，整局不再改变；首子为该阈值减 1',
};
// 随机模式解析：开局瞬间用时间戳种子随机确定具体模式，整局不再改变。
// 解析后与直接选中该模式完全一致（随机只发生在开始的一瞬间）。
function resolveRandomBorder(){
  // 降级边界与 3/5 级爆炸互斥：阈值模式为 3/5（显式选择）时，随机边界排除降级
  const opts=(capMode==='3'||capMode==='5')
    ?['default','wrap','bounce']
    :['default','wrap','bounce','degrade'];
  let s=Date.now()>>>0; s=(s*1664525+1013904223)>>>0;
  return opts[s%opts.length];
}
function resolveRandomCap(){
  // 降级边界与 3/5 级爆炸互斥：边界为降级时，随机阈值只取 4 级（degrade 仅与 4 级兼容）
  const opts=borderMode==='degrade'?['4']:['3','4','5'];
  let s=Date.now()>>>0; s=(s*1664525+1013904223)>>>0;
  return opts[s%opts.length];
}
// 根据棋盘大小返回最大允许玩家人数
function getMaxPlayersBySize(boardSize){
  if(boardSize===5)return 5;
  if(boardSize===6)return 7;
  return 10;
}
// 双向联动：棋盘大小 ↔ 人数/AI数量，互相扣掉不合法的按钮

/* ═══════ 设置系统（主题 / 震动 / 音效主题） ═══════ */
let appSettings={theme:'system',vibrate:true,soundTheme:'classic',dogBarkMode:'long'};
let settingsLoaded=false;

// 主题切换：system 跟随系统；light/dark 手动覆盖
function applyTheme(theme){
  if(theme==='light'){document.body.classList.add('light-mode');document.documentElement.setAttribute('data-theme','light')}
  else if(theme==='dark'){document.body.classList.remove('light-mode');document.documentElement.setAttribute('data-theme','dark')}
  else{document.body.classList.remove('light-mode');document.documentElement.removeAttribute('data-theme')}
  // 状态栏主题色
  const meta=document.getElementById('metaThemeColor');
  if(meta){
    const lightNow=(theme==='light')||(theme==='system'&&window.matchMedia('(prefers-color-scheme: light)').matches);
    meta.content=lightNow?'#eae8e0':'#0f0f13';
  }
  // 同步光球透明度（浅色下更淡）
  try{
    const lightNow=(theme==='light')||(theme==='system'&&window.matchMedia('(prefers-color-scheme: light)').matches);
    document.querySelectorAll('.orb').forEach(function(el){
      el.style.opacity=lightNow?(0.45+Math.random()*0.15).toFixed(2):(0.75+Math.random()*0.2).toFixed(2);
    });
  }catch(e){}
  // 游戏进行中：setBg 设置的内联背景会覆盖主题背景，切主题时用当前玩家色重新刷新
  try{
    if(gameMode&&!gameOver&&typeof curPlayer==='number'&&curPlayer>=0&&curPlayer<COLORS.length&&typeof setBg==='function')setBg(curPlayer);
  }catch(e){}
}

async function loadSettings(){
  try{
    const s=await tauriInvoke('load_settings');
    if(s)appSettings={theme:'system',vibrate:true,soundTheme:'classic',dogBarkMode:'long',...s};
  }catch(e){}
  settingsLoaded=true;
  applyTheme(appSettings.theme);
  updateDogBarkRow();
}

async function saveSettings(){
  try{await tauriInvoke('save_settings',{settings:appSettings})}catch(e){}
}

// 大狗叫声模式行：仅在大狗叫主题时显示
function updateDogBarkRow(){
  const row=document.getElementById('dogBarkRow');
  if(row)row.style.display=(appSettings.soundTheme==='dog')?'flex':'none';
}

// 渲染设置页（每次进入时同步 UI 状态并绑定事件）
function renderSettingsPage(){
  // 震动开关
  const vt=document.getElementById('vibrateToggle');
  if(vt){
    vt.checked=!!appSettings.vibrate;
    vt.onchange=function(){
      appSettings.vibrate=this.checked;
      saveSettings();
      if(appSettings.vibrate)vibrate(15);
    };
  }
  // 音效主题
  const st=document.getElementById('soundThemeGroup');
  if(st){
    st.querySelectorAll('.tg-btn').forEach(function(b){
      b.classList.toggle('selected',b.dataset.value===appSettings.soundTheme);
    });
    st.querySelectorAll('.tg-btn').forEach(function(b){
      b.onclick=function(){
        st.querySelectorAll('.tg-btn').forEach(function(x){x.classList.remove('selected')});
        this.classList.add('selected');
        appSettings.soundTheme=this.dataset.value;
        saveSettings();
        updateDogBarkRow();
      };
    });
  }
  // 大狗叫声模式（仅大狗叫主题显示）
  const dbRow=document.getElementById('dogBarkRow');
  const dbg=document.getElementById('dogBarkGroup');
  if(dbRow&&dbg){
    dbg.querySelectorAll('.tg-btn').forEach(function(b){
      b.classList.toggle('selected',b.dataset.value===appSettings.dogBarkMode);
    });
    dbg.querySelectorAll('.tg-btn').forEach(function(b){
      b.onclick=function(){
        dbg.querySelectorAll('.tg-btn').forEach(function(x){x.classList.remove('selected')});
        this.classList.add('selected');
        appSettings.dogBarkMode=this.dataset.value;
        saveSettings();
      };
    });
  }
  updateDogBarkRow();
  // 试听按钮
  const pl=document.getElementById('soundPreviewBtn');
  if(pl){
    pl.onclick=function(){
      playClick();setTimeout(()=>playExplosion(),300);
      setTimeout(()=>playElim(),700);setTimeout(()=>playGameOver(),1100);
    };
  }
}

loadSettings();

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
// ─── 预设音效主题（Web Audio 合成，无需外部文件） ───
// 每套主题定义 4 个音效：click/explosion/elim/gameover
// 值为单个音或音序列（数组，每项 {f,dur,type,vol}，按 120ms 间隔播放）
const SOUND_THEMES={
  classic:{
    label:'经典',
    click:{f:800,dur:0.08,type:'sine',vol:0.12},
    explosion:{f:150,dur:0.25,type:'sawtooth',vol:0.15},
    elim:[{f:400,dur:0.15,type:'square',vol:0.08},{f:300,dur:0.15,type:'square',vol:0.08},{f:200,dur:0.2,type:'square',vol:0.08}],
    gameover:[{f:523,dur:0.15,type:'sine',vol:0.12},{f:659,dur:0.15,type:'sine',vol:0.12},{f:784,dur:0.3,type:'sine',vol:0.15}],
  },
  soft:{
    label:'柔和',
    click:{f:600,dur:0.12,type:'triangle',vol:0.10},
    explosion:{f:110,dur:0.35,type:'sine',vol:0.14},
    elim:[{f:330,dur:0.2,type:'sine',vol:0.08},{f:262,dur:0.2,type:'sine',vol:0.08},{f:196,dur:0.25,type:'sine',vol:0.08}],
    gameover:[{f:392,dur:0.25,type:'sine',vol:0.10},{f:494,dur:0.25,type:'sine',vol:0.10},{f:587,dur:0.4,type:'sine',vol:0.12}],
  },
  crisp:{
    label:'清脆',
    click:{f:1200,dur:0.05,type:'triangle',vol:0.12},
    explosion:{f:220,dur:0.15,type:'square',vol:0.12},
    elim:[{f:600,dur:0.08,type:'triangle',vol:0.10},{f:450,dur:0.08,type:'triangle',vol:0.10},{f:300,dur:0.12,type:'triangle',vol:0.10}],
    gameover:[{f:784,dur:0.10,type:'triangle',vol:0.12},{f:988,dur:0.10,type:'triangle',vol:0.12},{f:1319,dur:0.25,type:'triangle',vol:0.15}],
  },
  electronic:{
    label:'电子',
    click:{f:700,dur:0.06,type:'square',vol:0.10},
    explosion:{f:100,dur:0.3,type:'sawtooth',vol:0.18},
    elim:[{f:500,dur:0.10,type:'sawtooth',vol:0.10},{f:350,dur:0.10,type:'sawtooth',vol:0.10},{f:200,dur:0.15,type:'sawtooth',vol:0.12}],
    gameover:[{f:440,dur:0.12,type:'square',vol:0.10},{f:554,dur:0.12,type:'square',vol:0.10},{f:659,dur:0.2,type:'square',vol:0.12}],
  },
  dog:{
    label:'大狗叫',
    // 使用 public/audio/ 下的真实音频，其余音效静音
    click:'audio/大狗.mp3',
    explosion:'dog-bark', // 特殊标记：按 dogBarkMode 在 3 种叫声间选择
    elim:null,
    gameover:null,
  },
  mute:{
    label:'静音',
    // 取消全部音效
    click:null,
    explosion:null,
    elim:null,
    gameover:null,
  },
};

// 大狗叫声模式 → 音频文件（中淡出 / 无淡出 / 长淡出）
const DOG_BARK_FILES={
  medium:'audio/叫(中淡出).mp3',
  none:'audio/叫(无淡出).mp3',
  long:'audio/叫(长淡出).mp3',
};

// 播放外部音频文件（相对 public 根目录）
function playSoundFile(src){
  try{
    const a=new Audio(encodeURI(src));
    a.volume=0.9;
    a.play().catch(()=>{});
  }catch(e){}
}

// 按当前主题播放指定音效
function playThemeSound(key){
  const theme=SOUND_THEMES[appSettings.soundTheme]||SOUND_THEMES.classic;
  const s=theme[key];
  if(!s)return;
  if(s==='dog-bark'){
    playSoundFile(DOG_BARK_FILES[appSettings.dogBarkMode]||DOG_BARK_FILES.long);
    return;
  }
  if(typeof s==='string'){playSoundFile(s);return;}
  if(Array.isArray(s)){
    s.forEach((x,i)=>setTimeout(()=>playTone(x.f,x.dur,x.type,x.vol),i*120));
  }else{
    playTone(s.f,s.dur,s.type,s.vol);
  }
}
function playClick(){playThemeSound('click');vibrate(12)}
function playExplosion(){playThemeSound('explosion');vibrate(25)}
function playElim(){
  playThemeSound('elim');
  vibrate([40,30,50]);
}
function playGameOver(){
  playThemeSound('gameover');
  vibrate([80,40,80,40,100]);
}
// ─── 震动反馈 ───
// Android WebView 的 navigator.vibrate 不可用（WebView 禁用 Vibration API），
// 因此优先走 Tauri haptics 插件（原生 Vibrator，需 VIBRATE 权限），
// 命令不可用（桌面等）时回退 navigator.vibrate，均失败则静默降级。
async function vibrate(pattern){
  if(appSettings.vibrate===false)return;
  const list=Array.isArray(pattern)?pattern:[pattern];
  const invoke=window.__TAURI_INTERNALS__&&window.__TAURI_INTERNALS__.invoke;
  if(invoke){
    try{
      // pattern 数组语义：[振动, 间隔, 振动, 间隔...]
      for(let i=0;i<list.length;i++){
        const d=list[i];
        if(i%2===0&&d>0)await invoke('plugin:haptics|vibrate',{duration:d});
        if(i<list.length-1&&list[i+1]>0)await new Promise(r=>setTimeout(r,list[i+1]));
      }
      return;
    }catch(e){/* 插件命令不可用（桌面端未注册等）→ 回退 */ }
  }
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
let animating=false; // 连锁动画播放中，期间锁定落子/悔棋
let aiAlgorithm='strategy';
let aiDepth=2;
let aiConfigs={}; // per-AI configs for eve mode
let gameCount=0;
let eliminatedPlayers=new Set();
var eliminationRounds=[];
var eliminationInfo={}; // pid -> 击败者 pid（被谁淘汰）
let gameHistory=[];
let chainStats={};
let maxChainOverall={player:null,length:0};

// 连炸跳过标志
let chainSkipAll=false;
// 自动跳过连爆动画（持久开关，true=跳过所有连爆动画，false=行为不变）
let autoSkipChain=false;
 
// 棋盘边界模式
let borderMode='default';
// 爆炸阈值模式（独立设置）：3=3级炸 / 4=默认4级 / 5=5级炸 / random=每步随机3/4/5
let capMode='4';

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
// 降级边界与 3/5 级爆炸互斥：设置界面联动禁用 + 冲突纠正
// 规则：选择降级边界 → 3/5 级不可选；选择 3/5 级 → 降级边界不可选；后点者生效，另一方弹回兼容值
function updateModeConflictUI(caller){
  const borderGroup=document.getElementById('borderModeGroup');
  const capGroup=document.getElementById('capModeGroup');
  if(!borderGroup||!capGroup)return;
  let selB=borderGroup.querySelector('.selected');
  let selC=capGroup.querySelector('.selected');
  let bv=selB?selB.dataset.value:null;
  let cv=selC?selC.dataset.value:null;
  // 纠正已选冲突（degrade + 3/5 并存）
  if(bv==='degrade'&&(cv==='3'||cv==='5')){
    if(caller==='cap'){
      // 用户刚选 3/5：降级边界不可用，弹回默认边界
      const bd=borderGroup.querySelector('.gb[data-value="default"]');
      if(bd)setSelected(borderGroup,bd);
      const d=document.getElementById('borderModeDesc');
      if(d&&BORDER_MODE_DESC['default'])d.textContent=BORDER_MODE_DESC['default'];
    }else{
      // 用户刚选降级边界（或初始化）：阈值弹回 4
      const c4=capGroup.querySelector('.gb[data-value="4"]');
      if(c4)setSelected(capGroup,c4);
      const d=document.getElementById('capModeDesc');
      if(d&&CAP_MODE_DESC['4'])d.textContent=CAP_MODE_DESC['4'];
    }
    // 纠正后重读选中值
    selB=borderGroup.querySelector('.selected'); selC=capGroup.querySelector('.selected');
    bv=selB?selB.dataset.value:null; cv=selC?selC.dataset.value:null;
  }
  // 禁用联动：cap 为 3/5 → degrade 不可选；border 为 degrade → 3/5 不可选
  const cap35=cv==='3'||cv==='5';
  borderGroup.querySelectorAll('.gb').forEach(function(b){
    b.disabled=(b.dataset.value==='degrade'&&cap35);
  });
  capGroup.querySelectorAll('.gb').forEach(function(b){
    b.disabled=((b.dataset.value==='3'||b.dataset.value==='5')&&bv==='degrade');
  });
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
  }else if(container.id==='borderModeGroup'){
    const descEl=document.getElementById('borderModeDesc');
    if(descEl&&BORDER_MODE_DESC[t.dataset.value]){descEl.textContent=BORDER_MODE_DESC[t.dataset.value];}
    updateModeConflictUI('border');
  }else if(container.id==='capModeGroup'){
    const descEl=document.getElementById('capModeDesc');
    if(descEl&&CAP_MODE_DESC[t.dataset.value]){descEl.textContent=CAP_MODE_DESC[t.dataset.value];}
    updateModeConflictUI('cap');
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
  enter() {
    document.body.style.background='';
  },
  leave() {}
});

Router.register('gameSetup', {
  back: 'welcome',
  enter() { document.body.style.background=''; setTimeout(setupLobbySync, 20); setTimeout(updateModeConflictUI, 20); },
  leave() {}
});

Router.register('history', {
  back: 'welcome',
  enter() { document.body.style.background=''; loadHistoryList(); },
  leave() { if(_multiSelectActive)exitMultiSelect(); }
});




Router.register('settings', {
  back: 'welcome',
  enter() { document.body.style.background=''; renderSettingsPage(); },
  leave() {}
});

Router.register('game', {
  back: null,
  enter() {},
  leave() { document.body.style.background='' }
});

Router.register('about', {
  back: 'settings',
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
Router.register('about-benchmark', {
  back: 'about',
  enter() { document.body.style.background='' },
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
  if(window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke)
    return window.__TAURI_INTERNALS__.invoke(cmd, args || {});
  // PWA 降级：非 Tauri 环境转调 ChainEngine（WASM 引擎 + localStorage 存储）
  if(window.ChainEngine && window.ChainEngine.webInvoke)
    return window.ChainEngine.webInvoke(cmd, args || {});
  return Promise.reject(new Error('Not in Tauri'));
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
        <div style="margin-top:6px"><button class="glass-btn primary" id="replayFromListBtn" style="padding:5px 12px;font-size:.75rem">▶ 回放</button></div>
      `;
      // 回放按钮（不触发条目详情跳转）
      const rbtn=div.querySelector('#replayFromListBtn');
      if(rbtn){
        rbtn.id='';
        rbtn.onclick=function(e){
          e.stopPropagation();
          const rid=parseInt(div.dataset.recordId);
          const rec=_historyRecords[rid];
          if(!rec)return;
          const historyData=expandHistory(rec.history, rec.playerCount||maxPlayers);
          openReplay({
            size: rec.boardSize || 7,
            maxPlayers: rec.playerCount || 2,
            borderMode: rec.borderMode || 'default',
            capMode: rec.capMode || '4',
            gameCount: rec.gameCount || 0,
            colorNames: rec.colorNames,
            history: historyData,
          });
        };
      }
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
  // 每步落子 (x,y,player)，无落子的初始状态为 null
  const mvList=new Array(n).fill(null);
  for(let t=0;t<n;t++){
    const snap=history[t].snapshot||{};
    for(let p=0;p<playerCount;p++){
      const d=snap[String(p)];
      if(d){pieces[p][t]=d.pieces||0;points[p][t]=d.points||0}
    }
    const mv=history[t].mv;
    if(mv&&(mv.x!==undefined||Array.isArray(mv))){
      // 兼容对象 {x,y,player,seed} 与数组 [x,y,player,seed]（tauri round_history 存数组）
      mvList[t]=Array.isArray(mv)?[mv[0],mv[1],mv[2],mv[3]||0]:[mv.x,mv.y,mv.player,mv.seed!==undefined?mv.seed:0];
    }
  }
  return {c:true,t:n,p:pieces,pt:points,m:mvList};
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
    const m=compact.m&&compact.m[t];
    const mv=(m&&m.length>=3)?{x:m[0],y:m[1],player:m[2],seed:(m.length>=4?m[3]:0)}:null;
    history.push({turn:t, snapshot, mv});
  }
  return history;
}
// Save game history for Tauri（紧凑存储）
// historyArg 可选：若传入则用其代替 gameHistory（用于 gameHistory 已被清空的场景）
// 将 _colorNames（可能为对象）转为数组，确保 Rust Vec<String> 正确反序列化
function toColorNamesArray(src){
  if(Array.isArray(src))return src;
  if(src&&typeof src==='object'){
    var arr=[];
    for(var i=0;i<maxPlayers;i++){
      arr.push(src[i]||COLOR_NAMES[i]||'玩家 '+(i+1));
    }
    return arr;
  }
  return COLOR_NAMES;
}
function getPlayerTypesArray(){
  var arr=[];
  for(var i=0;i<maxPlayers;i++){
    if(aiPlayers.has(i)){
      var cfg=aiConfigs[i]||{};
      arr.push(cfg.algorithm||'strategy');
    }else{
      arr.push('human');
    }
  }
  return arr;
}
async function saveGameHistory(winner, mode, aiAlg, aiDp, historyArg){
  const src = historyArg || gameHistory;
  const compact = compactHistory(src, maxPlayers);
  try {
    await tauriInvoke('save_game_history', {
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
        borderMode: borderMode || 'default',
        capMode: capMode || '4',
        winner: winner !== null && winner !== undefined ? winner : null,
        colorNames: toColorNamesArray(window._colorNames || COLOR_NAMES),
        chainStats: chainStats,
        maxChain: maxChainOverall,
        finished: true,
        history: compact,
        playerTypes: getPlayerTypesArray(),
        killedBy: {...eliminationInfo},
      }
    });
    // 保存成功后清理磁盘上的回合数据（不再需要）
    tauriInvoke('clear_round_history').catch(e=>logWarn('Clear round history after save failed:', e));
  } catch(e) {
    logWarn('Save history failed:', e);
  }
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
  const _eliminationInfo = {...eliminationInfo};
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
    eliminationInfo: _eliminationInfo,
    chainStats: _chainStats,
    maxChainOverall: _maxChainOverall,
    gameCount: _gameCount,
    capMode: capMode || '4',
    aiAlgorithm: _aiAlgorithm,
    aiDepth: _aiDepth,
    colorNames: toColorNamesArray(_colorNames),
    aiCount: _aiPlayers.length,
    undoStack: [],
    savedHistory: fullHistory.length > 0 ? compactHistory(fullHistory, _maxPlayers) : null,
  };
  const stateJson = JSON.stringify(state);
  const compact = fullHistory.length > 0 ? compactHistory(fullHistory, _maxPlayers) : {};
  // 生成未完成游戏的 playerTypes
  var _playerTypes=[];
  for(var _pi=0;_pi<_maxPlayers;_pi++){
    if(_aiPlayers.indexOf(_pi)>=0){
      var _pcfg=_aiConfigs[_pi]||{};
      _playerTypes.push(_pcfg.algorithm||'strategy');
    }else{
      _playerTypes.push('human');
    }
  }
  try {
    await tauriInvoke("save_game_history", {
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
        colorNames: toColorNamesArray(_colorNames),
        chainStats: _chainStats,
        maxChain: _maxChainOverall,
        finished: false,
        history: compact,
        gameState: stateJson,
        playerTypes: _playerTypes,
      }
    });
  } catch(e) {
    logWarn("Save unfinished history failed:", e);
  }
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
  borderMode = saved.borderMode || "default";
  capMode = saved.capMode || "4";
  // 降级边界与 3/5 级爆炸互斥：旧存档冲突时降级优先，阈值归 4
  if(borderMode==='degrade'&&(capMode==='3'||capMode==='5'))capMode='4';

  document.getElementById("pauseBtn").textContent = "暂停";
  _originPage = gameMode === "ai" ? "aiLobby" : (gameMode === "eve" ? "eveLobby" : "localLobby");

  board = saved.board;

  aiPlayers = new Set();
  if(saved.aiPlayers && saved.aiPlayers.length > 0){
    saved.aiPlayers.forEach(p => aiPlayers.add(p));
  }

  aiConfigs = saved.aiConfigs || {};

  eliminatedPlayers = new Set();
  eliminationRounds = [];
  eliminationInfo = {};
  if(saved.eliminatedPlayers && saved.eliminatedPlayers.length > 0){
    saved.eliminatedPlayers.forEach(p => eliminatedPlayers.add(p));
    // 重建淘汰轮次
    saved.eliminatedPlayers.forEach(p => eliminationRounds.push([p]));
  }
  if(saved.eliminationInfo)eliminationInfo = saved.eliminationInfo;

  _colorNames = saved.colorNames || COLOR_NAMES;
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
    eliminationInfo: {...eliminationInfo},
    chainStats: JSON.parse(JSON.stringify(chainStats || {})),
    maxChainOverall: maxChainOverall ? {...maxChainOverall} : {player:null,length:0},
    gameCount: gameCount || 0,
    borderMode: borderMode || 'default',
    capMode: capMode || '4',
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
// 创建空棋盘（每格 owner=null、count=0；保留 th 字段以兼容旧历史数据，但不再生成阈值）
function mkBoard(s){
  return Array.from({length:s},()=>Array.from({length:s},()=>{
    return {owner:null,count:0,th:undefined};
  }));
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

function recordHistory(mv){
  let sn={};
  for(let p=0;p<maxPlayers;p++){
    let pieces=0,points=0;
    for(let r of board)for(let c of r)if(c.owner===p){pieces++;points+=c.count}
    sn[p]={pieces,points};
  }
  gameHistory.push({turn:gameHistory.length,snapshot:sn,mv:mv||null});
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


// 边界模式感知的爆炸阈值
function capForMode(i,j,sz,bm,cell){
  void cell;
  if(capMode==='random')return 3+Math.floor(Math.random()*3);  // 随机模式：每步随机 3/4/5
  if(capMode==='3')return 3;
  if(capMode==='5')return 5;
  // 默认(4)：保留降级边界的位置修正
  if(bm==='degrade'){
    let onCorner=(i===0||i===sz-1)&&(j===0||j===sz-1);
    let onEdge=i===0||i===sz-1||j===0||j===sz-1;
    if(onCorner)return 2;
    if(onEdge)return 3;
  }
  return 4;
}

// 边界模式感知的邻居函数
function nbrsForMode(i,j,sz,bm){
  if(bm==='random'){
    // 随机边界：每次爆炸随机选择一种边界行为（default/wrap/bounce/degrade；degrade 邻居同 default）
    const r=Math.floor(Math.random()*4);
    if(r===0)return nbrsForMode(i,j,sz,'default');
    if(r===1)return nbrsForMode(i,j,sz,'wrap');
    if(r===2)return nbrsForMode(i,j,sz,'bounce');
    return nbrsForMode(i,j,sz,'default');   // degrade：邻居同 default
  }
  if(bm==='wrap'){
    let up=i===0?sz-1:i-1;
    let down=i+1>=sz?0:i+1;
    let left=j===0?sz-1:j-1;
    let right=j+1>=sz?0:j+1;
    return[[up,j],[down,j],[i,left],[i,right]];
  }
  if(bm==='bounce'){
    // 反弹边界：出界方向的能量反弹到正对方向（该方向额外 +1，重复出现表示能量叠加）
    let dirs=[[i-1,j],[i+1,j],[i,j-1],[i,j+1]];
    let opp=[1,0,3,2];
    let out=[];
    for(let k=0;k<4;k++){
      let[nx,ny]=dirs[k];
      if(nx>=0&&nx<sz&&ny>=0&&ny<sz)out.push([nx,ny]);
    }
    for(let k=0;k<4;k++){
      let[nx,ny]=dirs[k];
      if(nx<0||nx>=sz||ny<0||ny>=sz){
        let[ox,oy]=dirs[opp[k]];
        if(ox>=0&&ox<sz&&oy>=0&&oy<sz)out.push([ox,oy]);
      }
    }
    return out;
  }
  let r=[];
  if(i>0)r.push([i-1,j]);
  if(i+1<sz)r.push([i+1,j]);
  if(j>0)r.push([i,j-1]);
  if(j+1<sz)r.push([i,j+1]);
  return r;
}

async function processClick(b,s,x,y,pl,anim,playerColor,rustResult){
  animating=true;
  try{
  // 是否需要跳过连炸动画：autoSkipChain 为持久开关，chainSkipAll 为当前动画内点击「跳过动画」临时置位
  const wantSkip=()=>chainSkipAll||autoSkipChain;
  if(anim&&!wantSkip())showSkipBtn(true);
  let bm=borderMode||'default';   // 必须在使用前定义（首子分支会引用 bm，避免 TDZ 异常）
  let c=b[x][y];
  let isFirstMove=false;
  if(c.owner===null){
    let anyPieces=false;
    for(let r of b)for(let cl of r)if(cl.owner!==null)anyPieces=true;
    if(!anyPieces) firstMovePos=[x,y];
    isFirstMove = !hasPieces(pl,b);
    c.owner=pl;
    c.count=isFirstMove?capForMode(x,y,s,bm,c)-1:1;  // 首子等级 = 阈值 n-1（临界态）
  }
  else if(c.owner===pl)c.count++
  else return[];

  // 如果有 Rust 结果，先保存最终棋盘用于最后矫正
  let finalBoard = (rustResult && rustResult.board) ? rustResult.board : null;

  let had=new Set();
  for(let row of b)for(let cl of row)if(cl.owner!==null)had.add(cl.owner);
  let chain=isFirstMove?[]:[[x,y]];   // 首子放置不进连锁（临界态，本步仅放置）
  let chainCount=0;
  let animDelay=(anim==='explode')?220:0;

  // ── 引擎快照动画路径 ──
  // Rust 引擎返回连锁逐步快照（steps）与爆炸格坐标（exploded）时，动画按引擎真实结果渲染，
  // 杜绝 JS 模拟与引擎随机不一致导致的 cap3/cap5 特殊格方向跳变；
  // 无动画模式（AI 快速对战等 anim=null）同样以引擎快照为准，避免 JS 模拟中间态
  // 与引擎最终结果不一致造成"炸开一瞬间后回退"
  const snapSteps = (rustResult && Array.isArray(rustResult.steps) && rustResult.steps.length>0) ? rustResult.steps : null;
  const snapExploded = (rustResult && Array.isArray(rustResult.exploded)) ? rustResult.exploded : null;
  if(snapSteps){
    chainCount = (rustResult && rustResult.chainCount) || snapSteps.length;
    for(let k=0;k<snapSteps.length;k++){
      // 应用引擎快照棋盘（每一步爆炸后的真实状态）
      const snap = snapSteps[k];
      for(let i=0;i<size;i++)for(let j=0;j<size;j++)b[i][j]=snap[i][j];
      // 爆炸特效位置以引擎返回的坐标为准
      let ex=x,ey=y;
      if(snapExploded && snapExploded[k]){
        ex=snapExploded[k][0]; ey=snapExploded[k][1];
      }
      const skipNow=wantSkip();
      if(!anim){
        // 无动画模式：只推进数据，不逐帧渲染（末尾统一渲染引擎最终棋盘）
      }else if(anim&&skipNow){
        // 跳过模式：不播特效、不逐帧渲染、不等待，连锁瞬间完成（末尾统一渲染一次）
      }else if(anim==='explode'){
        playExplosion();
        let el=cells?.[ex]?.[ey];
        renderBoard(false);
        if(el){addShockwave(el,playerColor);addParticles(el,playerColor,8)}
        if(el)el.classList.add('explode');
        await sleep(animDelay);
        if(el)el.classList.remove('explode');
      }else if(anim){
        playExplosion();
        let el=cells?.[ex]?.[ey];if(el)el.classList.add('explode');
        renderBoard(false);
        await sleep(150);
        if(el)el.classList.remove('explode');
      }else{
        renderBoard(false);
      }
    }
    // 矫正到引擎最终棋盘（快照最后一步即最终态，再矫正一次保证一致）
    if(finalBoard){
      for(let i=0;i<size;i++) for(let j=0;j<size;j++) b[i][j]=finalBoard[i][j];
      renderBoard(true);
    }else if(anim&&chainCount>0&&wantSkip()){
      renderBoard(true);
    }else if(!anim&&chainCount>0){
      // 无动画模式兜底：确保最终棋盘已渲染（引擎结果）
      renderBoard(true);
    }
    // 记录连爆统计
    if(chainCount>0){
      if(!chainStats[pl]) chainStats[pl]={triggered:0,maxChain:0};
      chainStats[pl].triggered++;
      if(chainCount>chainStats[pl].maxChain) chainStats[pl].maxChain=chainCount;
      if(chainCount>maxChainOverall.length) maxChainOverall={player:pl,length:chainCount};
    }
    if(finalBoard&&rustResult&&rustResult.eliminated)return rustResult.eliminated;
    let nowSnap=new Set();
    for(let row of b)for(let cl of row)if(cl.owner!==null)nowSnap.add(cl.owner);
    return[...had].filter(p=>!nowSnap.has(p));
  }

  let chainGuard=0; // 连锁防御上限：防止特定棋盘结构下无限互炸导致动画/点击卡死
  while(chain.length){
    if(++chainGuard>100000)break;
    let[cx,cy]=chain.shift(),cell=b[cx][cy];
    // 随机边界模式：每次爆炸随机一种边界行为（与引擎 gen_range(0..4) 对齐：default/wrap/bounce/degrade）
    let effBm=bm;
    if(bm==='random'){
      const rr=Math.floor(Math.random()*4);
      effBm=rr===0?'default':rr===1?'wrap':rr===2?'bounce':'degrade';
    }
    let capv=capForMode(cx,cy,s,effBm,cell);
    if(cell.count>=capv){
      cell.count=0;cell.owner=null;
      chainCount++;
      const skipNow=wantSkip();
      // 爆炸扩散到邻居（按边界模式）
      let targets=[...nbrsForMode(cx,cy,s,effBm)];
      // 速爆(cap3)：随机一个方向"加 0"（该格完全不变）；重炮(cap5)：随机一个方向"加 2"
      let special=-1;
      if((capMode==='3'||capMode==='5')&&targets.length>0){
        special=Math.floor(Math.random()*targets.length);
      }
      for(let ti=0;ti<targets.length;ti++){
        let[nx,ny]=targets[ti];
        let nc=b[nx][ny];
        if(ti===special){
          if(capMode==='3'){
            // 加 0：该格完全不变（空格保持空、有棋子保持原样），不入连锁
            continue;
          }else{
            // 加 2：空格变 2 级，有棋子原等级 +2
            nc.owner=pl;nc.count=(nc.count||0)+2;
            chain.push([nx,ny]);
            continue;
          }
        }
        nc.owner=pl;nc.count++;
        chain.push([nx,ny]);
      }
      if(anim&&skipNow){
        // 跳过模式：不播特效、不逐帧渲染、不等待，连锁瞬间完成（末尾统一渲染一次）
      }else if(anim==='explode'){
        playExplosion();
        let el=cells?.[cx]?.[cy];
        renderBoard(false);
        if(el){addShockwave(el,playerColor);addParticles(el,playerColor,8)}
        if(el)el.classList.add('explode');
        await sleep(animDelay);
        if(el)el.classList.remove('explode');
      }else if(anim){
        playExplosion();
        let el=cells?.[cx]?.[cy];if(el)el.classList.add('explode');
        renderBoard(false);
        await sleep(150);
        if(el)el.classList.remove('explode');
      }else{
        // 无动画模式：不逐帧渲染 JS 模拟中间态（cap3/cap5 随机特殊格下与引擎
        // 最终结果可能不一致，避免"炸开一瞬间后回退"的视觉闪烁）
      }
    }
  }

  // 用 Rust 结果矫正本地棋盘（弥补前端简化模拟 vs 真实边界逻辑的偏差）
  if(finalBoard){
    for(let i=0;i<size;i++) for(let j=0;j<size;j++){
      b[i][j]=finalBoard[i][j];
    }
    // 矫正后始终渲染引擎真实结果：动画播放路径下此前依赖逐帧渲染 JS 模拟态，
    // 与引擎结果（尤其 cap3/cap5 随机特殊格）可能不一致，导致动画结束后棋盘停留错误状态
    renderBoard(true);
  }else if(anim&&chainCount>0&&wantSkip()){
    // 跳过模式未逐帧渲染，补一次最终棋盘渲染
    renderBoard(true);
  }else if(!anim&&chainCount>0){
    // 无动画模式兜底：确保 JS 模拟结果（引擎调用失败时）已渲染
    renderBoard(true);
  }

  // 记录连爆统计
  if(chainCount>0){
    if(!chainStats[pl]) chainStats[pl]={triggered:0,maxChain:0};
    chainStats[pl].triggered++;
    if(chainCount>chainStats[pl].maxChain) chainStats[pl].maxChain=chainCount;
    if(chainCount>maxChainOverall.length) maxChainOverall={player:pl,length:chainCount};
  }
  let now=new Set();
  for(let row of b)for(let cl of row)if(cl.owner!==null)now.add(cl.owner);
  if(finalBoard&&rustResult&&rustResult.eliminated)return rustResult.eliminated;
  return[...had].filter(p=>!now.has(p));
  }finally{
    animating=false;
    if(anim)showSkipBtn(false);
    chainSkipAll=false; // 手动跳过只作用于当前动画，播完即复位，避免影响后续动画
  }
}

function showSkipBtn(v){
  const el=document.getElementById('skipChainBtn');
  if(el)el.style.display=v?'block':'none';
}

function toggleAutoSkip(){
  autoSkipChain=!autoSkipChain;
  const btn=document.getElementById('autoSkipBtn');
  if(btn)btn.classList.toggle('on',autoSkipChain);
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
// 特效挂到独立 fixed 层，避免棋盘格子重绘（innerHTML='' / DOM 重建）清掉动画
function getFxLayer(){
  let l=document.getElementById('fxLayer');
  if(!l){
    l=document.createElement('div');l.id='fxLayer';
    l.style.cssText='position:fixed;inset:0;pointer-events:none;z-index:60;overflow:hidden';
    document.body.appendChild(l);
  }
  return l;
}
function addRipple(el){
  if(!el)return;
  const rect=el.getBoundingClientRect();
  const rip=document.createElement('div');rip.className='ripple';
  rip.style.cssText=`position:absolute;left:${rect.left}px;top:${rect.top}px;width:${rect.width}px;height:${rect.height}px`;
  getFxLayer().appendChild(rip);
  setTimeout(()=>rip.remove(),600);
}
function addShockwave(el,color){
  if(!el)return;
  const rect=el.getBoundingClientRect();
  const sw=document.createElement('div');sw.className='shockwave';
  sw.style.borderColor=color||'rgba(255,255,255,.5)';
  sw.style.cssText=`position:absolute;left:${rect.left-8}px;top:${rect.top-8}px;width:${rect.width+16}px;height:${rect.height+16}px;border:2px solid ${color||'rgba(255,255,255,.5)'}`;
  getFxLayer().appendChild(sw);
  setTimeout(()=>sw.remove(),600);
}
function addParticles(el,color,count){
  if(!el)return;
  const rect=el.getBoundingClientRect();
  const fx=getFxLayer();
  const cx=rect.left+rect.width/2,cy=rect.top+rect.height/2;
  for(let i=0;i<(count||6);i++){
    const p=document.createElement('div');p.className='particle';
    const angle=Math.random()*Math.PI*2;
    const dist=30+Math.random()*50;
    p.style.cssText=`position:absolute;left:${cx}px;top:${cy}px;background:${color||'rgba(255,255,255,.7)'};--dx:${Math.cos(angle)*dist}px;--dy:${Math.sin(angle)*dist}px`;
    fx.appendChild(p);
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
// 当前是否为亮色主题（手动亮色，或跟随系统且系统为亮色）
function isLightTheme(){
  const t=(appSettings&&appSettings.theme)||'system';
  return t==='light'||(t==='system'&&window.matchMedia('(prefers-color-scheme: light)').matches);
}
function setBg(pl){
  let h=COLORS[pl];
  if(!h||typeof h!=='string'||h.length!==7)return;  // 防御无效玩家索引：不设置背景，避免残留旧色
  let r=parseInt(h.substr(1,2),16),g=parseInt(h.substr(3,2),16),b=parseInt(h.substr(5,2),16);
  if(isLightTheme()){
    // 亮色主题：向白色混合出浅色氛围背景（避免内联深色背景锁死主题切换）
    const m=0.72;
    r=Math.round(r+(255-r)*m);g=Math.round(g+(255-g)*m);b=Math.round(b+(255-b)*m);
    document.body.style.background=`radial-gradient(ellipse 80% 50% at 50% -20%, rgba(240,179,75,.10) 0%, transparent 70%),radial-gradient(ellipse 60% 40% at 80% 100%, rgba(95,195,195,.08) 0%, transparent 70%),rgb(${r},${g},${b})`;
  }else{
    r=Math.round(r*.85);g=Math.round(g*.85);b=Math.round(b*.85);
    document.body.style.background=`radial-gradient(ellipse 80% 50% at 50% -20%, rgba(240,179,75,.04) 0%, transparent 70%),radial-gradient(ellipse 60% 40% at 80% 100%, rgba(95,195,195,.03) 0%, transparent 70%),rgb(${r},${g},${b})`;
  }
}
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
    t.style.cursor='pointer';
    t.onclick=function(e){e.stopPropagation();openInGamePlayerConfig(p)};
    el.appendChild(t);
  }
}
function cloneBoard(b){
  return b.map(row=>row.map(c=>({owner:c.owner,count:c.count})));
}

/* ═══════ AI 走法建议（局内手动触发，仅给当前一步） ═══════ */
let fastFinishing=false;    // 一键终局计算中（暂停后台游戏）
function clearAiHint(){
  try{document.querySelectorAll('.ai-hint').forEach(function(el){el.classList.remove('ai-hint')})}catch(e){}
}
// 打开建议弹窗：预填当前玩家 AI 配置（无则用全局默认）
function openAiHelpModal(){
  clearAiHint();
  if(gameOver||isPaused||aiThinking){showMsg('当前状态无法获取建议','error');return;}
  if(!hasPieces(curPlayer,board)){showMsg('当前玩家没有棋子','error');return;}
  const cfg=aiConfigs[curPlayer];
  const alg=(cfg&&cfg.algorithm)||aiAlgorithm||'alphabeta';
  const dep=(cfg&&cfg.depth)||aiDepth||3;
  const ml=(cfg&&cfg.useMlEval)!==false;
  // 算法按钮
  const btns=document.getElementById('aiHelpAlgBtns');
  if(btns){
    btns.querySelectorAll('.am-btn').forEach(function(b){
      const on=b.dataset.value===alg;
      b.classList.toggle('selected',on);
      b.onclick=function(){
        btns.querySelectorAll('.am-btn').forEach(function(x){x.classList.remove('selected')});
        this.classList.add('selected');
        updateAiHelpRows();
      };
    });
  }
  document.getElementById('aiHelpDepthValue').textContent=String(Math.max(1,Math.min(10,dep)));
  document.getElementById('aiHelpDepthDec').onclick=function(){var e=document.getElementById('aiHelpDepthValue');var v=parseInt(e.textContent)||3;if(v>1){v--;e.textContent=v}};
  document.getElementById('aiHelpDepthInc').onclick=function(){var e=document.getElementById('aiHelpDepthValue');var v=parseInt(e.textContent)||3;if(v<10){v++;e.textContent=v}};
  const evalBtns=document.getElementById('aiHelpEvalToggle');
  if(evalBtns){
    evalBtns.querySelectorAll('.tg-btn').forEach(function(b){
      b.classList.toggle('selected',b.dataset.value===String(ml));
      b.onclick=function(){
        evalBtns.querySelectorAll('.tg-btn').forEach(function(x){x.classList.remove('selected')});
        this.classList.add('selected');
      };
    });
  }
  updateAiHelpRows();
  openModal('aiHelpModal');
}
// 根据所选算法显示/隐藏深度与评估函数行（strategy 无需深度与评估）
function updateAiHelpRows(){
  const sel=document.querySelector('#aiHelpAlgBtns .am-btn.selected');
  const val=sel?sel.dataset.value:'alphabeta';
  const needDepth=val==='alphabeta'||val==='pvs'||val==='mcts';
  const needEval=val==='alphabeta'||val==='pvs';
  const dr=document.getElementById('aiHelpDepthRow');if(dr)dr.style.display=needDepth?'block':'none';
  const er=document.getElementById('aiHelpEvalRow');if(er)er.style.display=needEval?'block':'none';
}
function closeAiHelpModal(){closeModal('aiHelpModal')}
// 获取当前一步走法建议：调用引擎，仅高亮推荐落点
async function requestAiSuggestion(){
  closeAiHelpModal();
  clearAiHint();
  if(gameOver||isPaused||aiThinking)return;
  const pid=curPlayer;
  const sel=document.querySelector('#aiHelpAlgBtns .am-btn.selected');
  const alg=sel?sel.dataset.value:'alphabeta';
  const depth=parseInt(document.getElementById('aiHelpDepthValue').textContent)||3;
  const evalSel=document.querySelector('#aiHelpEvalToggle .tg-btn.selected');
  const useMlEval=evalSel?evalSel.dataset.value==='true':true;
  let cmd;
  if(alg==='mcts'){cmd='ai_move_mcts';}
  else if(alg==='pvs'){cmd='ai_move_v2';}
  else if(alg==='alphabeta'){cmd='ai_move';}
  else{cmd='ai_move_strategy';}
  const args={
    board:board,size:size,player:pid,depth:depth,
    eliminated:[...eliminatedPlayers],maxPlayers:maxPlayers,
    borderMode:borderMode,capMode:capMode,gameCount:gameCount,firstMovePos:firstMovePos,
    randomScale:10,useMlEval:useMlEval
  };
  if(alg==='pvs')args.algorithm=alg;
  try{
    const result=await tauriInvoke(cmd,args);
    if(!result||result.length!==2)return;
    if(gameOver||isPaused||aiThinking)return;
    if(curPlayer!==pid)return;
    const x=result[0],y=result[1];
    clearAiHint();
    if(cells&&cells[x]&&cells[x][y]){
      cells[x][y].classList.add('ai-hint');
      showMsg(`AI 建议落在 ${x+1},${y+1}`,'');
    }
  }catch(e){}
}

/* ═══════ 一键终局（场上只剩 AI 时快速结算） ═══════ */
function allAliveAreAI(){
  let aliveCnt=0;
  for(let p=0;p<maxPlayers;p++){
    if(eliminatedPlayers.has(p))continue;
    aliveCnt++;
    if(!aiPlayers.has(p))return false;
  }
  return aliveCnt>=2;
}
function updateFastFinishBtn(){
  const btn=document.getElementById('fastFinishBtn');
  if(!btn)return;
  const show=!gameOver&&allAliveAreAI()&&typeof board!=='undefined'&&board.length>0;
  btn.style.display=show?'inline-block':'none';
}
function showFastFinishOverlay(show){
  const ov=document.getElementById('fastFinishOverlay');
  if(ov)ov.style.display=show?'flex':'none';
}
function confirmFastFinish(){
  openModal('fastFinishConfirm');
}
async function fastFinish(){
  if(gameOver||fastFinishing)return;
  fastFinishing=true;
  aiThinking=true;         // 拦截排队的 setTimeout(triggerAI) 与人类点击
  isPaused=true;           // 暂停游戏状态，后台棋盘不再继续
  clearAiHint();
  showFastFinishOverlay(true);
  updateFastFinishBtn();
  const alive=[];
  for(let p=0;p<maxPlayers;p++){if(!eliminatedPlayers.has(p)&&hasPieces(p,board))alive.push(p);}
  const cfgArg={};
  for(const p of alive){
    const ac=aiConfigs[p]||{};
    cfgArg[p]={algorithm:ac.algorithm||aiAlgorithm,depth:ac.depth||aiDepth,useMlEval:ac.useMlEval!==false};
  }
  try{
    const r=await tauriInvoke('simulate_to_end',{
      board:board,size:size,maxPlayers:maxPlayers,
      curPlayer:curPlayer,
      eliminated:[...eliminatedPlayers],
      borderMode:borderMode,
      capMode:capMode,
      firstMovePos:firstMovePos,
      gameCount:gameCount,
      aiConfigs:cfgArg
    });
    board=r.board;
    eliminatedPlayers=new Set(r.eliminatedOrder||[]);
    // 保留真实落子步骤，追加模拟步骤（带每步落子 mv，供回放使用）
    const simHist=(r.history||[]).map(function(h){return {turn:h.turn,snapshot:h.snapshot,mv:h.mv?{x:h.mv[0],y:h.mv[1],player:h.mv[2],seed:h.mv[3]||0}:null}});
    const baseTurn=gameHistory.length;
    gameHistory=[...gameHistory,...simHist.map(function(h){return {...h,turn:baseTurn+h.turn}})];
    chainStats=r.chainStats||{};
    maxChainOverall=r.maxChain||{player:null,length:0};
    eliminationRounds=[r.eliminatedOrder||[]];
    eliminationInfo={};
    for(const kb of (r.killedBy||[])){
      if(!eliminationInfo[kb[0]])eliminationInfo[kb[0]]=kb[1];
    }
    gameOver=true;
    isPaused=false;
    aiThinking=false;
    fastFinishing=false;
    renderBoard(true);
    renderPlayerBar();
    showFastFinishOverlay(false);
    showSettlement(r.winner,_colorNames||COLOR_NAMES,gameHistory);
  }catch(e){
    logWarn('simulate_to_end failed:',e);
    isPaused=false;
    aiThinking=false;
    fastFinishing=false;
    showFastFinishOverlay(false);
    showMsg('终局计算失败：'+e,'error');
    updateFastFinishBtn();
  }
}

async function triggerAI(){
  if(gameOver||aiThinking||isPaused||animating)return;
  if(!aiPlayers.has(curPlayer))return;
  // AI 首子由玩家放置：AI 无棋子时，由玩家点击落子
  if(!hasPieces(curPlayer,board)){
    showMsg('请为 '+(window._colorNames&&window._colorNames[curPlayer]||('AI '+(curPlayer+1)))+' 落下首子，点击空位置', 'hint');
    return;
  }
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
    borderMode: borderMode,
    capMode: capMode,
    gameCount: gameCount,
    firstMovePos: firstMovePos,
    randomScale: randomScale,
    useMlEval: useMlEval,
  };
  if(alg==='pvs')args.algorithm=alg;
  try {
    const result = await tauriInvoke(cmd, args);
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
    updateFastFinishBtn();
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
  const rndSeed=(Math.floor(Math.random()*0x7fffffff))>>>0;
  let elim, killedBy;
  try {
    let result=await tauriInvoke('process_move',{
      board:board,size:size,x:x,y:y,
      player:curPlayer,maxPlayers:maxPlayers,
      borderMode:borderMode,
      capMode:capMode,
      seed:rndSeed
    });
    elim=await processClick(board,size,x,y,curPlayer,'explode',COLORS[curPlayer],result);
    board=result.board;
    killedBy=result.killedBy;
    elim=result.eliminated||[];
  }catch(e){
    elim=await processClick(board,size,x,y,curPlayer,'explode',COLORS[curPlayer]);
  }
  handleEliminations(elim, killedBy, curPlayer);
  recordHistory({x:x,y:y,player:curPlayer,seed:rndSeed});
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
  updateFastFinishBtn();
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
  // 先放加载占位，避免面板以空内容（深色背景）闪一下黑，
  // 数据（排名+图表）就绪后再替换
  let ps0=document.getElementById('pauseStats');
  let pc0=document.getElementById('pauseCharts');
  if(ps0)ps0.innerHTML='<div style="text-align:center;color:var(--dim);font-size:.82rem;padding:24px 0">正在加载统计…</div>';
  if(pc0)pc0.innerHTML='';
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

  // 排名列表
  let statsGrid=document.getElementById('pauseStats');
  statsGrid.innerHTML='';
  // 添加排名标题
  let rankTitle=document.createElement('h4');
  rankTitle.style.cssText='margin:0 0 6px 0;font-size:.85rem;color:var(--text);text-align:center';
  rankTitle.textContent='当前排名';
  statsGrid.appendChild(rankTitle);
  renderRankingList(statsGrid, board, maxPlayers, eliminatedPlayers, _colorNames||COLOR_NAMES, aiConfigs, null, chainStats, fullHistory, null, null, curPlayer);
  // 图表区域
  let chartArea=document.getElementById('pauseCharts');
  chartArea.innerHTML='';
  renderGameCharts(chartArea,fullHistory,{
    colorNames:_colorNames||COLOR_NAMES,colors:COLORS,eliminated:eliminatedPlayers,
    _noBack:true,_noGrid:true
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
    eliminationInfo: {...eliminationInfo},
    eliminationRounds: eliminationRounds.slice(),
    chainStats: JSON.parse(JSON.stringify(chainStats)),
    maxChainOverall: {...maxChainOverall},
    aiMove: null,  // AI 缓存走法，悔棋时无需重算
  });
  // 最多保留 50 步防止内存溢出
  if(undoStack.length>50)undoStack.shift();
  updateUndoBtn();
}
function undoLastMove(){
  if(undoStack.length===0||gameOver||aiThinking||animating)return;
  let state=undoStack.pop();
  board=state.board;
  curPlayer=state.curPlayer;
  firstMovePos=state.firstMovePos;
  eliminatedPlayers=state.eliminatedPlayers;
  eliminationInfo=state.eliminationInfo||{};
  eliminationRounds=state.eliminationRounds||[];
  chainStats=state.chainStats;
  maxChainOverall=state.maxChainOverall;
  // 移除 gameHistory 最后一条
  if(gameHistory.length>0)gameHistory.pop();
  renderBoard(true);
  renderPlayerBar();
  setBg(curPlayer);
  updateFastFinishBtn();
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
  const rndSeed=(Math.floor(Math.random()*0x7fffffff))>>>0;
  let elim, killedBy;
  try {
    let result=await tauriInvoke('process_move',{
      board:board,size:size,x:x,y:y,
      player:curPlayer,maxPlayers:maxPlayers,
      borderMode:borderMode,
      capMode:capMode,
      seed:rndSeed
    });
    elim=await processClick(board,size,x,y,curPlayer,null,COLORS[curPlayer],result);
    board=result.board;
    killedBy=result.killedBy;
    elim=result.eliminated||[];
  }catch(e){
    elim=await processClick(board,size,x,y,curPlayer,null,COLORS[curPlayer]);
  }
  handleEliminations(elim, killedBy, curPlayer);
  recordHistory({x:x,y:y,player:curPlayer,seed:rndSeed});
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
  updateFastFinishBtn();
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
    await saveUnfinishedGameHistory();
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
  for(let[cx,cy]of changed){
    renderBoard(true);
    let el=cells?.[cx]?.[cy];
    if(el)el.classList.add('explode');
    await sleep(180);
    if(el)el.classList.remove('explode');
  }
  renderBoard(true);
}

async function localClick(x,y){
  if(gameOver||aiThinking||isPaused||animating)return;
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
    const rndSeed=(Math.floor(Math.random()*0x7fffffff))>>>0;
    let elim, killedBy;
    try {
      let result=await tauriInvoke('process_move',{
        board:board,size:size,x:x,y:y,
        player:curPlayer,maxPlayers:maxPlayers,
        borderMode:borderMode,
        capMode:capMode,
        seed:rndSeed
      });
      elim=await processClick(board,size,x,y,curPlayer,'explode',COLORS[curPlayer],result);
      board=result.board;
      killedBy=result.killedBy;
      elim=result.eliminated||[];
    }catch(e){
      elim=await processClick(board,size,x,y,curPlayer,'explode',COLORS[curPlayer]);
    }
    handleEliminations(elim, killedBy, curPlayer);
    recordHistory({x:x,y:y,player:curPlayer,seed:rndSeed});
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
    updateFastFinishBtn();
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
  const rndSeed=(Math.floor(Math.random()*0x7fffffff))>>>0;
  let elim, killedBy;
  try {
    let result=await tauriInvoke('process_move',{
      board:board,size:size,x:x,y:y,
      player:curPlayer,maxPlayers:maxPlayers,
      borderMode:borderMode,
      capMode:capMode,
      seed:rndSeed
    });
    elim=await processClick(board,size,x,y,curPlayer,'explode',COLORS[curPlayer],result);
    board=result.board;
    killedBy=result.killedBy;
    elim=result.eliminated||[];
  }catch(e){
    elim=await processClick(board,size,x,y,curPlayer,'explode',COLORS[curPlayer]);
  }
  handleEliminations(elim, killedBy, curPlayer);
  recordHistory({x:x,y:y,player:curPlayer,seed:rndSeed});
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
  updateFastFinishBtn();
  if(aiPlayers.has(curPlayer)&&!gameOver)setTimeout(()=>triggerAI(),400);
}

function handleClick(x,y){
  if(!aiThinking&&!isPaused&&!animating)localClick(x,y);
}

/* ==================== SETTLEMENT ==================== */
function getPlayerLabel(p){
  if(p===undefined||p===null)return '未知';
  if(_colorNames&&_colorNames[p])return _colorNames[p];
  return aiPlayers.has(p)?`AI ${p+1}`:`玩家 ${p+1}`;
}

// 处理一轮淘汰：记录击败者、播放音效、toast 播报
function handleEliminations(elim, killedBy, curPlayer){
  if(!elim||elim.length===0)return;
  var parts=[];
  for(let e of elim){
    eliminatedPlayers.add(e);
    playElim();
    var killer=null;
    if(killedBy&&killedBy.length){
      for(var i=0;i<killedBy.length;i++){
        if(killedBy[i][0]===e){killer=killedBy[i][1];break}
      }
    }
    if(killer===null||killer===undefined)killer=curPlayer; // 兜底：视为当前落子玩家
    eliminationInfo[e]=killer;
    parts.push(getPlayerLabel(e)+' 被 '+getPlayerLabel(killer)+' 淘汰');
  }
  eliminationRounds.push([...elim]);
  if(parts.length){try{showToast('❌ '+parts.join('；'))}catch(e){logWarn('showToast failed:',e)}}
}

// 获取某玩家被谁击败（返回击败者 pid，无则 null）
function getEliminatorPid(pid){
  if(pid===undefined||pid===null)return null;
  var k=eliminationInfo[pid];
  return (k===undefined||k===null)?null:k;
}
// 获取某玩家被谁击败的名字（无则 null）
function getEliminatorName(pid, colorNames){
  var k=getEliminatorPid(pid);
  if(k===null)return null;
  return (colorNames&&colorNames[k])||getPlayerLabel(k);
}

// 获取玩家类型标签
function getPlayerTypeLabel(p){
  if(!aiPlayers.has(p))return '人类';
  var cfg=aiConfigs[p]||{};
  var alg=cfg.algorithm||'strategy';
  var labels={strategy:'AI 策略',alphabeta:'A-B',pvs:'PVS',mcts:'MCTS'};
  return labels[alg]||'AI '+alg;
}

// 绘制排名列表
function renderRankingList(container, boardState, playerCnt, eliminatedSet, colorNames, aiCfgMap, winnerP, chainStatsMap, fullHistory, playerDataOverride, playerTypesOverride, currentPlayerIdx, killerMap){
  var data=[];
  var maxPieces=0;
  for(var p=0;p<playerCnt;p++){
    var pieces=0,points=0;
    if(playerDataOverride&&playerDataOverride[p]){
      pieces=playerDataOverride[p].pieces||0;
      points=playerDataOverride[p].points||0;
    }else{
      for(var r=0;r<boardState.length;r++)for(var c=0;c<boardState[r].length;c++){
        var cell=boardState[r][c];
        if(cell.owner===p){pieces++;points+=cell.count}
      }
    }
    if(pieces>maxPieces)maxPieces=pieces;
    var typeRaw,typeLabel,typeCss,detailStr='';
    if(playerTypesOverride&&playerTypesOverride[p]){
      typeRaw=playerTypesOverride[p];
      var typeMap={strategy:'AI 策略',alphabeta:'A-B',pvs:'PVS',mcts:'MCTS',human:'人类'};
      var cssMap={strategy:'ai-strategy',alphabeta:'ai-alphabeta',pvs:'ai-pvs',mcts:'ai-mcts',human:'human'};
      typeLabel=typeMap[typeRaw]||typeRaw;
      typeCss='rank-type '+cssMap[typeRaw];
    }else if(aiCfgMap&&aiCfgMap[p]){
      var cfg=aiCfgMap[p];
      typeRaw=cfg.algorithm||'strategy';
      var typeMap2={strategy:'AI 策略',alphabeta:'A-B',pvs:'PVS',mcts:'MCTS'};
      var cssMap2={strategy:'ai-strategy',alphabeta:'ai-alphabeta',pvs:'ai-pvs',mcts:'ai-mcts'};
      typeLabel=typeMap2[typeRaw]||('AI '+typeRaw);
      typeCss='rank-type '+(cssMap2[typeRaw]||'');
      if(typeRaw==='strategy'){
        detailStr='';
      }else{
        detailStr='深度: '+cfg.depth+'  随机: '+cfg.randomScale+'%';
        if(typeRaw!=='mcts'){
          if(cfg.useMlEval===false)detailStr+=' 手写评估';
          else if(cfg.useMlEval===true||cfg.useMlEval===undefined)detailStr+=' ML评估';
        }
      }
    }else{
      typeRaw='human';
      typeLabel='人类';
      typeCss='rank-type human';
    }
    data.push({index:p,name:(colorNames&&colorNames[p])||getPlayerLabel(p),pieces:pieces,points:points,type:typeLabel,typeCss:typeCss,typeRaw:typeRaw,detail:detailStr,eliminated:(eliminatedSet&&eliminatedSet.has(p)),isWinner:p===winnerP,chain:chainStatsMap&&chainStatsMap[p]});
  }
  // 统一排名：活跃玩家按棋子数降序，同棋子数按点数降序
  // 淘汰玩家按淘汰顺序排列（先淘汰者末位）
  var elimOrder=[];
  if(eliminatedSet&&eliminatedSet.size>0){
    elimOrder=Array.from(eliminatedSet);
  }
  data.sort(function(a,b){
    if(a.eliminated&&!b.eliminated)return 1;
    if(!a.eliminated&&b.eliminated)return -1;
    if(a.eliminated&&b.eliminated){
      var ra=getEliminationRound(a.index);
      var rb=getEliminationRound(b.index);
      if(ra!==rb)return rb-ra; // 后淘汰者（更大轮次）排前面，先淘汰者排后面
      return 0; // 同一轮淘汰 = 并列
    }
    if(b.pieces!==a.pieces)return b.pieces-a.pieces;
    return b.points-a.points;
  });
  function getEliminationRound(pid){
    if(typeof eliminationRounds!=='undefined'&&eliminationRounds.length>0){
      for(var ri=0;ri<eliminationRounds.length;ri++){
        if(eliminationRounds[ri].indexOf(pid)>=0)return ri;
      }
    }
    if(eliminatedSet&&eliminatedSet.has(pid)){
      var eo=elimOrder.indexOf(pid);
      return eo>=0?eo:999;
    }
    return 999;
  }
  var leaderPieces=data.length>0?data[0].pieces:0;
  var table=document.createElement('div');table.className='rank-list';
  for(var i=0;i<data.length;i++){
    var d=data[i];
    var item=document.createElement('div');item.className='rank-item';
    if(d.isWinner)item.classList.add('winner');
    if(d.eliminated)item.classList.add('eliminated');
    if(currentPlayerIdx!==undefined&currentPlayerIdx!==null&&d.index===currentPlayerIdx&&!d.isWinner)item.classList.add('current-turn');
    item.style.setProperty('--rank-left-c',COLORS[d.index]||'#888');
    var barPct=maxPieces>0?Math.round(d.pieces/maxPieces*100):0;
    item.style.setProperty('--rank-bar-w',barPct+'%');
    item.style.setProperty('--rank-bar-c',COLORS[d.index]||'#888');
    var rankEl=document.createElement('span');rankEl.className='rank-pos';
    if(d.isWinner){rankEl.textContent='🏆';rankEl.classList.add('trophy')}
    else if(d.eliminated){rankEl.textContent='#'+(i+1);}
    else if(i===0){rankEl.textContent='🥇';rankEl.classList.add('medal')}
    else if(i===1){rankEl.textContent='🥈';rankEl.classList.add('medal')}
    else if(i===2){rankEl.textContent='🥉';rankEl.classList.add('medal')}
    else rankEl.textContent='#'+(i+1);
    item.appendChild(rankEl);
    var nameLine=document.createElement('div');nameLine.className='rank-name-line';
    var dot=document.createElement('span');dot.className='rank-dot';dot.style.background=COLORS[d.index]||'#888';
    var nameEl=document.createElement('span');nameEl.className='rank-name';nameEl.textContent=d.name;
    nameLine.appendChild(dot);nameLine.appendChild(nameEl);
    item.appendChild(nameLine);
    var statsLine=document.createElement('div');statsLine.className='rank-stats-line';
    var piecesStat=document.createElement('span');piecesStat.className='rank-stat';
    piecesStat.innerHTML='<span class="rank-stat-val">'+d.pieces+'</span><span class="rank-stat-lbl">棋</span>';
    statsLine.appendChild(piecesStat);
    var pointsStat=document.createElement('span');pointsStat.className='rank-stat';
    pointsStat.innerHTML='<span class="rank-stat-val">'+d.points+'</span><span class="rank-stat-lbl">点</span>';
    statsLine.appendChild(pointsStat);
    item.appendChild(statsLine);
    var typeLine=document.createElement('div');typeLine.className='rank-type-line';
    if(d.typeCss){
      var typeEl=document.createElement('span');typeEl.className=d.typeCss;typeEl.textContent=d.type;
      typeLine.appendChild(typeEl);
    }
    if(!d.isWinner&&!d.eliminated&&leaderPieces>d.pieces){
      var gapEl=document.createElement('span');gapEl.className='rank-gap';
      gapEl.textContent='-❤~'+d.pieces+'棋';
      typeLine.appendChild(gapEl);
    }
    if(d.chain&&d.chain.triggered>0){
      var chainEl=document.createElement('span');chainEl.className='rank-chain-badge';
      chainEl.textContent='连炸'+d.chain.triggered+'次';
      typeLine.appendChild(chainEl);
    }
    item.appendChild(typeLine);
    if(d.eliminated){
      var elimLine=document.createElement('div');elimLine.className='rank-elim-line';
      var kName=null;
      if(typeof killerMap!=='undefined'&&killerMap&&killerMap[d.index]!==undefined&&killerMap[d.index]!==null){
        kName=(colorNames&&colorNames[killerMap[d.index]])||getPlayerLabel(killerMap[d.index]);
      }else{
        kName=getEliminatorName(d.index,colorNames);
      }
      if(kName){
        elimLine.appendChild(document.createTextNode('⚔ 被 '));
        var kSpan=document.createElement('span');kSpan.className='rank-elim-killer';kSpan.textContent=kName;
        elimLine.appendChild(kSpan);
        elimLine.appendChild(document.createTextNode(' 淘汰'));
      }else{
        elimLine.textContent='✖ 已淘汰';
      }
      item.appendChild(elimLine);
    }
    if(d.detail){
      item.classList.add('has-detail');
      var detailLine=document.createElement('div');detailLine.className='rank-detail-line';
      detailLine.textContent=d.detail;
      item.appendChild(detailLine);
    }
    item.onclick=function(idx,pid){return function(e){
      e.stopPropagation();
      showPlayerRankModal(pid, colorNames, playerCnt, eliminatedSet, aiCfgMap, winnerP, chainStatsMap, fullHistory, playerTypesOverride, killerMap);
    }}(i,d.index);
    table.appendChild(item);
  }
  container.appendChild(table);
}

// 玩家详情弹窗
function showPlayerRankModal(pid, colorNames, playerCnt, eliminatedSet, aiCfgMap, winnerP, chainStatsMap, fullHistory, playerTypesOverride, killerMap){
  var pName=(colorNames&&colorNames[pid])||getPlayerLabel(pid);
  var isElim=eliminatedSet&&eliminatedSet.has(pid);
  var isWinner=pid===winnerP;
  var pColor=COLORS[pid]||'#888';
  // 获取玩家类型
  var typeRaw,typeLabel,typeCss,detailStr='';
  if(playerTypesOverride&&playerTypesOverride[pid]){
    typeRaw=playerTypesOverride[pid];
    var tm={strategy:'AI 策略',alphabeta:'A-B',pvs:'PVS',mcts:'MCTS',human:'人类'};
    var tmc={strategy:'ai-strategy',alphabeta:'ai-alphabeta',pvs:'ai-pvs',mcts:'ai-mcts',human:'human'};
    typeLabel=tm[typeRaw]||typeRaw;
    typeCss=tmc[typeRaw]||'';
  }else if(aiCfgMap&&aiCfgMap[pid]){
    var cfg=aiCfgMap[pid];
    typeRaw=cfg.algorithm||'strategy';
    var tm2={strategy:'AI 策略',alphabeta:'A-B',pvs:'PVS',mcts:'MCTS'};
    var tmc2={strategy:'ai-strategy',alphabeta:'ai-alphabeta',pvs:'ai-pvs',mcts:'ai-mcts'};
    typeLabel=tm2[typeRaw]||('AI '+typeRaw);
    typeCss=tmc2[typeRaw]||'';
    if(typeRaw==='strategy'){
      detailStr='';
    }else{
      detailStr='深度: '+cfg.depth+' | 随机: '+cfg.randomScale+'%';
      if(typeRaw!=='mcts'){
        if(cfg.useMlEval===false)detailStr+=' | 手写评估';
        else if(cfg.useMlEval===true||cfg.useMlEval===undefined)detailStr+=' | ML评估';
      }
    }
  }else{
    typeRaw='human';
    typeLabel='人类';
    typeCss='human';
  }
  // 从 fullHistory 提取该玩家的历史
  var playerHistory=[];
  if(fullHistory&&fullHistory.length>0){
    for(var hi=0;hi<fullHistory.length;hi++){
      var snap=fullHistory[hi].snapshot||{};
      var pd=snap[String(pid)];
      if(pd){
        playerHistory.push({turn:fullHistory[hi].turn||hi,snapshot:{}});
        playerHistory[playerHistory.length-1].snapshot[String(pid)]={pieces:pd.pieces||0,points:pd.points||0};
      }
    }
  }
  // 获取最后一幕的数据
  var finalPieces=0,finalPoints=0;
  if(playerHistory.length>0){
    var lastP=playerHistory[playerHistory.length-1].snapshot[String(pid)];
    if(lastP){finalPieces=lastP.pieces;finalPoints=lastP.points}
  }
  // 创建弹窗
  var overlay=document.createElement('div');overlay.className='rank-modal';
  history.pushState(null,'');
  window._rankModalOpen=1;
  var inner=document.createElement('div');inner.className='rank-modal-inner';
  inner.style.setProperty('--rm-color',pColor);
  // 头部
  var header=document.createElement('div');header.className='rank-modal-header';
  var hName=document.createElement('span');hName.className='rank-modal-name';
  hName.textContent=pName;
  var statusEl=document.createElement('span');statusEl.className='rank-modal-status';
  if(isWinner){statusEl.textContent='🏆 获胜';statusEl.classList.add('winner')}
  else if(isElim){
    var kName2=null;
    if(typeof killerMap!=='undefined'&&killerMap&&killerMap[pid]!==undefined&&killerMap[pid]!==null){
      kName2=(colorNames&&colorNames[killerMap[pid]])||getPlayerLabel(killerMap[pid]);
    }else{
      kName2=getEliminatorName(pid,colorNames);
    }
    statusEl.textContent=kName2?('✖ 被 '+kName2+' 淘汰'):'✖ 已淘汰';
    statusEl.classList.add('eliminated')
  }
  else{statusEl.textContent='● 存活';statusEl.classList.add('alive')}
  var closeBtn=document.createElement('button');closeBtn.className='rank-modal-close';
  closeBtn.textContent='✕';
  closeBtn.onclick=function(){document.body.removeChild(overlay);};
  header.appendChild(hName);header.appendChild(statusEl);header.appendChild(closeBtn);
  inner.appendChild(header);
  // 内容体
  var body=document.createElement('div');body.className='rank-modal-body';
  // Hero: 大数字展示棋子数和点数
  var hero=document.createElement('div');hero.className='rank-modal-hero';
  var heroPieces=document.createElement('div');heroPieces.className='rank-modal-hero-item';
  heroPieces.innerHTML='<div class="hero-label">棋子数</div><div class="hero-value pieces-color">'+finalPieces+'</div>';
  var heroPoints=document.createElement('div');heroPoints.className='rank-modal-hero-item';
  heroPoints.innerHTML='<div class="hero-label">点数</div><div class="hero-value">'+finalPoints+'</div>';
  hero.appendChild(heroPieces);hero.appendChild(heroPoints);
  body.appendChild(hero);
  // 类型 + AI 详情
  var typeRow=document.createElement('div');typeRow.className='rank-modal-type-row';
  var typeBadge=document.createElement('span');typeBadge.className='rank-modal-type-badge'+(typeCss?' '+typeCss:'');
  typeBadge.textContent=typeLabel;
  typeRow.appendChild(typeBadge);
  var typeDetail=document.createElement('span');typeDetail.className='rank-modal-type-detail';
  typeDetail.textContent=detailStr||'—';
  typeRow.appendChild(typeDetail);
  body.appendChild(typeRow);
  // 连炸统计
  if(chainStatsMap&&chainStatsMap[pid]&&chainStatsMap[pid].triggered>0){
    var cs=chainStatsMap[pid];
    var chainRow=document.createElement('div');chainRow.className='rank-modal-chain-row';
    chainRow.innerHTML='<span class="rank-modal-chain-item"><span class="chain-icon">💥</span><span>连炸 <strong class="chain-val">'+cs.triggered+'</strong> 次</span></span>'+
      '<span class="rank-modal-chain-item"><span class="chain-icon">🔥</span><span>最高 <strong class="chain-val">'+cs.maxChain+'</strong> 连</span></span>';
    body.appendChild(chainRow);
  }
  // 图表
  if(playerHistory.length>=2){
    var cNames={};cNames[String(pid)]=pName;
    var colors={};colors[String(pid)]=pColor;
    var cId1='rmP_'+Date.now()+'_'+Math.random().toString(36).slice(2,6);
    var cId2='rmPt_'+Date.now()+'_'+Math.random().toString(36).slice(2,6);
    var cb1=document.createElement('div');cb1.className='rank-modal-chart-box';
    cb1.innerHTML='<h4 onclick="showFullscreenChart(\''+cId1+'\')">棋子数变化 🔍</h4><canvas id="'+cId1+'" onclick="showFullscreenChart(\''+cId1+'\')" style="cursor:pointer"></canvas>';
    body.appendChild(cb1);
    var cb2=document.createElement('div');cb2.className='rank-modal-chart-box';
    cb2.innerHTML='<h4 onclick="showFullscreenChart(\''+cId2+'\')">点数变化 🔍</h4><canvas id="'+cId2+'" onclick="showFullscreenChart(\''+cId2+'\')" style="cursor:pointer"></canvas>';
    body.appendChild(cb2);
    setTimeout(function(){
      var el1=document.getElementById(cId1);if(el1){var r1=el1.getBoundingClientRect();if(r1.width>0&&r1.height>0)drawLineChart(el1,playerHistory,cNames,colors,'pieces');else{var p1=function(){var r=el1.getBoundingClientRect();if(r.width>0&&r.height>0){drawLineChart(el1,playerHistory,cNames,colors,'pieces')}else requestAnimationFrame(p1)};requestAnimationFrame(p1)}}
      var el2=document.getElementById(cId2);if(el2){var r2=el2.getBoundingClientRect();if(r2.width>0&&r2.height>0)drawLineChart(el2,playerHistory,cNames,colors,'points');else{var p2=function(){var r=el2.getBoundingClientRect();if(r.width>0&&r.height>0){drawLineChart(el2,playerHistory,cNames,colors,'points')}else requestAnimationFrame(p2)};requestAnimationFrame(p2)}}
    },50);
  }else{
    body.innerHTML+='<p style="text-align:center;color:var(--dim);font-size:.78rem;margin-top:8px">历史数据不足，无法绘制图表</p>';
  }
  inner.appendChild(body);
  overlay.appendChild(inner);
  overlay.onclick=function(e){if(e.target===overlay){document.body.removeChild(overlay);}};
  document.body.appendChild(overlay);
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
  if(!opts._noGrid){
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
  }
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
  if(c.capMode)capMode=c.capMode;
  if(c.borderMode)borderMode=c.borderMode;
  // 随机模式：再来一局同样在开局瞬间重新随机确定，整局不再改变
  if(capMode==='random')capMode=resolveRandomCap();
  if(borderMode==='random')borderMode=resolveRandomBorder();
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
  clearAiHint();
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
  // 排名列表
  let rankTitle=document.createElement('h4');
  rankTitle.style.cssText='margin:0 0 6px 0;font-size:.85rem;color:var(--text);text-align:center';
  rankTitle.textContent='最终排名';
  inner.appendChild(rankTitle);
  renderRankingList(inner, board, maxPlayers, eliminatedPlayers, colorNames||_colorNames||COLOR_NAMES, aiConfigs, winner, chainStats, fullHistory, null, null, null);
  renderGameCharts(inner, fullHistory, {
    winner, colorNames, colors: COLORS, eliminated: eliminatedPlayers,
    chainStats, maxChain: maxChainOverall, showTitle: false, _noBack: true, _noGrid: true,
    onReplay: () => { Router.navigate(_originPage || 'welcome'); }
  });
  // 回放对局按钮
  if(hasReplayMoves(fullHistory)){
    const replayBtn = document.createElement('button');
    replayBtn.className = 'glass-btn primary';
    replayBtn.textContent = '▶ 回放对局';
    replayBtn.style.marginTop = '6px';
    replayBtn.onclick = () => {
      openReplay({
        size: size,
        maxPlayers: maxPlayers,
        borderMode: borderMode || 'default',
        capMode: capMode || '4',
        gameCount: gameCount || 0,
        colorNames: _colorNames || colorNames || COLOR_NAMES,
        history: fullHistory,
      });
    };
    inner.appendChild(replayBtn);
  }
  // 返回主菜单按钮
  const homeBtn = document.createElement('button');
  homeBtn.className = 'glass-btn primary';
  homeBtn.textContent = '返回主菜单';
  homeBtn.style.marginTop = '6px';
  homeBtn.onclick = () => { Router.navigate('welcome'); };
  inner.appendChild(homeBtn);
  Router.navigate('checkout', prevPage);
  await saveGameHistory(winner, undefined, undefined, undefined, fullHistory);
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
  const histPlayerCnt = r.playerCount || maxPlayers;
  maxPlayers = histPlayerCnt;
  // 排名列表（从最后一幕提取棋子数/点数）
  var rankingTitle=document.createElement('h4');
  rankingTitle.style.cssText='margin:0 0 6px 0;font-size:.85rem;color:var(--text);text-align:center';
  rankingTitle.textContent='最终排名';
  inner.appendChild(rankingTitle);
  var finalSnap={};
  if(historyData&&historyData.length>0){
    var lastTurn=historyData[historyData.length-1];
    if(lastTurn&&lastTurn.snapshot){
      for(var ps in lastTurn.snapshot){
        finalSnap[ps]=lastTurn.snapshot[ps];
      }
    }
  }
  // 判断被淘汰的玩家（最后一幕棋子数为0）
  var histEliminated=new Set();
  for(var pe=0;pe<histPlayerCnt;pe++){
    if(!finalSnap[String(pe)]||!finalSnap[String(pe)].pieces)histEliminated.add(pe);
  }
  renderRankingList(inner, [], histPlayerCnt, histEliminated, r.colorNames, null, r.winner, r.chainStats, historyData, finalSnap, r.playerTypes, null, r.killedBy);
  renderGameCharts(inner, historyData, {
    winner: r.winner, colorNames: r.colorNames, colors: COLORS,
    chainStats: r.chainStats, maxChain: r.maxChain,
    showTitle: false, _noBack: true, _noGrid: true,
  });
  // 回放对局按钮（从记录中的落子数据重放）
  if(hasReplayMoves(historyData)){
    const replayBtn = document.createElement('button');
    replayBtn.className = 'glass-btn primary';
    replayBtn.textContent = '▶ 回放对局';
    replayBtn.onclick = () => {
      openReplay({
        size: r.boardSize || size,
        maxPlayers: histPlayerCnt,
        borderMode: r.borderMode || 'default',
        capMode: r.capMode || '4',
        gameCount: r.gameCount || 0,
        colorNames: r.colorNames,
        history: historyData,
      });
    };
    inner.appendChild(replayBtn);
  }
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
  gameMode=null;gameOver=false;isPaused=false;firstMovePos=null;curPlayer=0;aiThinking=false;fastFinishing=false;
  aiPlayers=new Set();eliminatedPlayers=new Set();eliminationRounds=[];eliminationInfo={};gameHistory=[];chainStats={};
  maxChainOverall={player:null,length:0};turnCount=0;gameCount=0;undoStack=[];
    clearAiHint();
  cells=[];board=[];
  try{document.getElementById('board').innerHTML='';}catch(e){}
  document.body.style.background='';
  // ★ 只移除动态 settlement，保留静态 #pauseOverlay，关闭模态
  document.querySelectorAll('.settlement').forEach(e=>{if(e.id!=='pauseOverlay')e.remove()});
  try{document.getElementById('pauseOverlay').style.display='none';}catch(e){}
  document.querySelectorAll('.modal').forEach(e=>e.classList.remove('show'));
  // ★ 重新显示 game screen 中的暂停按钮和消息，确保 DOM 干净
  try{document.getElementById('pauseBtn').textContent='暂停';}catch(e){}
  try{document.getElementById('fastFinishBtn').style.display='none';}catch(e){}
  try{document.getElementById('fastFinishOverlay').style.display='none';}catch(e){}
  clearAiHint();
  showMsg('','');
  Router.navigate('welcome');
}

/* ==================== ABOUT ==================== */
function renderChangelogCards(){
  var container=document.getElementById('changelogContainer');
  if(!container)return;
  var versions=[
    {v:'v3.3.1 · 第 36 版',desc:'新增随机玩法：爆炸阈值每步随机、棋盘边界每步随机、混合保持开局确定；首子等级改为阈值减一；关于页 AI 评测更新、暗色按钮质感优化、修复系统主题下游戏内亮色切换失效'},
    {v:'v3.3.0 · 第 35 版',desc:'新增Windows桌面版支持（自动更新适配、按平台提供对应安装包）、修复桌面图标显示、构建脚本支持APK/exe双平台'},
    {v:'v3.2.3 · 第 34 版',desc:'新增局内AI走法建议（按钮选择算法与深度、仅提示当前一步）、修复弹窗弹出闪烁、随机刻度设置真正生效、PWA离线缓存修复、AI介绍内容更新'},
    {v:'v3.2.2 · 第 33 版',desc:'新增大狗叫音频主题（爆炸音效可选3种叫声模式）、新增静音主题、音效主题按钮两行排版优化'},
    {v:'v3.2.1 · 第 32 版',desc:'修复了3.2.0仓促发布的各种bug'},
    {v:'v3.2.0 · 第 31 版',desc:'新增设置页面（主题切换/震动开关/预设音效/手动检查更新）、新增AI帮忙与一键终局、重写背景光球动画、修复对局结算显示问题'},
    {v:'v3.1.7 · 第 30 版',desc:'新增自动更新功能：启动时静默检查更新、有更新时弹窗提示、浏览器下载安装；修复中文文件名 URL 编码问题；优化提示文字为英文'},
    {v:'v3.1.6 · 第 29 版',desc:'新增自动更新功能：每次启动自动检测新版本、下载后自动打开安装包；优化游戏体验；修复已知问题'},
    {v:'v3.1.5 · 第 28 版',desc:'新增3个棋盘模式，丰富游戏体验'},
    {v:'v3.1.4 · 第 27 版',desc:'排名列表UI优化：可点击详情弹窗、暂停/结算/历史三处排名、局内改名取消、按钮布局优化'},
    {v:'v3.1.3 · 第 26 版',desc:'修复历史记录无法保存（colorNames 类型不匹配导致 Rust 反序列化失败）、恢复后图表丢失、局内算法修改弹窗'},
    {v:'v3.1.2 · 第 25 版',desc:'PVS 绝杀修复（终局符号反转）、Killer/History 动态优化、QSearch 深度收窄至3层、warmup 移除、分支收紧、AI 战力评测页面（560局8选手）'},
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
  const pName=playerConfigs[idx]?.name||('玩家'+(idx+1));
  document.getElementById('playerModalTitle').textContent=pName+' 设置';
  // 设置页面显示名称输入框
  const nameRow=document.getElementById('playerNameInput').closest('.form-row');
  if(nameRow)nameRow.style.display='block';
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
function closePlayerModal(){
  _inGameSettingsMode=false;
  closeModal('playerConfigModal');
}
var _inGameSettingsMode=false;
var _inGameSettingsIdx=-1;
function openInGamePlayerConfig(idx){
  _inGameSettingsMode=true;
  _inGameSettingsIdx=idx;
  var pName=(_colorNames&&_colorNames[idx])||(aiPlayers.has(idx)?'AI '+(idx+1):'玩家 '+(idx+1));
  document.getElementById('playerModalTitle').textContent=pName+' 设置';
  // 局内不显示名称输入框
  const nameRow=document.getElementById('playerNameInput').closest('.form-row');
  if(nameRow)nameRow.style.display='none';
  var isAI=aiPlayers.has(idx);
  var cfg=aiConfigs[idx]||{};
  var curType=isAI?(cfg.algorithm||'strategy'):'human';
  var curDepth=cfg.depth||2;
  var curRandom=cfg.randomScale??10;
  var curUseMlEval=cfg.useMlEval!==false;
  var nameInput=document.getElementById('playerNameInput');
  if(nameInput)nameInput.value=pName;
  document.getElementById('depthValue').textContent=String(curDepth);
  document.getElementById('randomScaleSlider').value=String(curRandom);
  document.getElementById('randomValueLabel').textContent=curRandom+'%';
  var btns=document.getElementById('playerTypeBtns');
  btns.querySelectorAll('.am-btn').forEach(function(b){b.classList.toggle('selected',b.dataset.value===curType)});
  btns.querySelectorAll('.am-btn').forEach(function(b){b.onclick=function(){btns.querySelectorAll('.am-btn').forEach(function(x){x.classList.remove('selected')});this.classList.add('selected');updateModalRows();}});
  updateModalRows();
  document.getElementById('depthDecBtn').onclick=function(){var e=document.getElementById('depthValue');var v=parseInt(e.textContent)||2;if(v>1){v--;e.textContent=v}};
  document.getElementById('depthIncBtn').onclick=function(){var e=document.getElementById('depthValue');var v=parseInt(e.textContent)||2;if(v<10){v++;e.textContent=v}};
  document.getElementById('randomScaleSlider').oninput=function(){document.getElementById('randomValueLabel').textContent=this.value+'%'};
  var evalBtns=document.getElementById('playerEvalToggle');
  if(evalBtns){
    evalBtns.querySelectorAll('.tg-btn').forEach(function(b){b.classList.toggle('selected',b.dataset.value===String(curUseMlEval))});
  }
  openModal('playerConfigModal');
}
function saveInGamePlayerConfig(){
  var idx=_inGameSettingsIdx;
  var selBtn=document.querySelector('#playerTypeBtns .am-btn.selected');
  var newType=selBtn?selBtn.dataset.value:'human';
  var newDepth=parseInt(document.getElementById('depthValue').textContent)||2;
  var newRandom=parseInt(document.getElementById('randomScaleSlider').value)??10;
  var evalSel=document.querySelector('#playerEvalToggle .tg-btn.selected');
  var newUseMlEval=evalSel?evalSel.dataset.value==='true':true;
  if(newType==='human'){
    if(aiPlayers.has(idx)){
      aiPlayers.delete(idx);
      delete aiConfigs[idx];
    }
  }else{
    aiPlayers.add(idx);
    aiConfigs[idx]={algorithm:newType,depth:newDepth,randomScale:newRandom,useMlEval:newUseMlEval};
    if(!aiConfigs[0]){aiAlgorithm=newType;aiDepth=newDepth}
  }
  _inGameSettingsMode=false;
  closeModal('playerConfigModal');
  renderPlayerBar();
  if(!gameOver&&idx===curPlayer&&aiPlayers.has(idx)&&hasPieces(idx,board)){
    setTimeout(function(){triggerAI()},300);
  }
}

function savePlayerConfig(){
  if(_inGameSettingsMode){
    saveInGamePlayerConfig();
    return;
  }
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
  const userBorderMode=getSelStr('borderModeGroup')||'default';
  const userCapMode=getSelStr('capModeGroup')||'4';
  borderMode=userBorderMode;
  capMode=userCapMode;
  // 随机模式：开局瞬间用时间戳种子确定具体模式，整局不再改变
  if(borderMode==='random')borderMode=resolveRandomBorder();
  if(capMode==='random')capMode=resolveRandomCap();
  board=mkBoard(size,capMode,gameCount);curPlayer=0;gameOver=false;isPaused=false;firstMovePos=null;
  document.getElementById('pauseBtn').textContent='暂停';
  gameMode='local';_originPage='gameSetup';
  aiPlayers=new Set();aiThinking=false;
  eliminatedPlayers=new Set();eliminationRounds=[];eliminationInfo={};gameHistory=[];chainStats={};
  maxChainOverall={player:null,length:0};
  let colorNames={};
  for(let i=0;i<cnt;i++){
    const cfg=playerConfigs[i];
    colorNames[i]=cfg.name||('玩家'+(i+1));
  }
  _colorNames=colorNames;
  recordHistory();
  renderBoard(true);
  show('game');
  document.body.style.background='';
  renderPlayerBar();
  _lastGameConfig={mode:'local',size,maxPlayers,colorNames,borderMode:userBorderMode,capMode:userCapMode};
}
function startAIFromSetup(sz,cnt){
  clearSavedGameState();
  location.hash='#game';
  resetRoundHistory();
  undoStack=[];
  size=sz;maxPlayers=cnt;
  const userBorderMode=getSelStr('borderModeGroup')||'default';
  const userCapMode=getSelStr('capModeGroup')||'4';
  borderMode=userBorderMode;
  capMode=userCapMode;
  // 随机模式：开局瞬间用时间戳种子确定具体模式，整局不再改变
  if(borderMode==='random')borderMode=resolveRandomBorder();
  if(capMode==='random')capMode=resolveRandomCap();
  board=mkBoard(size,capMode,gameCount);curPlayer=0;gameOver=false;isPaused=false;firstMovePos=null;
  document.getElementById('pauseBtn').textContent='暂停';
  gameMode='ai';_originPage='gameSetup';
  aiPlayers=new Set();aiConfigs={};aiThinking=false;
  eliminatedPlayers=new Set();eliminationRounds=[];eliminationInfo={};gameHistory=[];chainStats={};
  maxChainOverall={player:null,length:0};
  let colorNames={},humanIdx=-1;
  for(let i=0;i<cnt;i++){
    const cfg=playerConfigs[i];
    colorNames[i]=cfg.name||('玩家'+(i+1));
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
  updateFastFinishBtn();
  if(aiPlayers.has(0))setTimeout(()=>triggerAI(),400);
  _lastGameConfig={mode:'ai',size,aiCount:cnt-1,humanIdx,aiConfigs:JSON.parse(JSON.stringify(aiConfigs)),colorNames,borderMode:userBorderMode,capMode:userCapMode};
}
function startEveFromSetup(sz,cnt){
  clearSavedGameState();
  location.hash='#game';
  resetRoundHistory();
  undoStack=[];
  size=sz;maxPlayers=cnt;
  const userBorderMode=getSelStr('borderModeGroup')||'default';
  const userCapMode=getSelStr('capModeGroup')||'4';
  borderMode=userBorderMode;
  capMode=userCapMode;
  // 随机模式：开局瞬间用时间戳种子确定具体模式，整局不再改变
  if(borderMode==='random')borderMode=resolveRandomBorder();
  if(capMode==='random')capMode=resolveRandomCap();
  board=mkBoard(size,capMode,gameCount);curPlayer=0;gameOver=false;isPaused=false;firstMovePos=null;
  document.getElementById('pauseBtn').textContent='暂停';
  gameMode='eve';_originPage='gameSetup';
  aiPlayers=new Set();aiConfigs={};aiThinking=false;
  eliminatedPlayers=new Set();eliminationRounds=[];eliminationInfo={};gameHistory=[];chainStats={};
  maxChainOverall={player:null,length:0};
  let colorNames={};
  for(let i=0;i<cnt;i++){
    const cfg=playerConfigs[i];
    colorNames[i]=cfg.name||('AI '+(i+1));
    aiPlayers.add(i);
    aiConfigs[i]={algorithm:cfg.type,depth:cfg.depth,randomScale:cfg.randomScale??10,useMlEval:cfg.useMlEval!==false};
  }
  _colorNames=colorNames;
  recordHistory();
  renderBoard(true);
  show('game');
  document.body.style.background='';
  renderPlayerBar();
  updateFastFinishBtn();
  if(aiPlayers.has(0))setTimeout(()=>triggerAI(),400);
  _lastGameConfig={mode:'eve',size,maxPlayers:cnt,aiConfigs:JSON.parse(JSON.stringify(aiConfigs)),colorNames,borderMode:userBorderMode,capMode:userCapMode};
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
    // 1.2) 对局回放覆盖层 — 关闭回放，不切页面
    var rpOverlay=document.getElementById('replayOverlay');
    if(rpOverlay && rpOverlay.style.display==='flex'){
      if(typeof closeReplay==='function') closeReplay();
      handled = true;
    }
  }

  if(!handled){
    // 1.5) 玩家详情弹窗 (RankModal) — 关闭弹窗，不切页面
    var rankModal=document.querySelector('.rank-modal');
    if(rankModal){
      rankModal.remove();
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
      'settings':      () => Router.switchPage('welcome'),          // settings => 1
      'about':         () => Router.switchPage('settings'),         // 6 => settings
      'about-ai':      () => Router.switchPage('about'),            // 10 => 6
      'about-changelog': () => Router.switchPage('about'),          // 11 => 6
      'about-benchmark': () => Router.switchPage('about'),            // 12 => 6
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
});

/* ═══════════════════ 对局回放 ═══════════════════ */
// 回放状态（覆盖层模式，不占用路由）
let _rp=null;

/** history 中是否存在可回放的落子数据（mv） */
function hasReplayMoves(history){
  if(!Array.isArray(history)||history.length<2)return false;
  return history.some(h=>h&&h.mv&&((h.mv.x!==undefined&&h.mv.y!==undefined)||(Array.isArray(h.mv)&&h.mv.length>=3)));
}

/**
 * 打开回放覆盖层
 * cfg: {size, maxPlayers, borderMode, colorNames, history}
 * history: [{turn, snapshot, mv:{x,y,player}}]，第 0 条为初始状态（mv=null）
 */
function openReplay(cfg){
  const history=cfg&&cfg.history;
  if(!hasReplayMoves(history)){
    try{showToast('该记录无棋谱数据，无法回放')}catch(e){alert('该记录无棋谱数据，无法回放')}
    return;
  }
  const size=cfg.size||7;
  const maxPlayers=cfg.maxPlayers||2;
  const ov=document.getElementById('replayOverlay');
  _rp={
    size,
    maxPlayers,
    borderMode:cfg.borderMode||'default',
    capMode:cfg.capMode||'4',
    gameCount:cfg.gameCount||0,
    colorNames:cfg.colorNames||null,
    history,
    total:history.length-1,        // 落子步数（第 0 条为初始状态）
    step:0,
    board:mkBoard(size,cfg.capMode||'4',cfg.gameCount||0),
    cells:null,
    curPlayer:0,
    playing:false,
    token:0,
    speed:1,
    ck:new Map(),                  // checkpoint: step -> board 克隆（每 100 步）
  };
  // 构建回放棋盘 DOM
  const bd=document.getElementById('rpBoard');
  bd.replaceChildren();
  bd.style.gridTemplateColumns=`repeat(${size},1fr)`;
  _rp.cells=[];
  for(let i=0;i<size;i++){
    const row=[];
    for(let j=0;j<size;j++){
      const el=document.createElement('div');el.className='cell';
      bd.appendChild(el);row.push(el);
    }
    _rp.cells.push(row);
  }
  // 重置控件
  const slider=document.getElementById('rpSlider');
  slider.min=0;slider.max=_rp.total;slider.value=0;
  // 进度条：拖动中预览步数并立即暂停播放；点击/松开才真正跳转（避免逐帧触发重放）
  // 注意：拖动中不能调用 rpUpdateUI()——它会按 _rp.step 把 slider.value 拉回当前步，覆盖拖动目标
  slider.oninput=function(){
    if(!_rp)return;
    if(_rp.playing){
      _rp.playing=false;
      rpCancel();
      const playBtn=document.getElementById('rpPlayBtn');
      if(playBtn){playBtn.textContent='▶';playBtn.classList.remove('playing');}
    }
    const label=document.getElementById('rpStepLabel');
    if(label)label.textContent=this.value+' / '+_rp.total;
  };
  slider.onchange=function(){rpSeek(this.value)};
  rpBuildTicks();
  rpRenderPlayers();
  rpRenderBoard(true);
  rpUpdateUI();
  ov.style.display='flex';
}
function closeReplay(){
  if(_rp){_rp.playing=false;_rp.token++;}
  _rp=null;
  const ov=document.getElementById('replayOverlay');
  if(ov)ov.style.display='none';
  const bd=document.getElementById('rpBoard');
  if(bd)bd.replaceChildren();
  const pp=document.getElementById('rpPlayers');
  if(pp)pp.innerHTML='';
  const ticks=document.getElementById('rpTicks');
  if(ticks)ticks.innerHTML='';
}
/** 渲染回放棋盘（popX/popY 为落子格，加 pop 动画） */
function rpRenderBoard(force,popX,popY){
  if(!_rp)return;
  const bd=_rp.board;
  for(let i=0;i<_rp.size;i++)for(let j=0;j<_rp.size;j++){
    const el=_rp.cells[i][j],d=bd[i][j];
    el.innerHTML='';
    if(d.owner!==null){
      if(d.owner===_rp.curPlayer){
        const bg=document.createElement('div');bg.className='bg p'+d.owner;
        el.appendChild(bg);
      }
      const p=document.createElement('div');p.className='piece p'+d.owner;
      el.appendChild(p);
      drawDots(p,d.count);
      if(popX!==undefined&&i===popX&&j===popY)p.classList.add('pop');
    }
  }
}
/** 渲染回放玩家条 */
function rpRenderPlayers(){
  const el=document.getElementById('rpPlayers');
  if(!el||!_rp)return;
  el.innerHTML='';
  const cnt={};
  for(let p=0;p<_rp.maxPlayers;p++)cnt[p]=0;
  for(const row of _rp.board)for(const c of row)if(c.owner!==null)cnt[c.owner]++;
  for(let p=0;p<_rp.maxPlayers;p++){
    const t=document.createElement('span');t.className='player-tag';
    if(p===_rp.curPlayer)t.classList.add('active');
    if(cnt[p]===0&&_rp.total>0)t.classList.add('elim');
    t.style.background=COLORS_LIGHT[p];
    t.style.color=COLORS[p];
    const label=(_rp.colorNames&&_rp.colorNames[p])||('玩家 '+(p+1));
    t.textContent=`${label} ${cnt[p]}`;
    el.appendChild(t);
  }
}
/** 构建进度条步数刻度：步数少时逐格显示，多时等分取关键点 */
function rpBuildTicks(){
  const ticks=document.getElementById('rpTicks');
  if(!ticks||!_rp)return;
  ticks.innerHTML='';
  const total=_rp.total;
  let marks;
  if(total<=10){
    marks=[];
    for(let i=0;i<=total;i++)marks.push(i);
  }else{
    marks=[];
    for(let i=0;i<=4;i++)marks.push(Math.round(total*i/4));
  }
  marks=[...new Set(marks)];
  for(const m of marks){
    const d=document.createElement('div');
    d.className='rp-tick';
    d.textContent=m;
    ticks.appendChild(d);
  }
}
/** 更新进度条/步数标签/播放按钮 */
function rpUpdateUI(){
  if(!_rp)return;
  const label=document.getElementById('rpStepLabel');
  if(label)label.textContent=`${_rp.step} / ${_rp.total}`;
  const slider=document.getElementById('rpSlider');
  if(slider&&Number(slider.value)!==_rp.step)slider.value=_rp.step;
  const playBtn=document.getElementById('rpPlayBtn');
  if(playBtn){
    playBtn.textContent=_rp.playing?'⏸':'▶';
    playBtn.classList.toggle('playing',_rp.playing);
  }
}
function rpReadSpeed(){
  const s=document.querySelector('#rpSpeedGroup .gb.selected');
  return s?parseFloat(s.dataset.value)||1:1;
}
function rpSleep(ms){return new Promise(r=>setTimeout(r,ms))}
function rpCancel(){if(_rp)_rp.token++}

/** 从最近 checkpoint（或开头）重建到目标步之前的棋盘，再逐步推进到 target */
async function rpGoTo(target, animate){
  if(!_rp)return;
  const token=_rp.token;
  const hist=_rp.history;
  if(target>_rp.total)target=_rp.total;
  if(target<0)target=0;
  if(target===_rp.step)return;   // 已在目标步：无需操作
  // 后退：从最近 checkpoint 重建（不渲染，待推进到目标后一次渲染）
  if(target<_rp.step){
    let from=0;
    for(const[k]of _rp.ck){if(k<=target&&k>from)from=k;}
    if(from===0){
      _rp.board=mkBoard(_rp.size,_rp.capMode,_rp.gameCount);
      _rp.curPlayer=0;
    }else{
      _rp.board=cloneBoard(_rp.ck.get(from));
      const fromMv=hist[from]&&hist[from].mv;
      _rp.curPlayer=fromMv?(Array.isArray(fromMv)?fromMv[2]:fromMv.player):0;
    }
    _rp.step=from;
    if(token!==_rp.token)return;
  }
  while(_rp.step<target){
    if(token!==_rp.token)return;
    const k=_rp.step+1;
    const rawMv=hist[k]&&hist[k].mv;
    if(!rawMv){_rp.step=k;if(animate){rpRenderBoard();rpRenderPlayers();rpUpdateUI();}continue;}
    const mv=Array.isArray(rawMv)?{x:rawMv[0],y:rawMv[1],player:rawMv[2],seed:rawMv[3]||0}:rawMv;
    try{
      const r=await tauriInvoke('process_move',{
        board:_rp.board,size:_rp.size,x:mv.x,y:mv.y,
        player:mv.player,maxPlayers:_rp.maxPlayers,
        borderMode:_rp.borderMode,
        capMode:_rp.capMode,
        seed:(mv.seed!==undefined&&mv.seed!==null)?mv.seed:undefined
      });
      if(token!==_rp.token)return;
      _rp.board=r.board;
      _rp.curPlayer=mv.player;
      _rp.step=k;
      if(k%100===0)_rp.ck.set(k,cloneBoard(_rp.board));
      if(animate){
        // 恒定无落子动画：逐步渲染棋盘，让播放有过程感
        rpRenderBoard(true);
        rpRenderPlayers();
        rpUpdateUI();
      }
    }catch(e){
      logWarn('Replay move failed at step',k,e);
      break;
    }
    if(animate){
      // 实时读取当前倍速：播放中点击倍速按钮立即生效（按钮只改 DOM 选中态，
      // 不能依赖 rpPlayLoop 启动时快照的 _rp.speed）
      const delay=500/rpReadSpeed();  // 每步停顿：0.5x=1s / 1x=500ms / 2x=250ms / 4x=125ms
      if(delay>0)await rpSleep(delay);
    }
  }
  // 非动画模式（单步/拖动/跳转）：一次渲染最终状态，避免后退时“先跳回 checkpoint 再快速重放”
  if(!animate){
    rpRenderBoard(true);
    rpRenderPlayers();
    rpUpdateUI();
  }
}
/** 重置到 step（先回到开头再推进） */
async function rpResetTo(step, animate){
  if(!_rp)return;
  rpCancel();
  const token=_rp.token;
  _rp.board=mkBoard(_rp.size,_rp.capMode,_rp.gameCount);
  _rp.curPlayer=0;
  _rp.step=0;
  _rp.ck.clear();
  rpRenderBoard(true);
  rpRenderPlayers();
  rpUpdateUI();
  if(step>0)await rpGoTo(step,animate);
}
/** 播放主循环 */
async function rpPlayLoop(){
  if(!_rp)return;
  const token=_rp.token;
  _rp.speed=rpReadSpeed();
  while(_rp&&_rp.playing&&_rp.step<_rp.total){
    if(_rp.token!==token)return;
    await rpGoTo(_rp.step+1,true);
    if(_rp.token!==token)return;
  }
  if(_rp&&_rp.step>=_rp.total){_rp.playing=false;rpUpdateUI();}
}
/** 播放/暂停切换 */
function rpTogglePlay(){
  if(!_rp)return;
  if(_rp.playing){
    _rp.playing=false;
    rpCancel();
    rpUpdateUI();
    return;
  }
  if(_rp.step>=_rp.total){
    // 已到末尾：从开头重播（同步回开始 + 直接播放，避免异步竞态导致棋盘状态错乱）
    _rp.playing=true;
    rpUpdateUI();
    rpCancel();
    rpResetTo(0,false);
    rpPlayLoop();
    return;
  }
  _rp.playing=true;
  rpUpdateUI();
  rpPlayLoop();
}
/** 进度条拖动 */
async function rpSeek(val){
  if(!_rp)return;
  const target=parseInt(val,10)||0;
  _rp.playing=false;
  rpCancel();
  // 前进/后退统一走 rpGoTo（内部按需从 checkpoint 重建，非动画模式只渲染最终状态）
  // 必须 await 完成后再更新 UI：rpGoTo 会先把 _rp.step 设为 checkpoint/0 再逐步推进，
  // 若提前调用 rpUpdateUI()，进度条会被拉到中间值（先跳回先前位置再跳向目标）
  await rpGoTo(target,false);
  rpUpdateUI();
}
