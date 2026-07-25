#!/bin/sh
# show_color.sh — 真彩色终端预览连锁棋玩家颜色
# 用法: ./show_color.sh [dark|light]

set -e

# ── 配色（顺序与 index.html 一致） ──
HEX0="#E74C3C" HEX1="#F1C40F" HEX2="#3498DB" HEX3="#2ECC71" HEX4="#9B59B6"
HEX5="#E91E63" HEX6="#1ABC9C" HEX7="#F39C12" HEX8="#F5ECD7" HEX9="#5D6D7E"

R0=231 G0=76  B0=60
R1=241 G1=196 B1=15
R2=52  G2=152 B2=219
R3=46  G3=204 B3=113
R4=155 G4=89  B4=182
R5=233 G5=30  B5=99
R6=26  G6=188 B6=156
R7=243 G7=156 B7=18
R8=245 G8=236 B8=215
R9=93  G9=109 B9=126

NAME0="红色"   NAME1="黄色"   NAME2="蓝色"   NAME3="绿色"   NAME4="紫色"
NAME5="粉色"   NAME6="青色"   NAME7="橙色"   NAME8="奶白色" NAME9="深灰色"
NUM0="1" NUM1="2" NUM2="3" NUM3="4" NUM4="5"
NUM5="6" NUM6="7" NUM7="8" NUM8="9" NUM9="10"

W=16 H=4 GAP=2

case "${1:-normal}" in
  dark)  BG_R="15" BG_G="15" BG_B="21" ;;
  light) BG_R="243" BG_G="242" BG_B="236" ;;
  *)     BG_R="" ;;
esac

set_bg()    { printf '\033[48;2;%s;%s;%sm' "$1" "$2" "$3"; }
set_fg()    { printf '\033[38;2;%s;%s;%sm' "$1" "$2" "$3"; }
reset()     { printf '\033[0m'; }
set_bg_or() { if [ -n "$BG_R" ]; then set_bg "$BG_R" "$BG_G" "$BG_B"; fi; }
text_color(){ [ $(( ($1*299+$2*587+$3*114)/1000 )) -gt 150 ] && echo "0 0 0" || echo "255 255 255"; }

render_block(){
  local r=$1 g=$2 b=$3 name=$4 num=$5
  local tc=$(text_color $r $g $b)
  for row in $(seq 1 $H); do
    set_bg_or
    if [ "$row" -eq $(( (H+1)/2 )) ]; then
      set_bg $r $g $b; set_fg $tc
      printf "%-*s" "$W" " $num.$name "
      reset
    else
      set_bg $r $g $b; printf '%*s' "$W" ''; reset
    fi
    set_bg_or; printf '%*s' "$GAP" ''; reset
  done
  echo
}

render_hex(){
  set_bg $1 $2 $3; printf " %s " "$4"; reset
  if [ -n "$BG_R" ]; then set_bg "$BG_R" "$BG_G" "$BG_B"; fi
  printf " "
}

echo; echo "  连锁棋 · 10 色玩家配色方案"; echo

render_block $R0 $G0 $B0 "$NAME0" "$NUM0"
render_block $R1 $G1 $B1 "$NAME1" "$NUM1"
render_block $R2 $G2 $B2 "$NAME2" "$NUM2"
render_block $R3 $G3 $B3 "$NAME3" "$NUM3"
render_block $R4 $G4 $B4 "$NAME4" "$NUM4"
echo
render_block $R5 $G5 $B5 "$NAME5" "$NUM5"
render_block $R6 $G6 $B6 "$NAME6" "$NUM6"
render_block $R7 $G7 $B7 "$NAME7" "$NUM7"
render_block $R8 $G8 $B8 "$NAME8" "$NUM8"
render_block $R9 $G9 $B9 "$NAME9" "$NUM9"

echo; set_bg_or; printf "  "
render_hex $R0 $G0 $B0 "$HEX0"
render_hex $R1 $G1 $B1 "$HEX1"
render_hex $R2 $G2 $B2 "$HEX2"
render_hex $R3 $G3 $B3 "$HEX3"
render_hex $R4 $G4 $B4 "$HEX4"
render_hex $R5 $G5 $B5 "$HEX5"
render_hex $R6 $G6 $B6 "$HEX6"
render_hex $R7 $G7 $B7 "$HEX7"
render_hex $R8 $G8 $B8 "$HEX8"
render_hex $R9 $G9 $B9 "$HEX9"
reset; echo; echo

if [ "$TERM" = "xterm" ] || [ "$TERM" = "xterm-256color" ]; then
  if [ "$COLORTERM" != "truecolor" ] && [ "$COLORTERM" != "24bit" ]; then
    echo "  ⚠ 当前终端 ($TERM) 可能不支持真彩色（24-bit）。"
    echo "     若颜色显示异常，请使用支持真彩色的终端。"
    echo
  fi
fi
