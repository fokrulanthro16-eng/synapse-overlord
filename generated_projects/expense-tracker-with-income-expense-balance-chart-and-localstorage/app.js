
let txs=JSON.parse(localStorage.getItem('exp-txs')||'[]');
let txType='income',txFilter='all';
if(!txs.length){
  txs=[
    {id:1,desc:'Monthly Salary',   type:'income', cat:'Salary',        amt:3500,date:'2025-05-01'},
    {id:2,desc:'Grocery Shopping', type:'expense',cat:'Food',          amt:120, date:'2025-05-03'},
    {id:3,desc:'Uber Rides',       type:'expense',cat:'Transport',     amt:45,  date:'2025-05-05'},
    {id:4,desc:'Freelance Project',type:'income', cat:'Freelance',     amt:800, date:'2025-05-08'},
    {id:5,desc:'Netflix',          type:'expense',cat:'Entertainment', amt:15,  date:'2025-05-10'},
    {id:6,desc:'Doctor Visit',     type:'expense',cat:'Health',        amt:60,  date:'2025-05-12'},
    {id:7,desc:'Amazon Shopping',  type:'expense',cat:'Shopping',      amt:95,  date:'2025-05-13'},
    {id:8,desc:'Coffee & Lunch',   type:'expense',cat:'Food',          amt:38,  date:'2025-05-14'},
  ];
  localStorage.setItem('exp-txs',JSON.stringify(txs));
}
function setType(t,el){txType=t;document.querySelectorAll('[id^="type-"]').forEach(b=>b.classList.remove('active'));el.classList.add('active');}
function setTxFilter(f,el){txFilter=f;document.querySelectorAll('[id^="tf-"]').forEach(b=>b.classList.remove('active'));el.classList.add('active');renderAll();}
function addTx(){
  const desc=document.getElementById('tx-desc').value.trim();
  const amt=parseFloat(document.getElementById('tx-amount').value);
  const cat=document.getElementById('tx-cat').value;
  const date=document.getElementById('tx-date').value||new Date().toISOString().slice(0,10);
  if(!desc||!amt||amt<=0)return;
  txs.unshift({id:Date.now(),desc,type:txType,cat,amt,date});
  localStorage.setItem('exp-txs',JSON.stringify(txs));
  document.getElementById('tx-desc').value='';document.getElementById('tx-amount').value='';
  renderAll();
}
function delTx(id){txs=txs.filter(t=>t.id!==id);localStorage.setItem('exp-txs',JSON.stringify(txs));renderAll();}
function getFiltered(){
  const mo=document.getElementById('month-filter').value;
  let f=txs;
  if(mo!=='all')f=f.filter(t=>t.date.startsWith(mo));
  if(txFilter!=='all')f=f.filter(t=>t.type===txFilter);
  return f;
}
function renderAll(){
  const mo=document.getElementById('month-filter').value;
  let base=txs;if(mo!=='all')base=base.filter(t=>t.date.startsWith(mo));
  const income=base.filter(t=>t.type==='income').reduce((s,t)=>s+t.amt,0);
  const expense=base.filter(t=>t.type==='expense').reduce((s,t)=>s+t.amt,0);
  document.getElementById('total-income').textContent='$'+income.toFixed(2);
  document.getElementById('total-expense').textContent='$'+expense.toFixed(2);
  const bal=income-expense;
  const balEl=document.getElementById('balance');
  balEl.textContent='$'+Math.abs(bal).toFixed(2)+(bal<0?' (deficit)':'');
  balEl.style.color=bal>=0?'#10b981':'#ef4444';
  document.getElementById('tx-count').textContent=base.length;
  const cats={};
  base.filter(t=>t.type==='expense').forEach(t=>{cats[t.cat]=(cats[t.cat]||0)+t.amt;});
  const maxCat=Math.max(...Object.values(cats),1);
  const catIco={Food:'🍔',Transport:'🚗',Shopping:'🛍️',Health:'💊',Entertainment:'🎬',Salary:'💼',Freelance:'💻',Other:'📌'};
  document.getElementById('cat-chart').innerHTML=Object.entries(cats).sort((a,b)=>b[1]-a[1]).map(([c,v])=>`
<div style="margin-bottom:10px">
  <div style="display:flex;justify-content:space-between;font-size:13px;margin-bottom:3px"><span>${catIco[c]||'📌'} ${c}</span><span style="font-weight:600">$${v.toFixed(2)}</span></div>
  <div style="background:#e2e8f0;border-radius:4px;height:8px"><div style="background:#6366f1;height:100%;border-radius:4px;width:${Math.round((v/maxCat)*100)}%;transition:width .4s"></div></div>
</div>`).join('')||'<p style="color:#94a3b8;font-size:13px">No expense data for this period.</p>';
  const rows=getFiltered();
  document.getElementById('tx-list').innerHTML=rows.length?rows.map(t=>`
<div style="display:flex;align-items:center;gap:12px;padding:12px 0;border-bottom:1px solid #f1f5f9">
  <div style="width:36px;height:36px;border-radius:50%;background:${t.type==='income'?'#dcfce7':'#fee2e2'};display:flex;align-items:center;justify-content:center;font-size:18px;flex-shrink:0">${t.type==='income'?'📈':'📉'}</div>
  <div style="flex:1"><div style="font-size:14px;font-weight:600">${t.desc}</div><div style="font-size:11px;color:#94a3b8">${t.cat} · ${t.date}</div></div>
  <div style="font-size:16px;font-weight:700;color:${t.type==='income'?'#10b981':'#ef4444'}">${t.type==='income'?'+':'-'}$${t.amt.toFixed(2)}</div>
  <button onclick="delTx(${t.id})" style="background:none;border:none;cursor:pointer;color:#ef4444;font-size:16px">🗑</button>
</div>`).join(''):'<p style="text-align:center;color:#94a3b8;padding:24px 0">No transactions found.</p>';
}
document.getElementById('tx-date').valueAsDate=new Date();
renderAll();
