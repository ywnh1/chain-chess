#!/usr/bin/env node
import{readFileSync,existsSync}from"fs";import{resolve}from"path";
const f=resolve(import.meta.dirname,"public/index.html");
if(!existsSync(f)){console.error("NO FILE");process.exit(1)}
const h=readFileSync(f,"utf-8");
let e=[],w=[];
if(!h.startsWith("<!DOCTYPE html>"))e.push("no DOCTYPE");
const ids={};
for(const m of h.matchAll(/id="([^"]+)"/g))ids[m[1]]=(ids[m[1]]||0)+1;
for(const[id,c]of Object.entries(ids))if(c>1)e.push("dup id");
const kb=(Buffer.byteLength(h,"utf-8")/1024).toFixed(1);
// 引用一致性：index.html 里的 openHint('key') 与 screen id 必须在 app.js 中有定义
const appJs=readFileSync(resolve(import.meta.dirname,"public/app.js"),"utf-8");
const hintsBlock=appJs.slice(appJs.indexOf("const HINTS = {"),appJs.indexOf("/** 打开说明弹窗"));
const hintKeys=new Set([...hintsBlock.matchAll(/'([a-z-]+)': \{\n    title:/g)].map(m=>m[1]));
for(const m of h.matchAll(/openHint\('([a-z-]+)'\)/g))if(!hintKeys.has(m[1]))e.push("unknown hint key: "+m[1]);
for(const m of h.matchAll(/<div id="([a-z-]+)" class="screen"/g))if(!appJs.includes(`Router.register('${m[1]}'`))e.push("screen not registered: "+m[1]);
// 取值辅助别用错：getSel 是数字版（parseInt），字符串容器必须用 getSelStr。
// 这个错犯过一次：currentShapeMode 用了 getSel，点「长方形/自定义」永远退回 square。
const strContainers=["shapeModeGroup","borderModeGroup","capModeGroup","soundThemeGroup","dogBarkGroup"];
for(const c of strContainers)
  if(new RegExp("getSel\\('"+c+"'\\)").test(appJs))e.push("getSel('"+c+"') 用错了：字符串容器要用 getSelStr");
// 长方形棋盘：容器比例要跟着行列走。曾经锁死 1:1，格子被拉变形。
// 这里直接把函数抽出来跑一遍，看比例对不对——光看代码看不出来。
{
  const m=appJs.match(/function applyBoardAspect\([\s\S]*?\n\}/);
  if(!m)e.push("applyBoardAspect 不见了");
  else{
    const fn=new Function("window","return "+m[0])({innerWidth:400,innerHeight:800});
    const w1={style:{}};fn(w1,7,5,440);
    const a=parseFloat(w1.style.width),b=parseFloat(w1.style.height);
    if(!(a>0&&b>0))e.push("applyBoardAspect 没算出宽高");
    else if(Math.abs(a/b-5/7)>0.01)e.push("7行×5列 的容器比例应是 5/7，实际 "+(a/b).toFixed(3));
    const w2={style:{}};fn(w2,7,7,440);
    const c2=parseFloat(w2.style.width),d2=parseFloat(w2.style.height);
    if(!(c2>0&&d2>0))e.push("applyBoardAspect 正方形算不出宽高");
    else if(Math.abs(c2/d2-1)>0.01)e.push("正方形棋盘比例应当是 1:1，实际 "+(c2/d2).toFixed(3));
  }
  if(!/applyBoardAspect\(bd\.parentElement/.test(appJs))e.push("renderBoard 没调用 applyBoardAspect（长方形会被拉变形）");
}
console.log("\n"+h.split("\n").length+" lines, "+kb+"KB");
if(e.length){console.log("ERRORS:");e.forEach(x=>console.log(" - "+x))}
if(w.length){console.log("WARNINGS:");w.forEach(x=>console.log(" - "+x))}
if(!e.length&&!w.length)console.log("ALL GOOD");
process.exit(e.length?1:0);
