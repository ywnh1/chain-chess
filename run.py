#!/usr/bin/env python3
"""守护脚本 — 保持服务器持久运行"""
import subprocess
import time
import os
import sys

BASE = os.path.dirname(os.path.abspath(__file__))
LOG = os.path.join(BASE, "logs", "server.log")

def start():
    with open(LOG, "a") as f:
        f.write(f"\n[{time.strftime('%H:%M:%S')}] 🚀 启动服务器\n")
    proc = subprocess.Popen(
        [sys.executable, os.path.join(BASE, "main.py")],
        stdout=open(LOG, "a"), stderr=subprocess.STDOUT,
        cwd=BASE
    )
    return proc

if __name__ == "__main__":
    os.makedirs(os.path.join(BASE, "logs"), exist_ok=True)
    proc = start()
    while True:
        time.sleep(5)
        if proc.poll() is not None:
            print(f"[{time.strftime('%H:%M:%S')}] 服务器挂了 (code {proc.returncode}), 重启中...")
            proc = start()
