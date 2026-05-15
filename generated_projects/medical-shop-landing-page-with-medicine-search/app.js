
const P=[
  {id:1,n:'Paracetamol 500mg',s:'Tablets · 20pk',p:4.99,c:'tablets',i:'💊',a:'ok'},
  {id:2,n:'Amoxicillin 250mg',s:'Capsules · 10pk',p:8.50,c:'tablets',i:'💊',a:'ok'},
  {id:3,n:'Cetirizine 10mg', s:'Tablets · 30pk',p:6.25,c:'tablets',i:'💊',a:'low'},
  {id:4,n:'Vitamin D3 1000IU',s:'Softgels · 60pk',p:12.99,c:'vitamins',i:'⚡',a:'ok'},
  {id:5,n:'Vitamin C 1000mg', s:'Tablets · 30pk',p:11.50,c:'vitamins',i:'⚡',a:'ok'},
  {id:6,n:'Omega-3 Fish Oil',  s:'Capsules · 90pk',p:18.99,c:'vitamins',i:'⚡',a:'ok'},
  {id:7,n:'Cough Syrup 100ml', s:'Syrup · 100ml',  p:7.75,c:'syrups', i:'🍯',a:'ok'},
  {id:8,n:'Antacid 200ml',     s:'Syrup · 200ml',  p:6.99,c:'syrups', i:'🍯',a:'low'},
  {id:9,n:'Bandage Roll 5m',   s:'First Aid · 1pc',p:3.25,c:'firstaid',i:'🩹',a:'ok'},
  {id:10,n:'First Aid Kit',    s:'Complete · 25pc',p:24.99,c:'firstaid',i:'🩹',a:'ok'},
];
let cat='all', q='', cart=JSON.parse(localStorage.getItem('med-cart')||'[]');
function filterProds(){q=document.getElementById('search').value.toLowerCase();render();}
function setCat(c,el){cat=c;document.querySelectorAll('.cat').forEach(b=>b.classList.remove('active'));el.classList.add('active');render();}
function render(){
  const f=P.filter(p=>(cat==='all'||p.c===cat)&&(p.n.toLowerCase().includes(q)||p.s.toLowerCase().includes(q)));
  document.getElementById('pcnt').textContent=f.length+' items';
  document.getElementById('grid').innerHTML=f.map(p=>`
<div class="prod-card">
  <div class="prod-icon">${p.i}</div>
  <div class="prod-name">${p.n}</div>
  <div class="prod-sub">${p.s}</div>
  <span class="stk ${p.a}">${p.a==='ok'?'In Stock':p.a==='low'?'Low Stock':'Out of Stock'}</span>
  <div class="prod-price">$${p.p.toFixed(2)}</div>
  <button class="add-btn" onclick="addCart(${p.id})" ${p.a==='no'?'disabled':''}>+ Add to Cart</button>
</div>`).join('');
}
function addCart(id){
  const p=P.find(x=>x.id===id); if(!p) return;
  const ex=cart.find(x=>x.id===id);
  if(ex) ex.qty++; else cart.push({...p,qty:1});
  save(); updateCart();
}
function changeQty(id,d){
  const i=cart.findIndex(x=>x.id===id); if(i<0) return;
  cart[i].qty+=d; if(cart[i].qty<=0) cart.splice(i,1);
  save(); updateCart();
}
function save(){localStorage.setItem('med-cart',JSON.stringify(cart));}
function updateCart(){
  document.getElementById('cnt').textContent=cart.reduce((s,x)=>s+x.qty,0);
  document.getElementById('ctotal').textContent='$'+cart.reduce((s,x)=>s+x.p*x.qty,0).toFixed(2);
  document.getElementById('citems').innerHTML=cart.length
    ? cart.map(x=>`<div class="citem"><div style="flex:1"><div class="cname">${x.n}</div><div class="qbtns"><button class="qb" onclick="changeQty(${x.id},-1)">−</button><span style="font-size:13px;min-width:20px;text-align:center">${x.qty}</span><button class="qb" onclick="changeQty(${x.id},1)">+</button></div></div><div class="cprice">$${(x.p*x.qty).toFixed(2)}</div></div>`).join('')
    : '<p style="text-align:center;color:#94a3b8;padding:24px 0">Cart is empty</p>';
}
function toggleCart(){document.getElementById('cart').classList.toggle('open');document.getElementById('ov').classList.toggle('open');}
function checkout(){
  if(!cart.length){alert('Your cart is empty.');return;}
  const t=cart.reduce((s,x)=>s+x.p*x.qty,0);
  cart=[];save();updateCart();toggleCart();
  alert('✓ Order placed!\nTotal: $'+t.toFixed(2)+'\nThank you for choosing us!');
}
function sub(e,f){e.preventDefault();document.getElementById('cmsg').textContent='✓ Message sent! We will get back to you shortly.';f.reset();return false;}
render(); updateCart();

/* dm-synapse */
(function(){
  const b=document.getElementById('dm-btn');
  if(localStorage.getItem('syn-dm')==='1')document.body.classList.add('dark');
  const upd=()=>{if(b)b.textContent=document.body.classList.contains('dark')?'☀️':'🌙';};
  upd();
  if(b)b.addEventListener('click',()=>{
    document.body.classList.toggle('dark');
    localStorage.setItem('syn-dm',document.body.classList.contains('dark')?'1':'0');
    upd();
  });
})();
