#!/usr/bin/env python3
"""在 index.html 中添加 ML 评估切换 CSS 和 JS"""
path = "/home/ywnh1/Programs/chain-chess/tauri/public/index.html"

with open(path, "r") as f:
    c = f.read()

# 1. 添加 CSS - 在 "/* 算法选择卡片 */" 之前
css = """/* 评估函数切换按钮 */
.toggle-group{display:flex;gap:4px;flex-wrap:wrap}
.toggle-group .tg-btn{
  background:var(--glass-w-04);border:1px solid var(--glass-w-08);
  border-radius:var(--radius-sm);padding:6px 14px;font-size:.8rem;
  color:var(--dim);cursor:pointer;transition:all .15s ease;
}
.toggle-group .tg-btn:hover{background:var(--glass-w-08);color:var(--text)}
.toggle-group .tg-btn.selected{background:rgba(240,179,75,.15);border-color:var(--accent);color:var(--accent)}
.toggle-group .tg-btn:active{transform:scale(.93)}

"""
c = c.replace("/* 算法选择卡片 */", css + "/* 算法选择卡片 */", 1)

# 2. 添加 JS 函数 - 在 "function startAIGame()" 之前
js = """
// 设置 ML/手写评估函数
function setMlEval(enabled){
  const tg=document.getElementById('mlEvalToggle');
  if(tg){tg.querySelectorAll('.tg-btn').forEach(b=>b.classList.toggle('selected',b.dataset.value===String(enabled)))}
  if(window.__TAURI_INTERNALS__){
    window.__TAURI_INTERNALS__.invoke('set_ml_eval',{enabled:enabled}).catch(e=>console.warn('set_ml_eval:',e))
  }
}

"""
c = c.replace("\nfunction startAIGame(){", js + "\nfunction startAIGame(){", 1)

with open(path, "w") as f:
    f.write(c)

print("✅ CSS + JS 已添加")
