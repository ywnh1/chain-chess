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
// Gitee API 返回的 content 是 base64；atob 解出来是字节串，中文必须再按 UTF-8
// 解一次，否则更新弹窗里的更新说明全是乱码（曾如此）。
if(/JSON\.parse\(atob\(/.test(h)) e.push("index.html 还在用裸 atob 解 base64：更新弹窗会乱码");
if(!/function b64ToUtf8/.test(h)) e.push("index.html 缺少 b64ToUtf8");
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
// 形状切换的显示逻辑：三种模式各该显示哪些控件。把函数抽出来真跑一遍，
// 光看代码看不出漏分支（自定义曾误显示普通尺寸按钮，初始化也曾误跳编辑器）。
{
  const m=appJs.match(/function applyShapeMode\([\s\S]*?\n\}/);
  if(!m)e.push("applyShapeMode 不见了");
  else{
    const run=(mode,jump)=>{
      let navigated=null;
      const mk=()=>({style:{}});
      const els={squareSizeRow:mk(),rectSizeRow:mk()};
      const fn=new Function("currentShapeMode","document","Router","setupLobbySync","return "+m[0])(
        ()=>mode,
        {getElementById:id=>els[id]||null},
        {navigate:id=>{navigated=id;}},
        ()=>{}
      );
      fn(jump);
      return {sq:els.squareSizeRow.style.display,rc:els.rectSizeRow.style.display,nav:navigated};
    };
    const a=run('square',false);
    if(a.sq==='none')e.push("正方形模式不该隐藏尺寸按钮");
    if(a.rc!=='none')e.push("正方形模式不该显示长宽选择器");
    const b=run('rect',false);
    if(b.sq!=='none')e.push("长方形模式应隐藏正方形尺寸按钮");
    if(b.rc==='none')e.push("长方形模式应显示长宽选择器");
    const c=run('custom',false);
    if(c.sq!=='none')e.push("自定义模式应隐藏正方形尺寸按钮（这里出过 bug）");
    if(c.rc!=='none')e.push("自定义模式不该显示长宽选择器");
    if(run('custom',true).nav!=='board-editor')e.push("用户点自定义时该跳形状编辑器");
    if(run('custom',false).nav)e.push("初始化时不该跳编辑器（会退不出来）");
  }
}
// 空洞不是棋盘格子：processClick 在引擎结果返回前就会直接改棋盘，
// 前端得自己挡一道 —— 这里出过「AI 把子下进洞里」（棋子先被画上去，
// 等 result.board 覆盖回来时已经闪过一瞬）。
{
  const m=appJs.match(/async function processClick\([\s\S]*?\n\}/);
  if(!m)e.push("processClick 不见了");
  else if(!/if\(!c\|\|c\.blocked\)return \[\]/.test(m[0]))
    e.push("processClick 没挡 blocked：棋子会被画进洞里");
}
// 不规则棋盘只支持默认边界 —— 其余选项要禁用
if(!/customShape&&b\.dataset\.value!=='default'/.test(appJs))
  e.push("updateModeConflictUI 没处理不规则棋盘的边界禁用");
// 棋盘副本必须带上 blocked，否则悔棋后洞就没了。只盯 cloneBoard 的返回，
// _boardCache 那种只用于渲染对比的缓存不需要（它本来就只看 owner/count）。
if(/return b\.map\(row=>row\.map\(c=>\(\{owner:c\.owner,count:c\.count\}\)\)\)/.test(appJs))
  e.push("cloneBoard 丢了 blocked：悔棋后不规则棋盘会变回普通棋盘");
// 回放入口必须传 cols：少了它，长方形回放会按正方形重建，坐标直接错乱
for(const m of appJs.matchAll(/openReplay\(\{([\s\S]*?)\n      \}\)/g))
  if(!/cols:/.test(m[1]))e.push("openReplay 调用没传 cols");
// 外观设置：设置页要有两个开关，app.js 要真正消费它们，CSS 要能藏住光球。
// 这几条都是「加过又忘了接上」的高危点：按钮渲染出来但点击没反应，最难发现。
for(const id of ["orbsGroup","bgFollowGroup"])
  if(!h.includes(`id="${id}"`))e.push("设置页缺少外观开关: "+id);
for(const fn of ["applyOrbs","bindAppearanceGroup","applyBgFollow"])
  if(!appJs.includes("function "+fn+"("))e.push("app.js 缺少 function "+fn+"：外观开关会失效");
if(!/appSettings\.bgFollow===false/.test(appJs))
  e.push("setBg 没挡 bgFollow：关掉背景跟随后仍会随玩家变色");
if(!appJs.includes("orbs-on"))
  e.push("app.js 没切换 orbs-on：光球开关无效");
{
  const css=readFileSync(resolve(import.meta.dirname,"public/style.css"),"utf-8");
  if(!/body\.orbs-on \.orb-container\{display:block\}/.test(css))
    e.push("style.css 缺少 .orbs-on 规则：光球不会显示");
}
console.log("\n"+h.split("\n").length+" lines, "+kb+"KB");
if(e.length){console.log("ERRORS:");e.forEach(x=>console.log(" - "+x))}
if(w.length){console.log("WARNINGS:");w.forEach(x=>console.log(" - "+x))}
if(!e.length&&!w.length)console.log("ALL GOOD");
process.exit(e.length?1:0);
