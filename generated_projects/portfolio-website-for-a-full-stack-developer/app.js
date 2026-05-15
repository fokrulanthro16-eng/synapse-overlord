
function sub(e,f){
  e.preventDefault();
  const msg=document.getElementById('fmsg');
  if(msg){msg.textContent='✓ Message sent! We will get back to you soon.';msg.style.color='#10b981';}
  f.reset();
  return false;
}
