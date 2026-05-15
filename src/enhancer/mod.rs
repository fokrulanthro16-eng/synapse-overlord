use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

// ── Types ─────────────────────────────────────────────────────────────────────

pub struct ResolvedProject {
    pub slug: String,
    pub dir: PathBuf,
}

pub struct EnhanceOutput {
    pub logs: Vec<String>,
    pub slug: String,
    pub backup_path: String,
    pub changed_files: Vec<String>,
}

// ── Project resolver ──────────────────────────────────────────────────────────

pub fn resolve_project(query: &str) -> Result<ResolvedProject> {
    let q = query.trim().to_lowercase();
    if q.contains("..") || q.contains('/') || q.contains('\\') {
        return Err(anyhow!("Path traversal blocked"));
    }
    if q.is_empty() {
        return Err(anyhow!("Project name is required"));
    }
    let root = Path::new("generated_projects");
    if !root.exists() {
        return Err(anyhow!("No generated_projects/ folder found. Build a project first."));
    }
    let mut hits: Vec<(String, PathBuf)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(root) {
        for entry in rd.flatten() {
            let p = entry.path();
            if !p.is_dir() { continue; }
            let slug = entry.file_name().to_string_lossy().to_lowercase();
            if slug == q || slug.contains(q.as_str()) {
                hits.push((entry.file_name().to_string_lossy().to_string(), p));
            }
        }
    }
    match hits.len() {
        0 => Err(anyhow!(
            "No project matching '{}'. Available: ls generated_projects/  |  build project <idea>",
            q
        )),
        1 => Ok(ResolvedProject { slug: hits[0].0.clone(), dir: hits[0].1.clone() }),
        _ => {
            let list = hits.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>().join(", ");
            Err(anyhow!("Multiple matches: {} — use exact slug", list))
        }
    }
}

// ── Backup ────────────────────────────────────────────────────────────────────

fn backup(dir: &Path) -> Result<String> {
    let ts = crate::settings::now_secs();
    let bd = dir.join(".synapse_backups").join(ts.to_string());
    std::fs::create_dir_all(&bd)?;
    for f in ["index.html", "styles.css", "app.js", "README.md"] {
        let src = dir.join(f);
        if src.exists() {
            std::fs::copy(&src, bd.join(f))?;
        }
    }
    Ok(bd.display().to_string())
}

// ── Enhancement detection ─────────────────────────────────────────────────────

enum Enh { DarkMode, Cart, PremiumCards, ContactForm, HeroUpgrade, General }

fn detect(s: &str) -> Enh {
    let s = s.to_lowercase();
    let has = |kw: &[&str]| kw.iter().any(|k| s.contains(k));
    if has(&["dark mode", "dark theme", "night mode", "dark"]) { return Enh::DarkMode; }
    if has(&["cart", "shopping cart", "localstorage", "local storage", "basket"]) { return Enh::Cart; }
    if has(&["premium", "glassmorphism", "glass", "card style", "modern card", "card"]) { return Enh::PremiumCards; }
    if has(&["contact", "form", "message form"]) { return Enh::ContactForm; }
    if has(&["hero", "banner", "headline", "above the fold"]) { return Enh::HeroUpgrade; }
    Enh::General
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub async fn enhance_project(query: &str, _mode: &str, instruction: &str) -> EnhanceOutput {
    let mut logs = Vec::new();
    let mut changed: Vec<String> = Vec::new();

    let proj = match resolve_project(query) {
        Ok(p) => p,
        Err(e) => {
            logs.push(format!("[Enhancer] ERROR: {}", e));
            return EnhanceOutput { logs, slug: String::new(), backup_path: String::new(), changed_files: Vec::new() };
        }
    };
    logs.push(format!("[Enhancer] Project → {}", proj.slug));

    let bp = match backup(&proj.dir) {
        Ok(p) => { logs.push(format!("[Enhancer] Backup → {}", p)); p }
        Err(e) => {
            logs.push(format!("[Enhancer] Backup failed: {}", e));
            return EnhanceOutput { logs, slug: proj.slug, backup_path: String::new(), changed_files: Vec::new() };
        }
    };

    logs.push(format!("[Enhancer] Applying: {}", instruction.trim()));
    match detect(instruction) {
        Enh::DarkMode     => apply_dark_mode(&proj.dir, &mut logs, &mut changed),
        Enh::Cart         => apply_cart(&proj.dir, &mut logs, &mut changed),
        Enh::PremiumCards => apply_premium_cards(&proj.dir, &mut logs, &mut changed),
        Enh::ContactForm  => apply_contact(&proj.dir, &mut logs, &mut changed),
        Enh::HeroUpgrade  => apply_hero(&proj.dir, &mut logs, &mut changed),
        Enh::General      => {
            apply_premium_cards(&proj.dir, &mut logs, &mut changed);
            apply_contact(&proj.dir, &mut logs, &mut changed);
        }
    }
    update_readme(&proj.dir, instruction, &mut logs, &mut changed);

    let done = if changed.is_empty() { "none (already applied)".into() } else { changed.join(", ") };
    logs.push(format!("[Enhancer] Done — changed: {}", done));
    EnhanceOutput { logs, slug: proj.slug, backup_path: bp, changed_files: changed }
}

// ── File helpers ──────────────────────────────────────────────────────────────

fn file_contains(dir: &Path, file: &str, marker: &str) -> bool {
    std::fs::read_to_string(dir.join(file))
        .map(|s| s.contains(marker))
        .unwrap_or(false)
}

fn push_changed(name: &str, changed: &mut Vec<String>) {
    if !changed.contains(&name.to_string()) { changed.push(name.to_string()); }
}

fn append_to(dir: &Path, file: &str, content: &str, logs: &mut Vec<String>, changed: &mut Vec<String>) {
    use std::io::Write;
    match std::fs::OpenOptions::new().append(true).open(dir.join(file)) {
        Ok(mut f) => { let _ = f.write_all(content.as_bytes()); push_changed(file, changed); }
        Err(e) => logs.push(format!("[Enhancer] {file} write error: {e}")),
    }
}

fn inject_before_body_end(dir: &Path, snippet: &str, logs: &mut Vec<String>, changed: &mut Vec<String>) {
    let p = dir.join("index.html");
    match std::fs::read_to_string(&p) {
        Ok(html) if html.contains("</body>") => {
            let new = html.replacen("</body>", &format!("{}\n</body>", snippet), 1);
            match std::fs::write(&p, new) {
                Ok(_) => push_changed("index.html", changed),
                Err(e) => logs.push(format!("[Enhancer] index.html write error: {e}")),
            }
        }
        Err(e) => logs.push(format!("[Enhancer] index.html read error: {e}")),
        _ => {}
    }
}

// ── Apply functions ───────────────────────────────────────────────────────────

fn apply_dark_mode(dir: &Path, logs: &mut Vec<String>, changed: &mut Vec<String>) {
    if !file_contains(dir, "styles.css", "/* dm-synapse */") {
        append_to(dir, "styles.css", DARK_MODE_CSS, logs, changed);
    }
    if !file_contains(dir, "app.js", "/* dm-synapse */") {
        append_to(dir, "app.js", DARK_MODE_JS, logs, changed);
    }
    if !file_contains(dir, "index.html", "id=\"dm-btn\"") {
        inject_before_body_end(dir, DARK_MODE_BTN, logs, changed);
    }
    logs.push("[Enhancer] Dark mode toggle applied".to_string());
}

fn apply_cart(dir: &Path, logs: &mut Vec<String>, changed: &mut Vec<String>) {
    if !file_contains(dir, "styles.css", "/* cart-syn */") {
        append_to(dir, "styles.css", CART_CSS, logs, changed);
    }
    if !file_contains(dir, "app.js", "/* cart-syn */") {
        append_to(dir, "app.js", CART_JS, logs, changed);
    }
    if !file_contains(dir, "index.html", "id=\"syn-cart-bar\"") {
        inject_before_body_end(dir, CART_HTML, logs, changed);
    }
    logs.push("[Enhancer] Cart bar with localStorage applied".to_string());
}

fn apply_premium_cards(dir: &Path, logs: &mut Vec<String>, changed: &mut Vec<String>) {
    if !file_contains(dir, "styles.css", "/* premium-syn */") {
        append_to(dir, "styles.css", PREMIUM_CSS, logs, changed);
    }
    logs.push("[Enhancer] Premium card styling applied".to_string());
}

fn apply_contact(dir: &Path, logs: &mut Vec<String>, changed: &mut Vec<String>) {
    if !file_contains(dir, "app.js", "/* contact-syn */") {
        append_to(dir, "app.js", CONTACT_JS, logs, changed);
    }
    logs.push("[Enhancer] Contact form enhancement applied".to_string());
}

fn apply_hero(dir: &Path, logs: &mut Vec<String>, changed: &mut Vec<String>) {
    if !file_contains(dir, "styles.css", "/* hero-syn */") {
        append_to(dir, "styles.css", HERO_CSS, logs, changed);
    }
    logs.push("[Enhancer] Hero section enhanced".to_string());
}

fn update_readme(dir: &Path, instruction: &str, logs: &mut Vec<String>, changed: &mut Vec<String>) {
    use std::io::Write;
    let ts = crate::settings::now_secs();
    let entry = format!("\n## Enhancement — {}\n\n> {}\n", ts, instruction.trim());
    match std::fs::OpenOptions::new().append(true).create(true).open(dir.join("README.md")) {
        Ok(mut f) => { let _ = f.write_all(entry.as_bytes()); push_changed("README.md", changed); }
        Err(e) => logs.push(format!("[Enhancer] README error: {e}")),
    }
}

// ── CSS/JS snippets ───────────────────────────────────────────────────────────

const DARK_MODE_CSS: &str = r##"
/* dm-synapse */
body.dark{background:#0f172a!important;color:#e2e8f0!important}
body.dark .nav{background:#1e293b!important;border-color:#334155!important}
body.dark .card,body.dark .prod-card{background:#1e293b!important;border-color:#334155!important}
body.dark .delivery,body.dark .contact-s{background:#0f172a!important}
body.dark .cart-panel{background:#1e293b!important}
body.dark .cats{background:#1e293b!important;border-color:#334155!important}
body.dark form input,body.dark form textarea{background:#0f172a!important;border-color:#334155!important;color:#e2e8f0!important}
body.dark .footer{background:#020617!important}
#dm-btn{position:fixed;bottom:22px;right:22px;background:#6366f1;color:#fff;border:none;border-radius:50%;width:48px;height:48px;font-size:22px;cursor:pointer;z-index:400;box-shadow:0 4px 16px rgba(0,0,0,.4);transition:background .18s}
#dm-btn:hover{background:#4f46e5}
"##;

const DARK_MODE_JS: &str = r##"
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
"##;

const DARK_MODE_BTN: &str = r##"<button id="dm-btn" title="Toggle dark mode">🌙</button>"##;

const CART_CSS: &str = r##"
/* cart-syn */
#syn-cart-bar{position:fixed;bottom:0;left:0;right:0;background:linear-gradient(90deg,#6366f1,#8b5cf6);color:#fff;padding:11px 22px;display:flex;align-items:center;justify-content:space-between;z-index:350;transform:translateY(100%);transition:transform .28s;font-size:14px;font-weight:600}
#syn-cart-bar.on{transform:translateY(0)}
#syn-cart-bar button{background:#fff;color:#6366f1;border:none;padding:6px 16px;border-radius:5px;font-weight:bold;cursor:pointer;font-size:13px}
"##;

const CART_JS: &str = r##"
/* cart-syn */
(function(){
  const bar=document.getElementById('syn-cart-bar');
  if(!bar)return;
  function sync(){
    const keys=['med-cart','shop-cart','cart'];
    let items=[];
    for(const k of keys){const v=localStorage.getItem(k);if(v)try{items=JSON.parse(v);break;}catch(_){}}
    const count=items.reduce((s,x)=>s+(x.qty||1),0);
    const total=items.reduce((s,x)=>s+((x.p||0)*(x.qty||1)),0);
    const lbl=bar.querySelector('.syn-lbl');
    if(lbl)lbl.textContent=count+' item'+(count!==1?'s':'')+' · $'+total.toFixed(2);
    bar.classList.toggle('on',count>0);
  }
  sync(); setInterval(sync,800); window.addEventListener('storage',sync);
})();
"##;

const CART_HTML: &str = r##"<div id="syn-cart-bar"><span class="syn-lbl">0 items · $0.00</span><button onclick="document.getElementById('syn-cart-bar').classList.remove('on')">✕ Close</button></div>"##;

const PREMIUM_CSS: &str = r##"
/* premium-syn */
.prod-card,.card{box-shadow:0 2px 12px rgba(0,0,0,.07)!important;border-radius:14px!important;transition:box-shadow .22s,transform .22s,border-color .22s!important}
.prod-card:hover,.card:hover{box-shadow:0 14px 40px rgba(99,102,241,.2)!important;transform:translateY(-5px)!important;border-color:#a5b4fc!important}
.prod-icon{filter:drop-shadow(0 3px 6px rgba(0,0,0,.15))}
.prod-price{background:linear-gradient(90deg,#6366f1,#8b5cf6);-webkit-background-clip:text;-webkit-text-fill-color:transparent;background-clip:text;font-size:22px!important}
.add-btn{background:linear-gradient(90deg,#6366f1,#8b5cf6)!important;border-radius:10px!important;padding:11px!important}
.add-btn:hover{background:linear-gradient(90deg,#4f46e5,#7c3aed)!important}
"##;

const CONTACT_JS: &str = r##"
/* contact-syn */
(function(){
  document.querySelectorAll('form').forEach(f=>{
    if(f.dataset.synC)return; f.dataset.synC='1';
    f.addEventListener('submit',function(e){
      e.preventDefault(); e.stopImmediatePropagation();
      const btn=f.querySelector('button[type=submit]');
      if(btn){const orig=btn.textContent;btn.textContent='Sending…';btn.disabled=true;
        setTimeout(()=>{btn.textContent=orig;btn.disabled=false;},1200);}
      setTimeout(()=>{
        f.querySelectorAll('[id$="msg"],[class*="fmsg"]').forEach(m=>{
          m.textContent='✓ Message received! We\'ll be in touch within 24 hours.';
          m.style.color='#10b981';
        });
        f.reset();
      },900);
    });
  });
})();
"##;

const HERO_CSS: &str = r##"
/* hero-syn */
.hero{background:linear-gradient(135deg,#4338ca 0%,#7c3aed 45%,#be185d 100%)!important;padding:96px 24px!important}
.hero h1{font-size:54px!important;text-shadow:0 2px 24px rgba(0,0,0,.18)!important;letter-spacing:-.5px}
.hero>p,.role{font-size:20px!important;max-width:640px;margin-left:auto;margin-right:auto}
.avail,.hlabel{background:rgba(255,255,255,.22)!important;backdrop-filter:blur(10px);border:1px solid rgba(255,255,255,.35)!important}
.hbtns .btn{padding:13px 30px!important;font-size:15px!important;box-shadow:0 6px 20px rgba(0,0,0,.25)!important}
"##;
