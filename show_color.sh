#!/bin/sh
# show_color.sh — 真彩色终端预览连锁棋玩家颜色

set -e

HEX0="#E74C3C" HEX1="#F1C40F" HEX2="#3498DB" HEX3="#2ECC71" HEX4="#9B59B6"
HEX5="#E91E63" HEX6="#1ABC9C" HEX7="#F39C12" HEX8="#8B5E3C" HEX9="#5D6D7E"
R0=231 G0=76  B0=60   R1=241 G1=196 B1=15   R2=52  G2=152 B2=219
R3=46  G3=204 B3=113  R4=155 G4=89  B4=182  R5=233 G5=30  B5=99
R6=26  G6=188 B6=156  R7=243 G7=156 B7=18   R8=139 G8=94  B8=60
R9=93  G9=109 B9=126
NAME0="红色" NAME1="黄色" NAME2="蓝色" NAME3="绿色" NAME4="紫色"
NAME5="粉色" NAME6="青色" NAME7="橙色" NAME8="棕色" NAME9="深灰色"

W=16 H=4 GAP=2
case "${1:-normal}" in dark) BG_R=15 BG_G=15 BG_B=21 ;; light) BG_R=243 BG_G=242 BG_B=236 ;; *) BG_R= ;; esac

set_bg(){ printf '\033[48;2;%s;%s;%sm' "$1" "$2" "$3"; }
set_fg(){ printf '\033[38;2;%s;%s;%sm' "$1" "$2" "$3"; }
reset(){ printf '\033[0m'; }
set_bg_or(){ [ -n "$BG_R" ] && set_bg "$BG_R" "$BG_G" "$BG_B"; }
tc(){ [ $(($1*299+$2*587+$3*114)) -gt 150000 ] && echo "0 0 0" || echo "255 255 255"; }

blk(){ local r=$1 g=$2 b=$3 n=$4 u=$5 t=$(tc $r $g $b)
  for row in $(seq 1 $H); do set_bg_or
    [ "$row" -eq $(( (H+1)/2 )) ] && { set_bg $r $g $b; set_fg $t; printf "%-*s" "$W" " $u.$n "; reset; } || { set_bg $r $g $b; printf '%*s' "$W" ''; reset; }
    set_bg_or; printf '%*s' "$GAP" ''; reset; done; echo; }

hex(){ set_bg $1 $2 $3; printf " %s " "$4"; reset; [ -n "$BG_R" ] && set_bg "$BG_R" "$BG_G" "$BG_B"; printf " "; }

echo; echo "  连锁棋 · 10 色玩家配色方案"; echo
blk $R0 $G0 $B0 "$NAME0" 1; blk $R1 $G1 $B1 "$NAME1" 2; blk $R2 $G2 $B2 "$NAME2" 3
blk $R3 $G3 $B3 "$NAME3" 4; blk $R4 $G4 $B4 "$NAME4" 5; echo
blk $R5 $G5 $B5 "$NAME5" 6; blk $R6 $G6 $B6 "$NAME6" 7; blk $R7 $G7 $B7 "$NAME7" 8
blk $R8 $G8 $B8 "$NAME8" 9; blk $R9 $G9 $B9 "$NAME9" 10; echo

set_bg_or; printf "  "
hex $R0 $G0 $B0 "$HEX0"; hex $R1 $G1 $B1 "$HEX1"; hex $R2 $G2 $B2 "$HEX2"
hex $R3 $G3 $B3 "$HEX3"; hex $R4 $G4 $B4 "$HEX4"; hex $R5 $G5 $B5 "$HEX5"
hex $R6 $G6 $B6 "$HEX6"; hex $R7 $G7 $B7 "$HEX7"; hex $R8 $G8 $B8 "$HEX8"
hex $R9 $G9 $B9 "$HEX9"; reset; echo; echo
