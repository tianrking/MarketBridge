"use strict";
const $ = id => document.getElementById(id);
let lastResult = null, nextCursor = 0, polling = false;
function message(text, error=false) { $("message").textContent=text; $("message").className=error?"error":""; }
function show(id, value) { lastResult=value; $(id).textContent=JSON.stringify(value,null,2); }
async function api(path, body) {
  const headers={"Accept":"application/json"}; if($("key").value) headers["x-api-key"]=$("key").value;
  if(body!==undefined) headers["Content-Type"]="application/json";
  const controller=new AbortController(), timeout=setTimeout(()=>controller.abort(),15000);
  try {
    const response=await fetch(path,{method:body===undefined?"GET":"POST",headers,body:body===undefined?undefined:JSON.stringify(body),redirect:"error",signal:controller.signal,cache:"no-store"});
    const text=await response.text(); if(text.length>8*1024*1024) throw Error("响应超过工作台显示限制");
    let result; try {result=JSON.parse(text);} catch {throw Error(`HTTP ${response.status}: 返回非 JSON 内容`);}
    if(!response.ok) throw Error(`HTTP ${response.status}: ${JSON.stringify(result)}`);
    return result;
  } finally {clearTimeout(timeout);}
}
function bind(id, action) {$(id).addEventListener("click",async()=>{const button=$(id);button.disabled=true;try{await action();message("操作完成；请检查返回状态与证据限制。");}catch(error){message(error.message,true);}finally{button.disabled=false;}});}
for(const button of document.querySelectorAll("nav button")) button.addEventListener("click",()=>{for(const b of document.querySelectorAll("nav button"))b.setAttribute("aria-selected",String(b===button));for(const section of document.querySelectorAll("main>.panel"))section.hidden=section.id!==button.dataset.tab;});
bind("connect",async()=>{try{const info=await api("/v1/system/info");$("connection").textContent=`已连接 · ${info.version} · 不下单`;show("observation",info);}catch(e){$("connection").textContent="连接失败 / 检查 API Key";throw e;}});
async function refresh(){const path=$("view").value;try{const result=await api(path);show("observation",result);$("observedAt").textContent=`查询完成 ${new Date().toISOString()} · ${path} · 此时间不是源数据时间`;}catch(e){$("observedAt").textContent=`查询失败 ${new Date().toISOString()}；下方为之前结果，可能已过期`;throw e;}}
bind("refresh",refresh);
setInterval(async()=>{if(!$("poll").checked||polling||document.hidden)return;polling=true;try{await refresh();}catch(e){$("poll").checked=false;message(`${e.message}；自动刷新已停止`,true);}finally{polling=false;}},5000);
async function refreshSqueeze(){
  const minimumScore=Number($("squeezeMinScore").value), limit=Number($("squeezeLimit").value);
  if(!Number.isInteger(minimumScore)||minimumScore<0||minimumScore>10||!Number.isInteger(limit)||limit<1||limit>100) throw Error("扫描参数必须在允许范围内");
  const result=await api(`/v1/research/squeeze/scan?exchange=binance&minimum_score=${minimumScore}&limit=${limit}&max_data_age_ms=15000`);
  const rows=$("squeezeRows"); rows.textContent="";
  const stateFilter=$("squeezeState").value;
  for(const candidate of result.candidates||[]){
    if(stateFilter!=="all"&&candidate.state!==stateFilter) continue;
    const row=document.createElement("tr");
    const values=[candidate.symbol,candidate.state,`${candidate.score}/${candidate.max_score}`,candidate.data_quality?.fresh?"新鲜":"过期",(candidate.evidence||[]).join("；")||"—"];
    for(const value of values){const cell=document.createElement("td");cell.textContent=String(value);row.appendChild(cell);} rows.appendChild(row);
  }
  if(!rows.children.length){const row=document.createElement("tr"),cell=document.createElement("td");cell.colSpan=5;cell.className="empty";cell.textContent="当前没有满足评分或新鲜度条件的 Binance 候选";row.appendChild(cell);rows.appendChild(row);}
  $("squeezeObservedAt").textContent=`扫描完成 ${new Date().toISOString()} · 已观测 ${result.observed_symbols??0} 个状态`;
  show("squeezeResult",result);
}
bind("squeezeRefresh",refreshSqueeze);
setInterval(async()=>{if(!$("squeezePoll").checked||document.hidden)return;try{await refreshSqueeze();}catch(error){message(`${error.message}；做空研究自动刷新已停止`,true);$("squeezePoll").checked=false;}},5000);
async function refreshFacts(){
  const symbol=$("factsSymbol").value.trim().toUpperCase(), exchange=$("factsExchange").value;
  if(!/^[A-Z0-9._-]{2,30}$/.test(symbol)) throw Error("Symbol 格式无效");
  const result=await api(`/v1/research/symbol-state?symbol=${encodeURIComponent(symbol)}&exchange=${encodeURIComponent(exchange)}`);
  show("factsResult",result); $("factsObservedAt").textContent=`查询完成 ${new Date().toISOString()} · ${exchange}:${symbol}`;
}
bind("factsRefresh",refreshFacts);
setInterval(async()=>{if(!$("factsPoll").checked||document.hidden)return;try{await refreshFacts();}catch(error){message(`${error.message}；事实指标自动刷新已停止`,true);$("factsPoll").checked=false;}},5000);
const runId=()=>`ui-${Date.now()}-${crypto.randomUUID().slice(0,8)}`;
$("runid").value=runId();
bind("template",async()=>{const example=await api("/workbench/example.json");const model=$("model").value;let value;
  if(model==="same-asset-spot/v1")value=example;
  else if(model==="scenario-replay/v1")value={frames:[example]};
  else if(model==="candidate-screen/v1")value={as_of_ms:example.as_of_ms,min_net_bps:0,candidates:[{id:"synthetic",evidence:example}]};
  else if(model==="prefunded-taker-scenario/v1")value={initial:{buy_venue_quote:1000000,buy_venue_base:0,sell_venue_quote:0,sell_venue_base:10},frames:[{evidence:example,size_index:0,buy_fill_fraction:1,sell_fill_fraction:1}]};
  else if(model==="allocated-spot-portfolio/v1")value={accounts:[{instrument:example.buy.instrument,quote:1000000,base:0},{instrument:example.sell.instrument,quote:0,base:10}],steps:[{id:"entry",purpose:"entry",gate:{kind:"always"},evidence:example,size_index:0,buy_fill_fraction:1,sell_fill_fraction:1}]};
  else throw Error("该模型的单位与输入需明确指定，请按 docs/user-guide/02-models.md 导入场景，不自动猜测合约。");
  $("input").value=JSON.stringify(value,null,2);$("runid").value=runId();
});
bind("run",async()=>{const result=await api("/v1/research/workspace",{action:"run",request:{id:$("runid").value,model:$("model").value,input:JSON.parse($("input").value)}});show("experimentResult",result);if(result.payload?.output?.status==="failed")throw Error("实验失败并已归档，详见返回 error；这不是成功结果。");});
$("import").addEventListener("change",async event=>{try{const file=event.target.files[0];if(!file)return;if(file.size>2*1024*1024)throw Error("输入文件超过 2 MiB");$("input").value=JSON.stringify(JSON.parse(await file.text()),null,2);message("已载入本地 JSON，尚未执行。");}catch(e){message(e.message,true);}});
async function list(){const cursor=Number($("cursor").value);if(!Number.isSafeInteger(cursor)||cursor<0)throw Error("游标必须是非负整数");const result=await api("/v1/research/workspace",{action:"list",request:{namespace:$("namespace").value,after_sequence:cursor,limit:20}});show("archiveResult",result);nextCursor=result.next_sequence;}
bind("list",list);bind("next",async()=>{$("cursor").value=nextCursor;await list();});
$("namespace").addEventListener("change",()=>{nextCursor=0;$("cursor").value=0;});
bind("integrity",async()=>show("archiveResult",await api("/v1/research/workspace",{action:"integrity"})));
bind("submitAction",async()=>show("archiveResult",await api("/v1/research/workspace",JSON.parse($("action").value))));
bind("loadConfig",async()=>{const result=await api("/v1/research/control");$("config").value=JSON.stringify(result.config,null,2);show("controlResult",result);});
bind("applyConfig",async()=>show("controlResult",await api("/v1/research/control",JSON.parse($("config").value))));
bind("disable",async()=>{const status=await api("/v1/research/control");const config={...status.config,revision:runId(),enabled:false};show("controlResult",await api("/v1/research/control",config));$("config").value=JSON.stringify(config,null,2);});
bind("export",async()=>{if(lastResult===null)throw Error("还没有可导出的 API 结果");const url=URL.createObjectURL(new Blob([JSON.stringify(lastResult,null,2)],{type:"application/json"}));const a=document.createElement("a");a.href=url;a.download=`marketbridge-${Date.now()}.json`;a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);});
