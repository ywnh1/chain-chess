#!/bin/sh
# 连锁棋 - 启动/停止脚本
# ./start.sh       → 启动
# ./start.sh stop  → 停止

cd "$(dirname "$0")"
mkdir -p logs

# ════════════════════ 停止 ════════════════════
stop_server() {
  echo "♟ 停止服务..."
  kill "$(cat logs/server.pid 2>/dev/null)" 2>/dev/null  || true
  kill "$(cat logs/ngrok.pid 2>/dev/null)"  2>/dev/null  || true
  fuser -k 8000/tcp 2>/dev/null                         || true
  ps aux 2>/dev/null | grep -E "main\.py|run\.py" | grep -v grep \
    | awk '{print $2}' | xargs kill 2>/dev/null          || true
  rm -f logs/server.pid logs/ngrok.pid
  echo "♟ 连锁棋 已停止"
}

[ "${1:-}" = "stop" ] && { stop_server; exit 0; }

# ════════════════════ 清理 ════════════════════
echo "♟ 清理旧进程..."
fuser -k 8000/tcp 2>/dev/null                          || true
ps aux 2>/dev/null | grep -E "main\.py|run\.py" | grep -v grep \
  | awk '{print $2}' | xargs kill 2>/dev/null           || true
sleep 1
rm -f logs/server.pid logs/ngrok.pid
echo "   ✅"

# ════════════════════ 检查 venv ════════════════════
VENV_PY="./venv/bin/python3"
if [ ! -f "$VENV_PY" ]; then
  echo "❌ 未找到 $VENV_PY，请先创建虚拟环境:"
  echo "   python3 -m venv venv"
  exit 1
fi

# 强制 venv 路径优先（解决 Debian 系统 dist-packages 干扰）
VENV_SITE=$($VENV_PY -c "import site; print(site.getsitepackages()[0])" 2>/dev/null)
if [ -n "$VENV_SITE" ]; then
  export PYTHONPATH="$VENV_SITE${PYTHONPATH:+:$PYTHONPATH}"
fi

# ════════════════════ 依赖检查 ════════════════════
echo "♟ 检查依赖..."

# 用临时文件捕获 stderr，让用户看到真正的错误
$VENV_PY -c "
import os, sys
print('  ── 环境诊断 ──')
print(f'  PYTHONPATH={os.environ.get(\"PYTHONPATH\",\"\")}')
for i, p in enumerate(sys.path):
    label = ''
    if 'dist-packages' in p and 'venv' not in p: label = ' ← 系统路径!'
    if 'venv' in p: label = ' ← venv'
    if p.endswith('site-packages'): label = ' ← venv site-packages'
    print(f'  path[{i}]={p}{label}')
print('  ── 导入测试 ──')
import typing_extensions
print(f'  typing_extensions: {typing_extensions.__file__.replace(os.environ[\"HOME\"],\"~\")}')
from typing_extensions import Sentinel
print('  Sentinel: ✅')
from fastapi import FastAPI
print('  FastAPI: ✅')
import uvicorn
print('  Uvicorn: ✅')
import websockets
print('  WebSockets: ✅')
print('  ——— 全部通过 ✅')
" >logs/check.log 2>&1

if [ $? -eq 0 ]; then
  cat logs/check.log
else
  echo "  ❌ 依赖异常！错误信息:"
  sed 's/^/    /' logs/check.log
  echo ""
  echo "  尝试自动修复..."
  # 方案1：强制重装（跳过缓存，从清华源下载）
  $VENV_PY -m pip install --force-reinstall --no-cache-dir --quiet \
    typing_extensions pydantic pydantic-core fastapi uvicorn websockets 2>&1 | tail -5
  sleep 1
  echo "  修复后重新检查..."
  $VENV_PY -c "
import typing_extensions
print(typing_extensions.__file__)
from typing_extensions import Sentinel
from fastapi import FastAPI
import uvicorn
import websockets
" >logs/check.log 2>&1
  if [ $? -ne 0 ]; then
    cat logs/check.log
    echo ""
    echo "  方案2：单独强制重装 typing_extensions..."
    $VENV_PY -m pip install --force-reinstall --no-cache-dir \
      "typing_extensions>=4.16" 2>&1 | tail -5
    sleep 1
    $VENV_PY -c "
import typing_extensions
print(typing_extensions.__file__)
from typing_extensions import Sentinel
" >logs/check.log 2>&1
    if [ $? -ne 0 ]; then
      cat logs/check.log
      echo ""
      echo "  ❌ 自动修复均失败，请手动执行以下诊断:"
      echo "     cd $(pwd)"
      echo "     $VENV_PY -c \"import typing_extensions; print(typing_extensions.__file__); from typing_extensions import Sentinel\""
      echo ""
      echo "  如果报错 ImportError: cannot import name 'Sentinel'，原因是系统路径"
      echo "  /usr/lib/python3/dist-packages/ 下的旧版 typing_extensions 优先于 venv。"
      echo "  解决办法（任选其一）:"
      echo "   (A) 移除系统旧版: sudo mv /usr/lib/python3/dist-packages/typing_extensions.py{,.bak}"
      echo "   (B) 升级系统版:   sudo pip3 install -U typing_extensions"
      echo "   (C) 换用绝对路径: PYTHONPATH=$VENV_PY/../lib/python3.13/site-packages ./start.sh"
      exit 1
    fi
  fi
  echo "  修复完成 ✅"
fi

# ════════════════════ 启动 ════════════════════
echo "♟ 启动服务器..."
: > logs/server.log
: > logs/daemon.log
: > log.log

$VENV_PY run.py > logs/daemon.log 2>&1 &
PID=$!
echo "$PID" > logs/server.pid

# 轮询等待就绪（最多 8 秒）
READY=
for i in 1 2 3 4; do
  sleep 2
  if $VENV_PY -c "
import urllib.request
urllib.request.urlopen('http://127.0.0.1:8000/', timeout=2)
" 2>/dev/null; then
    READY=1
    break
  fi
done

echo ""
if [ -n "$READY" ]; then
  echo "┌─────────────────────────────────────────┐"
  echo "│  ♟ 连锁棋  服务已就绪 ✓                  │"
  echo "│                                         │"
  echo "│  本地: http://127.0.0.1:8000             │"
  echo "│  停止: ./start.sh stop                   │"
  echo "└─────────────────────────────────────────┘"
else
  cat logs/server.log 2>/dev/null | tail -20
  echo ""
  echo "⚠️  服务未就绪，日志如上"
  echo "   查看完整日志:  tail -f logs/server.log"
  echo "   查看守护日志:  tail -f logs/daemon.log"
fi

# ─── ngrok（可选） ───
if command -v ngrok >/dev/null 2>&1; then
  sleep 2
  ngrok http 8000 --log=stdout > logs/ngrok.log 2>&1 &
  echo $! > logs/ngrok.pid
  sleep 4
  URL=$(curl -s http://127.0.0.1:4040/api/tunnels 2>/dev/null \
    | $VENV_PY -c "
import sys,json
try:
    for t in json.load(sys.stdin)['tunnels']:
        u=t.get('public_url','')
        if u: print(u)
except: pass
" 2>/dev/null)
  [ -n "$URL" ] && echo "  外网: $URL"
fi
