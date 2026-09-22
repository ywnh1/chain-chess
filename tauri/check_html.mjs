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
console.log("\n"+h.split("\n").length+" lines, "+kb+"KB");
if(e.length){console.log("ERRORS:");e.forEach(x=>console.log(" - "+x))}
if(w.length){console.log("WARNINGS:");w.forEach(x=>console.log(" - "+x))}
if(!e.length&&!w.length)console.log("ALL GOOD");
process.exit(e.length?1:0);
