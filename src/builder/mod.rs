use std::path::{Path, PathBuf};
use anyhow::{anyhow, Result};

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct BuildResult {
    pub project_name: String,
    pub folder: String,
    pub project_type: String,
    pub generated_at: u64,
}

pub struct BuildOutput {
    pub logs: Vec<String>,
    pub thoughts: Vec<String>,
    #[allow(dead_code)]
    pub result: Option<BuildResult>,
}

#[derive(Clone, Copy)]
enum PType { Medical, Portfolio, Ecommerce, Generic, Restaurant, Student, Quiz, Todo, Chatbot, Admin, Files, Expense }

impl PType {
    fn label(self) -> &'static str {
        match self {
            Self::Medical    => "medical/shop",
            Self::Portfolio  => "portfolio",
            Self::Ecommerce  => "ecommerce",
            Self::Generic    => "landing-page",
            Self::Restaurant => "restaurant/menu",
            Self::Student    => "student-dashboard",
            Self::Quiz       => "quiz-app",
            Self::Todo       => "notes/todo",
            Self::Chatbot    => "ai-chatbot",
            Self::Admin      => "admin-dashboard",
            Self::Files      => "file-tracker",
            Self::Expense    => "expense-tracker",
        }
    }
}

// ── Detect ────────────────────────────────────────────────────────────────────

fn detect(idea: &str) -> PType {
    let s = idea.to_lowercase();
    let has = |kw: &[&str]| kw.iter().any(|k| s.contains(k));
    // Management dashboards / systems with specific domain content → adaptive builder
    if has(&["management dashboard","management system","management portal","management app"]) {
        return PType::Generic;
    }
    if has(&["medical","medicine","pharmacy","clinic","drug","health shop","medicard"]) {
        return PType::Medical;
    }
    if has(&["portfolio","developer","designer","resume","personal site","full stack","full-stack"]) {
        return PType::Portfolio;
    }
    if has(&["ecommerce","e-commerce","shop","store","product","cart","shopping"]) {
        return PType::Ecommerce;
    }
    if has(&["restaurant","menu","food","ordering","cafe","diner","bistro"]) { return PType::Restaurant; }
    if has(&["student","attendance","marks","gpa","grade tracker"]) { return PType::Student; }
    if has(&["quiz","exam","question","score","timer","test","trivia"]) { return PType::Quiz; }
    if has(&["notes","todo","task","reminder","checklist","planner"]) { return PType::Todo; }
    if has(&["chatbot","chat bot","ai assistant","ai chat","support bot","customer support"]) { return PType::Chatbot; }
    if has(&["admin","analytics","crm","management panel","control panel"]) { return PType::Admin; }
    if has(&["file tracking","document tracking","file manager","document manager","records","archive"]) { return PType::Files; }
    if has(&["expense","budget","finance","spending","income","transaction","money tracker"]) { return PType::Expense; }
    PType::Generic
}

// ── Slug / Dir ────────────────────────────────────────────────────────────────

pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut last = true;
    for c in s.to_lowercase().chars() {
        if c.is_alphanumeric() { out.push(c); last = false; }
        else if !last          { out.push('-'); last = true; }
    }
    if out.ends_with('-') { out.pop(); }
    if out.is_empty() { "project".to_string() } else { out }
}

fn unique_dir(slug: &str) -> PathBuf {
    let root = PathBuf::from("generated_projects");
    let base = root.join(slug);
    if !base.exists() { return base; }
    for i in 2u32..100 {
        let c = root.join(format!("{}-{}", slug, i));
        if !c.exists() { return c; }
    }
    root.join(format!("{}-{}", slug, crate::settings::now_secs()))
}

fn validate(dir: &Path) -> Result<()> {
    for f in ["index.html","styles.css","app.js","README.md"] {
        if !dir.join(f).exists() { return Err(anyhow!("Missing: {}", f)); }
    }
    let h = std::fs::read_to_string(dir.join("index.html"))?;
    if !h.contains("styles.css") { return Err(anyhow!("missing styles.css link")); }
    if !h.contains("app.js")     { return Err(anyhow!("missing app.js script")); }
    Ok(())
}

fn title_case(s: &str) -> String {
    s.split_whitespace().map(|w| {
        let mut c = w.chars();
        match c.next() {
            None    => String::new(),
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        }
    }).collect::<Vec<_>>().join(" ")
}

// ── Main handler ──────────────────────────────────────────────────────────────

pub async fn handle_build_project_command(idea: &str) -> BuildOutput {
    let mut logs     = Vec::<String>::new();
    let mut thoughts = Vec::<String>::new();
    let idea = idea.trim();
    if idea.is_empty() {
        logs.push("[Builder] Usage: build project <your project idea>".to_string());
        return BuildOutput { logs, thoughts, result: None };
    }
    logs.push(format!("[Builder] Goal: {}", idea));
    let pt = detect(idea);
    logs.push(format!("[Builder] Type detected: {}", pt.label()));
    logs.push("[Builder] Mode: offline generator".to_string());
    let slug = slugify(idea);
    let dir  = unique_dir(&slug);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        logs.push(format!("[Builder] ERROR: {}", e));
        return BuildOutput { logs, thoughts, result: None };
    }
    logs.push(format!("[Builder] Created: {}", dir.display()));
    let t = title_case(idea);
    let files: [(&str, String); 4] = [
        ("index.html", make_html(pt, &t, idea)),
        ("styles.css",  make_css()),
        ("app.js",      make_js(pt, idea)),
        ("README.md",   format!(
            "# {t}\n\n**Type:** {pt}\n**Generator:** Synapse-Overlord\n\n## Run\n\nOpen `index.html` in any browser. No build step required.\n",
            t=t, pt=pt.label()
        )),
    ];
    for (name, content) in &files {
        match std::fs::write(dir.join(name), content) {
            Ok(())  => logs.push(format!("[Builder] Wrote {}", name)),
            Err(e)  => logs.push(format!("[Builder] ERROR {}: {}", name, e)),
        }
    }
    let meta = serde_json::json!({
        "project_type": pt.label(),
        "project_name": idea,
        "generated_at": crate::settings::now_secs(),
        "generator": "synapse-overlord"
    });
    if let Ok(meta_str) = serde_json::to_string(&meta) {
        let _ = std::fs::write(dir.join(".synapse-meta.json"), meta_str);
    }
    match validate(&dir) {
        Ok(())  => logs.push("[Builder] Validation passed".to_string()),
        Err(e)  => logs.push(format!("[Builder] Validation warning: {}", e)),
    }
    logs.push(format!("[Builder] Open: {}/index.html", dir.display()));
    thoughts.push(format!("Project built → {}", dir.display()));
    BuildOutput {
        logs, thoughts,
        result: Some(BuildResult {
            project_name: idea.to_string(),
            folder: dir.display().to_string(),
            project_type: pt.label().to_string(),
            generated_at: crate::settings::now_secs(),
        }),
    }
}

// ── File assembly ─────────────────────────────────────────────────────────────

fn make_html(pt: PType, title: &str, idea: &str) -> String {
    let body = match pt {
        PType::Medical   => html_medical(title),
        PType::Portfolio => html_portfolio(title),
        PType::Ecommerce   => html_ecommerce(title),
        PType::Generic     => html_adaptive(idea, title),
        PType::Restaurant  => html_restaurant(title),
        PType::Student     => html_student(title),
        PType::Quiz        => html_quiz(title),
        PType::Todo        => html_todo(title),
        PType::Chatbot     => html_chatbot(title),
        PType::Admin       => html_admin(title),
        PType::Files       => html_files(title),
        PType::Expense     => html_expense(title),
    };
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n\
         <meta charset=\"UTF-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n\
         <title>{title}</title>\n\
         <link rel=\"stylesheet\" href=\"styles.css\">\n\
         </head>\n<body>\n{body}\n<script src=\"app.js\"></script>\n</body>\n</html>",
        title=title, body=body
    )
}

fn make_css() -> String { SHARED_CSS.to_string() }

fn make_js(pt: PType, idea: &str) -> String {
    match pt {
        PType::Medical    => JS_MEDICAL.to_string(),
        PType::Ecommerce  => JS_ECOMMERCE.to_string(),
        PType::Restaurant => JS_RESTAURANT.to_string(),
        PType::Student    => JS_STUDENT.to_string(),
        PType::Quiz       => JS_QUIZ.to_string(),
        PType::Todo       => JS_TODO.to_string(),
        PType::Chatbot    => JS_CHATBOT.to_string(),
        PType::Admin      => JS_ADMIN.to_string(),
        PType::Files      => JS_FILES.to_string(),
        PType::Expense    => JS_EXPENSE.to_string(),
        PType::Generic    => js_adaptive(idea),
        _                 => JS_SIMPLE.to_string(),
    }
}

// ── HTML bodies ───────────────────────────────────────────────────────────────

fn html_medical(t: &str) -> String {
    format!(r##"
<nav class="nav"><div class="nav-wrap"><span class="logo">💊 {t}</span>
  <input id="search" class="nav-search" placeholder="Search medicines..." oninput="filterProds()">
  <button class="cart-btn" onclick="toggleCart()">🛒 Cart <span id="cnt" class="cbadge">0</span></button>
</div></nav>
<div class="cats">
  <button class="cat active" onclick="setCat('all',this)">All Products</button>
  <button class="cat" onclick="setCat('tablets',this)">💊 Tablets</button>
  <button class="cat" onclick="setCat('vitamins',this)">⚡ Vitamins</button>
  <button class="cat" onclick="setCat('syrups',this)">🍯 Syrups</button>
  <button class="cat" onclick="setCat('firstaid',this)">🩹 First Aid</button>
</div>
<main class="main"><div class="sec-hdr"><h2 class="sec-title">Products</h2><span id="pcnt" class="badge"></span></div>
  <div id="grid" class="grid"></div></main>
<section class="delivery"><h2>🚚 Delivery Options</h2>
  <div class="row3">
    <div class="dcard"><b>⚡ Express 2-4h</b><p>Priority delivery in city areas</p></div>
    <div class="dcard"><b>🌡️ Cold Chain</b><p>Temperature-controlled transport</p></div>
    <div class="dcard"><b>💳 Pay Flexibly</b><p>Online or cash on delivery</p></div>
  </div>
</section>
<section class="contact-s"><h2>📞 Contact Us</h2>
  <div class="row2">
    <div><p>📍 123 Health Street, Medical District</p><p>📞 1800-MED-HELP (24/7)</p><p>✉️ support@medishop.com</p><p>🕐 Mon–Sun: 8 AM – 10 PM</p></div>
    <form onsubmit="return sub(event,this)">
      <input placeholder="Your Name" required><input type="email" placeholder="Email" required>
      <input type="tel" placeholder="Phone Number">
      <textarea placeholder="How can we help?" rows="4"></textarea>
      <button type="submit">Send Message</button><p id="cmsg" class="fmsg"></p>
    </form>
  </div>
</section>
<aside id="cart" class="cart-panel">
  <div class="cart-hdr"><b>🛒 Cart Summary</b><button onclick="toggleCart()">✕</button></div>
  <div id="citems" class="citems"></div>
  <div class="cart-foot">
    <div class="total-row"><span>Subtotal</span><strong id="ctotal">$0.00</strong></div>
    <button onclick="checkout()">Proceed to Checkout →</button>
    <p class="free-note">🚚 Free delivery on orders above $30</p>
  </div>
</aside>
<div id="ov" class="overlay" onclick="toggleCart()"></div>
<footer class="footer"><p>💊 {t} · Licensed Online Pharmacy · © 2025</p></footer>
"##, t=t)
}

fn html_portfolio(t: &str) -> String {
    format!(r##"
<nav class="nav"><div class="nav-wrap"><span class="logo">&lt;{t}/&gt;</span>
  <div class="links"><a href="#skills">Skills</a><a href="#projects">Projects</a><a href="#contact">Contact</a></div>
  <a class="btn" href="#contact">Hire Me</a>
</div></nav>
<section id="hero" class="hero">
  <div class="avail">🟢 Available for work</div>
  <h1>Hi, I'm <span class="grad">Alex Chen</span></h1>
  <p class="role">Full Stack Developer &amp; UI/UX Enthusiast</p>
  <p>Building fast, accessible, and beautiful web applications since 2019. Open to remote &amp; contract opportunities.</p>
  <div class="hbtns"><a class="btn" href="#projects">View My Work</a><a class="btn ghost" href="#contact">Get In Touch</a></div>
</section>
<section id="skills" class="section"><h2>🛠 Tech Stack</h2>
  <div class="tags"><span>React</span><span>TypeScript</span><span>Node.js</span><span>Python</span><span>Rust</span><span>PostgreSQL</span><span>Docker</span><span>AWS</span><span>Next.js</span><span>GraphQL</span><span>Redis</span><span>Tailwind</span></div>
</section>
<section id="projects" class="section"><h2>📦 Featured Projects</h2>
  <div class="grid3">
    <div class="card"><h3>🛒 E-Commerce Platform</h3><p>Full-stack marketplace with Stripe payments and real-time inventory management.</p><div class="tags sm"><span>React</span><span>Node.js</span><span>PostgreSQL</span></div></div>
    <div class="card"><h3>🤖 AI Chat Assistant</h3><p>GPT-4 powered support bot with context memory, analytics, and multi-language support.</p><div class="tags sm"><span>Python</span><span>FastAPI</span><span>OpenAI</span></div></div>
    <div class="card"><h3>📊 Analytics Dashboard</h3><p>Real-time data visualization with custom D3.js charts and automated CSV export.</p><div class="tags sm"><span>Vue.js</span><span>D3.js</span><span>Redis</span></div></div>
  </div>
</section>
<section id="contact" class="section"><h2>📬 Let's Work Together</h2>
  <form onsubmit="return sub(event,this)">
    <div class="frow"><input placeholder="Your Name" required><input type="email" placeholder="Email" required></div>
    <input placeholder="Subject / Role">
    <textarea placeholder="Tell me about your project..." rows="5" required></textarea>
    <button type="submit" class="btn">Send Message</button><p id="fmsg" class="fmsg"></p>
  </form>
</section>
<footer class="footer"><p>&lt;{t}/&gt; · Built with ❤️ · © 2025</p></footer>
"##, t=t)
}

fn html_ecommerce(t: &str) -> String {
    format!(r##"
<header class="nav"><div class="nav-wrap"><span class="logo">🛍️ {t}</span>
  <input id="search" class="nav-search" placeholder="Search products..." oninput="filterProds()">
  <button class="cart-btn" onclick="toggleCart()">🛒 <span id="cnt" class="cbadge">0</span></button>
</div></header>
<div class="cats">
  <button class="cat active" onclick="setCat('all',this)">All</button>
  <button class="cat" onclick="setCat('electronics',this)">📱 Electronics</button>
  <button class="cat" onclick="setCat('clothing',this)">👕 Clothing</button>
  <button class="cat" onclick="setCat('books',this)">📚 Books</button>
  <button class="cat" onclick="setCat('accessories',this)">🎒 Accessories</button>
</div>
<main class="main"><div id="grid" class="grid"></div></main>
<aside id="cart" class="cart-panel">
  <div class="cart-hdr"><b>🛒 Shopping Cart</b><button onclick="toggleCart()">✕</button></div>
  <div id="citems" class="citems"></div>
  <div class="cart-foot">
    <div class="total-row"><span>Total</span><strong id="ctotal">$0.00</strong></div>
    <button onclick="checkout()">Checkout →</button>
  </div>
</aside>
<div id="ov" class="overlay" onclick="toggleCart()"></div>
<div id="toast" class="toast"></div>
<footer class="footer"><p>🛍️ {t} · Secure Shopping · © 2025</p></footer>
"##, t=t)
}

// ── Adaptive noun extraction ──────────────────────────────────────────────────

fn extract_adaptive_nouns(s: &str) -> Vec<String> {
    const SKIP: &[&str] = &[
        "with","and","for","the","from","that","this","have","been","will","also",
        "app","apps","web","site","page","website","landing","dashboard","management",
        "admin","system","platform","portal","application","builder","tool","project",
        "card","cards","list","table","form","forms","filter","filters","search","view",
        "panel","area","widget","button","section","layout","grid","feature","features",
        "stats","stat","summary","analytics","metrics","chart","graph","report","overview",
        "booking","contact","upload","download","share","detail","modal","dialog","based",
        "modern","clean","responsive","simple","basic","full","main","core","best","nice",
        "using","like","just","even","real","estate","custom","smart","good","easy",
        "space","place","item","items","thing","things","data","info","sharing",
    ];
    let mut seen = std::collections::HashSet::new();
    s.split(|c: char| !c.is_alphabetic())
        .filter(|w| w.len() >= 4 && !SKIP.contains(w))
        .filter(|w| seen.insert(w.to_string()))
        .map(|w| {
            let mut ch = w.chars();
            match ch.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + ch.as_str(),
            }
        })
        .take(6)
        .collect()
}

// ── Adaptive dashboard generator ──────────────────────────────────────────────

fn gen_adaptive_dashboard(t: &str, entities: &[String], has_notices: bool, has_billing: bool) -> String {
    let mut out = String::new();
    let primary = entities.first().map(|s| s.as_str()).unwrap_or("Record");

    let mut nav_items: Vec<String> = vec!["Overview".to_string()];
    for e in entities.iter().take(4) { nav_items.push(e.clone()); }
    nav_items.push("Settings".to_string());

    out.push_str("<div class=\"admin-layout\">\n<aside class=\"admin-sidebar\">\n");
    out.push_str(&format!("  <div class=\"admin-logo\">⚙️ {t}</div>\n  <nav class=\"admin-nav\">\n"));
    for (i, item) in nav_items.iter().enumerate() {
        let cls = if i == 0 { "anav active" } else { "anav" };
        out.push_str(&format!("    <a class=\"{cls}\" href=\"#\" onclick=\"switchNav('{item}',this);return false\">{item}</a>\n"));
    }
    out.push_str("  </nav>\n  <div class=\"admin-sidebar-foot\"><div class=\"admin-user\">Synapse v1.0</div></div>\n</aside>\n");

    out.push_str("<div class=\"admin-body\">\n  <div class=\"admin-header\">\n");
    out.push_str(&format!("    <div><span style=\"font-size:18px;font-weight:700\">{t}</span>\
        <span style=\"display:block;font-size:12px;color:#94a3b8;margin-top:2px\">Last updated: just now</span></div>\n"));
    out.push_str("    <div style=\"display:flex;gap:8px;align-items:center\">\n");
    out.push_str("      <input id=\"dashSearch\" placeholder=\"Search...\" oninput=\"dashFilter()\" \
        style=\"padding:8px 12px;border-radius:6px;border:1px solid #e2e8f0;font-size:13px\">\n");
    out.push_str(&format!("      <button class=\"btn\" onclick=\"exportCSV()\">⬇ Export</button>\n"));
    out.push_str(&format!("      <button class=\"btn\" onclick=\"addNew()\">+ Add {primary}</button>\n"));
    out.push_str("    </div>\n  </div>\n  <div class=\"panel\">\n");

    // Stats row
    out.push_str("    <div class=\"stat-grid\">\n");
    let s_icons = ["👥","📋","📦","💰"];
    let s_vals  = ["1,284","326","89", if has_billing { "$42K" } else { "12" }];
    for (i, ico) in s_icons.iter().enumerate() {
        let lbl = entities.get(i).map(|e| format!("Total {}s", e))
            .unwrap_or_else(|| ["Records","This Week","Active","Revenue"][i].to_string());
        out.push_str(&format!("      <div class=\"stat-card\">\
            <div class=\"stat-icon\">{ico}</div>\
            <div class=\"stat-val\">{v}</div>\
            <div class=\"stat-lbl\">{lbl}</div></div>\n", ico=ico, v=s_vals[i], lbl=lbl));
    }
    out.push_str("    </div>\n");

    // Main grid: table + activity
    out.push_str("    <div class=\"admin-grid2\" style=\"margin-top:20px\">\n");

    // Primary table (wide)
    out.push_str("      <div class=\"admin-card\" style=\"grid-column:1/3\">\n");
    out.push_str(&format!("        <div class=\"card-title\">{primary} Records \
        <span style=\"float:right;font-size:12px;color:#94a3b8\" id=\"recCnt\">6 records</span></div>\n"));
    out.push_str("        <div class=\"tbl-wrap\"><table class=\"data-tbl\" id=\"mainTbl\">\n");

    let pl = primary.to_lowercase();
    let (h1,h2,h3) = if pl.contains("patient") || pl.contains("hospital") {
        ("Patient Name","Condition","Assigned Doctor")
    } else if pl.contains("student") || pl.contains("school") || pl.contains("class") {
        ("Student Name","Grade","Class")
    } else if pl.contains("teacher") || pl.contains("staff") || pl.contains("employee") {
        ("Name","Department","Role")
    } else if pl.contains("propert") || pl.contains("listing") {
        ("Property","Price","Location")
    } else if pl.contains("hotel") || pl.contains("travel") || pl.contains("destination") {
        ("Name","Location","Price/Night")
    } else {
        ("Name","Category","Date")
    };

    out.push_str(&format!("          <thead><tr><th>{h1}</th><th>{h2}</th><th>{h3}</th><th>Status</th><th>Actions</th></tr></thead>\n          <tbody id=\"tblBody\">\n"));

    let statuses = [("Active","ok"),("Active","ok"),("Pending","warn"),("Active","ok"),("Inactive","err"),("Pending","warn")];
    for i in 1..=6usize {
        let (s_lbl, s_cls) = statuses[i-1];
        let v2 = ["Category A","Category B","Category C","Category A","Category D","Category B"][i-1];
        out.push_str(&format!("            <tr>\
            <td>{primary} {i:03}</td><td>{v2}</td><td>2025-0{i}-15</td>\
            <td><span class=\"adp-badge {s_cls}\">{s_lbl}</span></td>\
            <td><button onclick=\"editRow(this)\" class=\"btn\" style=\"padding:3px 10px;font-size:11px\">Edit</button> \
            <button onclick=\"this.closest('tr').remove()\" class=\"btn\" \
            style=\"padding:3px 10px;font-size:11px;background:#ef4444\">Del</button></td></tr>\n",
            primary=primary, i=i, v2=v2, s_cls=s_cls, s_lbl=s_lbl));
    }
    out.push_str("          </tbody></table></div>\n      </div>\n");

    // Side panel: notices or activity
    out.push_str("      <div class=\"admin-card\">\n");
    if has_notices {
        out.push_str("        <div class=\"card-title\">📢 Notices</div>\n        <div class=\"activity-feed\">\n");
        let notices = [("#3b82f6","System Update","Scheduled maintenance Sunday 2–4 AM."),
                       ("#10b981","Welcome","New records can be added directly from the dashboard."),
                       ("#f59e0b","Reminder","Monthly reports are due by end of this week.")];
        for (color, title, body) in &notices {
            out.push_str(&format!("          <div class=\"act-item\" style=\"border-left:3px solid {color};padding-left:8px\">\
                <div><strong style=\"font-size:12px\">{title}</strong><div>{body}</div></div></div>\n"));
        }
        out.push_str("        </div>\n");
    } else {
        out.push_str("        <div class=\"card-title\">🕐 Recent Activity</div>\n        <div class=\"activity-feed\">\n");
        let actions = ["added","updated","viewed","exported","archived"];
        for i in 1..=5usize {
            out.push_str(&format!("          <div class=\"act-item\"><div class=\"act-dot\"></div>\
                <div><strong>{primary} {i:03}</strong> was {act}\
                <div style=\"color:#94a3b8;font-size:11px\">just now</div></div></div>\n",
                primary=primary, i=i, act=actions[i-1]));
        }
        out.push_str("        </div>\n");
    }
    out.push_str("      </div>\n");

    // Second entity overview table
    if entities.len() > 1 {
        let sec = &entities[1];
        out.push_str(&format!("      <div class=\"admin-card\">\n        <div class=\"card-title\">{sec} Overview</div>\n"));
        out.push_str(&format!("        <table class=\"data-tbl\"><thead><tr><th>{sec}</th><th>Status</th><th>Date</th></tr></thead><tbody>\n"));
        for i in 1..=4usize {
            let (sl, sc) = statuses[i-1];
            out.push_str(&format!("          <tr><td>{sec} {i:03}</td><td><span class=\"adp-badge {sc}\">{sl}</span></td><td>2025-0{i}-10</td></tr>\n"));
        }
        out.push_str("        </tbody></table>\n      </div>\n");
    }

    out.push_str("    </div>\n  </div>\n</div>\n</div>");
    out
}

// ── Adaptive file-manager generator ──────────────────────────────────────────

fn gen_adaptive_filemanager(t: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("<nav class=\"nav\"><div class=\"nav-wrap\"><span class=\"logo\">📁 {t}</span>\n  \
        <input placeholder=\"Search files...\" oninput=\"fileSearch(this.value)\" \
        style=\"flex:1;max-width:280px;padding:8px 14px;border-radius:20px;border:1px solid #e2e8f0;font-size:14px\">\n  \
        <button class=\"btn\" onclick=\"triggerUpload()\">⬆ Upload</button>\n</div></nav>\n"));

    out.push_str("<div style=\"padding:20px 24px;display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:16px\">\n");
    for (ico, val, lbl) in [("🗂️","128","Total Files"),("💾","4.2 GB","Storage Used"),("🔗","34","Shared"),("📥","12","Recent")] {
        out.push_str(&format!("  <div class=\"stat-card\"><div class=\"stat-icon\">{ico}</div>\
            <div class=\"stat-val\">{val}</div><div class=\"stat-lbl\">{lbl}</div></div>\n"));
    }
    out.push_str("</div>\n");

    out.push_str("<div style=\"padding:0 24px 20px\">\n  <div class=\"upload-zone\" onclick=\"triggerUpload()\">\n    \
        <div style=\"font-size:2.8rem;margin-bottom:10px\">📤</div>\n    \
        <p style=\"font-weight:600;margin-bottom:4px\">Drag &amp; drop files here or click to browse</p>\n    \
        <p style=\"font-size:13px;color:#94a3b8\">Supports all file types · Max 100 MB per file</p>\n    \
        <button class=\"btn\" style=\"margin-top:12px;pointer-events:none\">Choose Files</button>\n    \
        <input type=\"file\" id=\"fileInput\" multiple style=\"display:none\" onchange=\"handleUpload(this)\">\n  \
        </div>\n</div>\n");

    out.push_str("<div style=\"padding:0 24px\">\n  <div class=\"admin-card\">\n    \
        <div style=\"display:flex;justify-content:space-between;align-items:center;margin-bottom:12px\">\n      \
        <div class=\"card-title\" style=\"margin:0\">📁 Files</div>\n      \
        <div style=\"display:flex;gap:8px\">\n        \
        <select onchange=\"sortFiles(this.value)\" style=\"padding:6px 10px;border-radius:6px;border:1px solid #e2e8f0;font-size:12px\">\
        <option>Latest</option><option>Name</option><option>Size</option></select>\n        \
        <button onclick=\"deleteSelected()\" class=\"btn\" style=\"font-size:12px;padding:6px 12px;background:#ef4444\">Delete Selected</button>\n      \
        </div>\n    </div>\n    \
        <div class=\"tbl-wrap\"><table class=\"data-tbl\" id=\"fileTbl\">\n      \
        <thead><tr><th><input type=\"checkbox\" onchange=\"toggleAll(this)\"></th>\
        <th>Name</th><th>Size</th><th>Type</th><th>Modified</th><th>Actions</th></tr></thead>\n      \
        <tbody id=\"fileBody\">\n");

    let files = [("📄","Report_Q1.pdf","2.4 MB","PDF"),("📊","Presentation.pptx","5.1 MB","PPTX"),
                 ("📈","Data_Export.xlsx","1.2 MB","XLSX"),("🖼️","Logo_Final.png","0.8 MB","PNG"),
                 ("📝","Contract_2025.docx","3.6 MB","DOCX"),("🗜️","backup_archive.zip","12.4 MB","ZIP")];
    for (i,(ico,name,size,ftype)) in files.iter().enumerate() {
        let m = i+1;
        out.push_str(&format!("        <tr><td><input type=\"checkbox\" class=\"rowCheck\"></td>\
            <td>{ico} {name}</td><td>{size}</td><td>{ftype}</td><td>2025-0{m}-15</td>\
            <td style=\"display:flex;gap:5px\">\
            <button onclick=\"downloadFile('{name}')\" class=\"btn\" style=\"padding:3px 8px;font-size:11px\" title=\"Download\">↓</button>\
            <button onclick=\"shareFile('{name}')\" class=\"btn\" style=\"padding:3px 8px;font-size:11px;background:#3b82f6\">🔗</button>\
            <button onclick=\"deleteFile(this)\" class=\"btn\" style=\"padding:3px 8px;font-size:11px;background:#ef4444\">✕</button>\
            </td></tr>\n"));
    }
    out.push_str("      </tbody></table></div>\n  </div>\n</div>\n");
    out.push_str(&format!("<footer class=\"footer\"><p>📁 {t} · All rights reserved · © 2025</p></footer>\n"));
    out
}

// ── Adaptive landing / listing generator ─────────────────────────────────────

fn gen_adaptive_landing(t: &str, entities: &[String], has_list: bool, has_bkng: bool, has_pric: bool, has_team: bool) -> String {
    let mut out = String::new();
    let primary = entities.first().map(|s| s.as_str()).unwrap_or("Item");

    let icon = if has_list { "🔍" } else if has_bkng { "📅" } else if has_pric { "💳" } else { "⚡" };
    let cta = if has_bkng { "Book Now" } else if has_pric { "View Plans" } else if has_list { "Browse Now" } else { "Get Started" };
    let cta_href = if has_bkng { "book" } else if has_pric { "plans" } else if has_list { "browse" } else { "contact" };
    let tagline = if has_list {
        format!("Browse and discover {}s. Filter by your preferences and find the perfect match.", primary.to_lowercase())
    } else if has_bkng {
        format!("Book your {} in seconds. Easy scheduling, instant confirmation.", primary.to_lowercase())
    } else if has_pric {
        "Choose the plan that's right for you. Start free, upgrade anytime.".to_string()
    } else {
        format!("The smarter way to manage {}. Modern, fast, and built for you.", primary.to_lowercase())
    };

    // Nav
    let mut nav = format!("<a href=\"#features\">Features</a>");
    if has_list { nav.push_str("<a href=\"#browse\">Browse</a>"); }
    if has_pric { nav.push_str("<a href=\"#plans\">Plans</a>"); }
    if has_team { nav.push_str("<a href=\"#team\">Team</a>"); }
    if has_bkng { nav.push_str("<a href=\"#book\">Book Now</a>"); }
    nav.push_str("<a href=\"#contact\">Contact</a>");

    out.push_str(&format!("<nav class=\"nav\"><div class=\"nav-wrap\"><span class=\"logo\">{icon} {t}</span>\n  \
        <div class=\"links\">{nav}</div>\n  <a class=\"btn\" href=\"#{cta_href}\">{cta}</a>\n</div></nav>\n"));

    // Hero
    out.push_str(&format!("<section class=\"hero\">\n  <div class=\"avail\">🚀 Now Available</div>\n  \
        <h1><span class=\"grad\">{t}</span></h1>\n  <p>{tagline}</p>\n  \
        <div class=\"hbtns\"><a class=\"btn\" href=\"#{cta_href}\">{cta}</a>\
        <a class=\"btn ghost\" href=\"#features\">See Features →</a></div>\n</section>\n",
        t=t, cta_href=cta_href, cta=cta, tagline=tagline));

    // Feature cards
    out.push_str("<section id=\"features\" class=\"section\"><h2>✨ What You Get</h2><div class=\"grid3\">\n");
    let ficons = ["⚡","🔒","📊","🔗","🌍","🧠"];
    let fdescs = [
        "Optimized for peak performance with intelligent caching and fast load times.",
        "Bank-grade security with encrypted storage and granular access controls.",
        "Real-time dashboards and actionable insights to drive better decisions.",
        "Seamlessly integrates with your existing tools and workflows.",
        "Scale with confidence — automatic failover and 99.9% uptime SLA.",
        "AI-powered automation that saves hours of manual work every day.",
    ];
    for (i,(ico,desc)) in ficons.iter().zip(fdescs.iter()).enumerate() {
        let name = entities.get(i).cloned()
            .unwrap_or_else(|| ["Speed","Security","Analytics","Integration","Scale","Automation"][i].to_string());
        out.push_str(&format!("  <div class=\"card\"><h3>{ico} {name}</h3><p>{desc}</p></div>\n"));
    }
    out.push_str("</div></section>\n");

    // Stats bar
    out.push_str("<section class=\"stats-s\"><div class=\"stats-row\">\n");
    for (v,l) in [("10K+","Users"),("99.9%","Uptime"),("50M+","Requests"),("4.9★","Rating")] {
        out.push_str(&format!("  <div class=\"st\"><div class=\"st-n\">{v}</div><div>{l}</div></div>\n"));
    }
    out.push_str("</div></section>\n");

    // Listing + filter
    if has_list {
        out.push_str(&format!("<section id=\"browse\" class=\"section\"><h2>🔍 Browse {primary}s</h2>\n  <div class=\"filter-row\">\n"));
        out.push_str("    <input id=\"lSearch\" placeholder=\"Search...\" oninput=\"filterCards()\" style=\"flex:1;min-width:160px\">\n");
        out.push_str("    <select id=\"lType\" onchange=\"filterCards()\"><option value=\"\">All Types</option>\
            <option>Type A</option><option>Type B</option><option>Type C</option></select>\n");
        out.push_str("    <select id=\"lSort\" onchange=\"filterCards()\"><option>Latest</option>\
            <option>Price ↑</option><option>Price ↓</option><option>Rating</option></select>\n");
        out.push_str("  </div>\n  <div id=\"cardGrid\" class=\"listing-grid\">\n");
        let card_icons = ["🏷️","🏠","✈️","🎯","🌟","💎"];
        for i in 1..=6usize {
            let price = 100 + i * 75;
            let ico = card_icons[i % card_icons.len()];
            let r = (3 + i) % 10;
            out.push_str(&format!("    <div class=\"listing-card\" data-q=\"{primary} {i}\" onclick=\"viewDetail({i})\">\
                <div class=\"listing-thumb\">{ico}</div>\
                <div class=\"listing-body\"><h3>{primary} {i}</h3>\
                <p style=\"color:#64748b;font-size:13px;margin:6px 0\">Premium option with excellent features and great value.</p>\
                <div class=\"listing-price\">${price}</div>\
                <div style=\"display:flex;justify-content:space-between;align-items:center\">\
                <span style=\"font-size:12px;color:#94a3b8\">⭐ 4.{r} · {rev} reviews</span>\
                <button class=\"btn\" style=\"font-size:12px;padding:6px 14px\">Select</button>\
                </div></div></div>\n", primary=primary, i=i, price=price, ico=ico, r=r, rev=30+i*7));
        }
        out.push_str("  </div>\n</section>\n");
    }

    // Pricing plans
    if has_pric {
        out.push_str("<section id=\"plans\" class=\"section\"><h2>💳 Choose Your Plan</h2><div class=\"price-grid\">\n");
        let plans: &[(&str,&str,&str,bool,&[&str])] = &[
            ("Starter","$29","/mo",false, &["5 Projects","10 GB Storage","Email Support","Basic Analytics"]),
            ("Pro","$79","/mo",true,      &["Unlimited Projects","100 GB Storage","Priority Support","Advanced Analytics","Custom Domain"]),
            ("Enterprise","Custom","",false,&["Everything in Pro","1 TB Storage","Dedicated Manager","SLA Guarantee","White Label","API Access"]),
        ];
        for (name,price,per,featured,feats) in plans {
            let cls = if *featured { "price-card featured" } else { "price-card" };
            let pop = if *featured { "<div style=\"background:#6366f1;color:#fff;font-size:10px;font-weight:700;padding:2px 10px;border-radius:10px;display:inline-block;margin-bottom:8px\">POPULAR</div><br>" } else { "" };
            let fl: String = feats.iter().map(|f| format!("<li>✓ {f}</li>")).collect::<Vec<_>>().join("");
            out.push_str(&format!("  <div class=\"{cls}\">{pop}<h3>{name}</h3>\
                <div class=\"price-amt\">{price}<span style=\"font-size:1rem;font-weight:400;color:#94a3b8\">{per}</span></div>\
                <ul>{fl}</ul>\
                <button class=\"btn\" style=\"width:100%;margin-top:16px\" onclick=\"choosePlan('{name}')\">Choose {name}</button></div>\n"));
        }
        out.push_str("</div></section>\n");
    }

    // Team section
    if has_team {
        let role = entities.iter().find(|e| {
            let l = e.to_lowercase();
            ["trainer","doctor","teacher","agent","instructor","therapist","consultant","professor"].iter().any(|k| l.contains(k))
        }).map(|s| s.as_str()).unwrap_or("Expert");
        let role_s = if role.ends_with('s') { &role[..role.len()-1] } else { role };
        out.push_str(&format!("<section id=\"team\" class=\"section\"><h2>👥 Meet Our {role}s</h2><div class=\"team-grid\">\n"));
        let pfx = ["Senior","Lead","Head","Expert","Chief","Principal"];
        let ltr = ['A','B','C','D','E','F'];
        for i in 0..6usize {
            out.push_str(&format!("  <div class=\"team-card\"><div class=\"team-avatar\">{letter}</div>\
                <h3>{pre} {role_s}</h3>\
                <p style=\"color:#64748b;font-size:13px\">5+ years delivering excellence to every client.</p>\
                <div style=\"margin-top:10px;font-size:12px;color:#6366f1\">⭐ 4.{r} · {rev} clients</div></div>\n",
                letter=ltr[i], pre=pfx[i%pfx.len()], role_s=role_s, r=(7+i)%10, rev=50+i*12));
        }
        out.push_str("</div></section>\n");
    }

    // Booking form
    if has_bkng {
        out.push_str("<section id=\"book\" class=\"section\"><h2>📅 Book Now</h2>\n  \
            <form onsubmit=\"return bookSlot(event,this)\" class=\"card\" style=\"max-width:480px;margin:0 auto\">\n    \
            <div class=\"frow\"><input placeholder=\"Your Name\" required>\
            <input type=\"email\" placeholder=\"Email\" required></div>\n    \
            <input type=\"date\" id=\"bdate\" required style=\"width:100%;padding:10px;margin:6px 0;border-radius:6px;border:1px solid #e2e8f0;font-size:14px\">\n    \
            <select id=\"btime\" style=\"width:100%;padding:10px;margin:6px 0;border-radius:6px;border:1px solid #e2e8f0;font-size:14px\">\n      \
            <option>09:00 AM</option><option>10:00 AM</option><option>11:00 AM</option>\n      \
            <option>12:00 PM</option><option>02:00 PM</option><option>03:00 PM</option><option>04:00 PM</option>\n    \
            </select>\n    <textarea placeholder=\"Additional notes (optional)\" rows=\"3\"></textarea>\n    \
            <button type=\"submit\" class=\"btn\" style=\"width:100%\">Confirm Booking</button>\n    \
            <p id=\"fmsg\" class=\"fmsg\"></p>\n  </form>\n</section>\n");
    }

    // Contact
    out.push_str("<section id=\"contact\" class=\"section\"><h2>📬 Get In Touch</h2>\n  \
        <form onsubmit=\"return sub(event,this)\" style=\"max-width:480px;margin:0 auto\">\n    \
        <div class=\"frow\"><input placeholder=\"Your Name\" required>\
        <input type=\"email\" placeholder=\"Email\" required></div>\n    \
        <textarea placeholder=\"Your message...\" rows=\"4\" required></textarea>\n    \
        <button type=\"submit\" class=\"btn\" style=\"width:100%\">Send Message</button>\
        <p id=\"fmsg\" class=\"fmsg\"></p>\n  </form>\n</section>\n");

    out.push_str(&format!("<footer class=\"footer\"><p>{icon} {t} · All rights reserved · © 2025</p></footer>\n",
        icon=icon, t=t));
    out
}

// ── Main adaptive dispatcher ──────────────────────────────────────────────────

fn html_adaptive(idea: &str, t: &str) -> String {
    let s = idea.to_lowercase();
    let has = |kw: &[&str]| kw.iter().any(|k| s.contains(k));

    let is_dash  = has(&["dashboard","management system","management portal","management app",
                          "management dashboard","admin panel"]);
    let is_files = has(&["upload area","file sharing","file list","storage stats",
                          "document manager","storage usage"]);
    let has_list = has(&["listing","directory","catalog","property cards","hotel cards",
                          "listing app","browse"]) ||
                   (has(&["filter","filters"]) && has(&["cards","grid","results"]));
    let has_bkng = has(&["booking form","appointment","book a","reserve a","time slot","reservation"]);
    let has_pric = has(&["pricing plan","membership plan","subscription plan","price tier",
                          "our plans","pricing"])
        || (has(&["plans"]) && has(&["membership","subscription","pricing","tier","starter","pro","enterprise"]));
    let has_team = has(&["trainers","doctors","teachers","agents","instructors","therapists",
                          "consultants","professors","staff team","our team","meet the"]);

    let entities = extract_adaptive_nouns(&s);

    if is_dash {
        gen_adaptive_dashboard(t, &entities,
            has(&["notice","announcement","bulletin"]),
            has(&["billing","revenue","payment","invoice"]))
    } else if is_files {
        gen_adaptive_filemanager(t)
    } else {
        gen_adaptive_landing(t, &entities, has_list, has_bkng, has_pric, has_team)
    }
}

fn html_restaurant(t: &str) -> String {
    format!(r##"
<nav class="nav"><div class="nav-wrap"><span class="logo">🍽️ {t}</span>
  <input id="search" class="nav-search" placeholder="Search menu..." oninput="filterMenu()">
  <button class="cart-btn" onclick="toggleCart()">🛒 Order <span id="cnt" class="cbadge">0</span></button>
</div></nav>
<div class="cats">
  <button class="cat active" onclick="setCat('all',this)">All</button>
  <button class="cat" onclick="setCat('starters',this)">🥗 Starters</button>
  <button class="cat" onclick="setCat('mains',this)">🍽️ Mains</button>
  <button class="cat" onclick="setCat('desserts',this)">🍰 Desserts</button>
  <button class="cat" onclick="setCat('drinks',this)">🥤 Drinks</button>
</div>
<section class="hero" style="padding:40px 24px">
  <div class="avail">🕐 Open · 10 AM – 11 PM</div>
  <h1>Welcome to <span class="grad">{t}</span></h1>
  <p>Fresh ingredients. Bold flavors. Order online for pickup or delivery.</p>
</section>
<main class="main"><div class="sec-hdr"><h2 class="sec-title">Our Menu</h2><span id="mcnt" class="badge"></span></div>
  <div id="grid" class="grid"></div></main>
<aside id="cart" class="cart-panel">
  <div class="cart-hdr"><b>🛒 Your Order</b><button onclick="toggleCart()">✕</button></div>
  <div id="citems" class="citems"></div>
  <div class="cart-foot">
    <div class="total-row"><span>Total</span><strong id="ctotal">$0.00</strong></div>
    <button onclick="checkout()">Place Order →</button>
    <p class="free-note">🚚 Free delivery on orders above $25</p>
  </div>
</aside>
<div id="ov" class="overlay" onclick="toggleCart()"></div>
<section class="contact-s"><h2>📞 Contact &amp; Reservations</h2>
  <div class="row2">
    <div><p>📍 45 Flavor Street, Downtown</p><p>📞 (555) 123-4567</p><p>✉️ info@restaurant.com</p><p>🕐 Mon–Sun: 10 AM – 11 PM</p></div>
    <form onsubmit="return sub(event,this)">
      <input placeholder="Your Name" required><input type="email" placeholder="Email" required>
      <input placeholder="Date &amp; Time for Reservation">
      <textarea placeholder="Special requests..." rows="3"></textarea>
      <button type="submit">Book a Table</button><p id="fmsg" class="fmsg"></p>
    </form>
  </div>
</section>
<footer class="footer"><p>🍽️ {t} · Eat Well, Live Well · © 2025</p></footer>
"##, t=t)
}

fn html_student(t: &str) -> String {
    format!(r##"
<nav class="nav"><div class="nav-wrap"><span class="logo">🎓 {t}</span>
  <div class="links"><a href="#courses">Courses</a><a href="#assignments">Assignments</a><a href="#contact">Support</a></div>
</div></nav>
<section class="hero" style="padding:48px 24px">
  <div class="avail">📚 Semester 2 · 2025</div>
  <h1><span class="grad">Welcome Back,</span> Student!</h1>
  <p>Track your courses, grades, and upcoming assignments all in one place.</p>
</section>
<section class="section" style="padding-top:24px">
  <div class="stats-row" style="background:#f8fafc;padding:28px;border-radius:12px;max-width:100%">
    <div class="st"><div class="st-n" style="color:#6366f1">3.8</div><div style="color:#475569">GPA</div></div>
    <div class="st"><div class="st-n" style="color:#10b981">94%</div><div style="color:#475569">Attendance</div></div>
    <div class="st"><div class="st-n" style="color:#f59e0b">5</div><div style="color:#475569">Courses</div></div>
    <div class="st"><div class="st-n" style="color:#ef4444">3</div><div style="color:#475569">Due Soon</div></div>
  </div>
</section>
<section id="courses" class="section"><h2>📘 My Courses</h2>
  <div class="grid3">
    <div class="card"><h3>📊 Data Structures</h3><p>Prof. Smith · CS201</p><div class="tags sm"><span>A · 92%</span><span>Mon/Wed</span></div></div>
    <div class="card"><h3>🧮 Linear Algebra</h3><p>Prof. Patel · MATH202</p><div class="tags sm"><span>B+ · 87%</span><span>Tue/Thu</span></div></div>
    <div class="card"><h3>💻 Web Development</h3><p>Prof. Lee · CS303</p><div class="tags sm"><span>A+ · 96%</span><span>Mon/Fri</span></div></div>
    <div class="card"><h3>🧪 Physics Lab</h3><p>Prof. Johnson · PHY101</p><div class="tags sm"><span>B · 83%</span><span>Wednesday</span></div></div>
    <div class="card"><h3>📝 English Comp</h3><p>Prof. Davis · ENG110</p><div class="tags sm"><span>A · 91%</span><span>Tue/Thu</span></div></div>
  </div>
</section>
<section id="assignments" class="section"><h2>📋 Upcoming Assignments</h2>
  <div id="asgn-list" class="grid3"></div>
</section>
<section id="contact" class="section"><h2>🎓 Academic Support</h2>
  <form onsubmit="return sub(event,this)">
    <div class="frow"><input placeholder="Your Name" required><input type="email" placeholder="Student Email" required></div>
    <input placeholder="Course / Subject">
    <textarea placeholder="Describe your question or issue..." rows="4" required></textarea>
    <button type="submit" class="btn">Send to Advisor</button><p id="fmsg" class="fmsg"></p>
  </form>
</section>
<footer class="footer"><p>🎓 {t} · Powered by Synapse-Overlord · © 2025</p></footer>
"##, t=t)
}

fn html_quiz(t: &str) -> String {
    format!(r##"
<nav class="nav"><div class="nav-wrap"><span class="logo">🧠 {t}</span></div></nav>
<div style="max-width:680px;margin:48px auto;padding:0 20px">
  <div id="start-screen" class="card" style="text-align:center;padding:48px">
    <div style="font-size:56px;margin-bottom:16px">🧠</div>
    <h2 style="font-size:28px;margin-bottom:12px">{t}</h2>
    <p style="color:#64748b;margin-bottom:24px">Test your knowledge! 10 questions · 30 seconds each</p>
    <div class="tags" style="justify-content:center;margin-bottom:24px">
      <span>10 Questions</span><span>Multiple Choice</span><span>Instant Feedback</span>
    </div>
    <button class="btn" onclick="startQuiz()" style="font-size:16px;padding:14px 36px">Start Quiz →</button>
  </div>
  <div id="quiz-screen" style="display:none">
    <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:20px">
      <span id="qnum" class="badge" style="font-size:14px"></span>
      <div id="timer" style="font-size:18px;font-weight:700;color:#ef4444"></div>
      <span id="score-live" class="badge" style="background:#dcfce7;color:#166534;font-size:14px"></span>
    </div>
    <div style="background:#e0e7ff;border-radius:8px;height:6px;margin-bottom:24px">
      <div id="prog" style="background:#6366f1;height:100%;border-radius:8px;transition:width .3s"></div>
    </div>
    <div class="card" style="padding:28px;margin-bottom:20px">
      <div id="qtext" style="font-size:18px;font-weight:600;line-height:1.5;margin-bottom:20px"></div>
      <div id="opts" style="display:grid;grid-template-columns:1fr 1fr;gap:10px"></div>
    </div>
    <div style="text-align:center"><button id="nxt" class="btn" onclick="nextQ()" style="display:none">Next →</button></div>
  </div>
  <div id="result-screen" style="display:none;text-align:center">
    <div class="card" style="padding:48px">
      <div id="grade-icon" style="font-size:64px;margin-bottom:16px"></div>
      <h2 style="font-size:28px;margin-bottom:8px">Quiz Complete!</h2>
      <div id="final-score" style="font-size:48px;font-weight:800;color:#6366f1;margin:16px 0"></div>
      <p id="grade-msg" style="color:#64748b;margin-bottom:28px"></p>
      <button class="btn" onclick="restartQuiz()">Try Again</button>
    </div>
  </div>
</div>
<footer class="footer"><p>🧠 {t} · Challenge Yourself · © 2025</p></footer>
"##, t=t)
}

fn html_todo(t: &str) -> String {
    format!(r##"
<nav class="nav"><div class="nav-wrap"><span class="logo">📝 {t}</span>
  <div class="links">
    <a href="#" onclick="setView('notes');return false">Notes</a>
    <a href="#" onclick="setView('todos');return false">Tasks</a>
  </div>
  <span id="task-count" class="badge">0 tasks</span>
</div></nav>
<div style="max-width:860px;margin:32px auto;padding:0 20px">
  <div id="view-notes">
    <div class="card" style="margin-bottom:20px;padding:20px">
      <h3 style="margin-bottom:12px">✏️ New Note</h3>
      <input id="note-title" class="nav-search" style="width:100%;margin-bottom:10px;border-radius:7px" placeholder="Note title...">
      <textarea id="note-body" style="width:100%;padding:10px;border:1px solid #e2e8f0;border-radius:7px;font-family:inherit;font-size:14px;resize:vertical" rows="4" placeholder="Write your note here..."></textarea>
      <div style="margin-top:10px;display:flex;gap:8px">
        <select id="note-tag" style="padding:8px 12px;border:1px solid #e2e8f0;border-radius:7px;font-size:13px">
          <option value="general">📌 General</option>
          <option value="work">💼 Work</option>
          <option value="personal">👤 Personal</option>
          <option value="ideas">💡 Ideas</option>
        </select>
        <button class="btn" onclick="addNote()">+ Add Note</button>
      </div>
    </div>
    <div id="notes-list" class="grid3"></div>
  </div>
  <div id="view-todos" style="display:none">
    <div class="card" style="margin-bottom:20px;padding:20px">
      <h3 style="margin-bottom:12px">✅ New Task</h3>
      <div style="display:flex;gap:10px">
        <input id="todo-input" class="nav-search" style="flex:1;border-radius:7px" placeholder="Add a task..." onkeydown="if(event.key==='Enter')addTodo()">
        <select id="todo-pri" style="padding:8px 12px;border:1px solid #e2e8f0;border-radius:7px;font-size:13px">
          <option value="low">🟢 Low</option>
          <option value="medium">🟡 Medium</option>
          <option value="high">🔴 High</option>
        </select>
        <button class="btn" onclick="addTodo()">Add</button>
      </div>
    </div>
    <div class="card" style="padding:20px">
      <div style="display:flex;gap:8px;margin-bottom:16px">
        <button class="cat active" id="tf-all" onclick="filterTodos('all',this)">All</button>
        <button class="cat" id="tf-active" onclick="filterTodos('active',this)">Active</button>
        <button class="cat" id="tf-done" onclick="filterTodos('done',this)">Done</button>
      </div>
      <div id="todo-list"></div>
    </div>
  </div>
</div>
<footer class="footer"><p>📝 {t} · Stay Organized · © 2025</p></footer>
"##, t=t)
}

fn html_chatbot(t: &str) -> String {
    format!(r##"
<div class="chat-layout">
  <aside class="chat-sidebar">
    <div class="chat-sidebar-hdr"><span class="logo">🤖 {t}</span></div>
    <div class="chat-info">
      <div class="bot-avatar">🤖</div>
      <div class="bot-name">{t}</div>
      <div class="bot-status">🟢 Online · Ready to help</div>
    </div>
    <div class="sidebar-links">
      <button class="slink active" onclick="clearChat()">🗑 Clear Chat</button>
      <button class="slink" onclick="loadHistory()">📂 Load History</button>
    </div>
    <div class="suggested-section">
      <p class="suggested-label">Quick prompts</p>
      <button class="sugg" onclick="sendMsg('What can you help me with?')">What can you help me with?</button>
      <button class="sugg" onclick="sendMsg('Tell me a fun fact')">Tell me a fun fact</button>
      <button class="sugg" onclick="sendMsg('How do I get started?')">How do I get started?</button>
      <button class="sugg" onclick="sendMsg('Summarize your capabilities')">Summarize your capabilities</button>
    </div>
  </aside>
  <main class="chat-main">
    <div class="chat-topbar">
      <div><strong>🤖 {t}</strong><span class="chat-sub">AI Assistant · Powered by Synapse</span></div>
      <button class="btn" onclick="clearChat()">✕ Clear</button>
    </div>
    <div id="messages" class="messages"></div>
    <div class="chat-input-row">
      <input id="msg-input" class="chat-input" placeholder="Type your message..." onkeydown="if(event.key==='Enter'&&!event.shiftKey){{sendMsg();event.preventDefault();}}">
      <button class="send-btn" onclick="sendMsg()">➤ Send</button>
    </div>
  </main>
</div>
"##, t=t)
}

fn html_admin(t: &str) -> String {
    format!(r##"
<div class="admin-layout">
  <aside class="admin-sidebar">
    <div class="admin-logo">⚙️ {t}</div>
    <nav class="admin-nav">
      <a class="anav active" onclick="showPanel('overview',this)">📊 Overview</a>
      <a class="anav" onclick="showPanel('users',this)">👥 Users</a>
      <a class="anav" onclick="showPanel('orders',this)">📦 Orders</a>
      <a class="anav" onclick="showPanel('analytics',this)">📈 Analytics</a>
      <a class="anav" onclick="showPanel('settings',this)">⚙️ Settings</a>
    </nav>
    <div class="admin-sidebar-foot">
      <div class="admin-user">👤 Admin User</div>
    </div>
  </aside>
  <div class="admin-body">
    <header class="admin-header">
      <div>
        <h1 id="panel-title" style="font-size:22px;font-weight:700">Overview</h1>
        <p style="color:#64748b;font-size:13px">Welcome back, Admin</p>
      </div>
      <div style="display:flex;gap:10px;align-items:center">
        <input id="tbl-search" class="nav-search" style="width:200px" placeholder="Search..." oninput="filterTable()">
        <button class="btn" onclick="exportCSV()">⬇ Export</button>
      </div>
    </header>
    <div id="panel-overview" class="panel">
      <div class="stat-grid">
        <div class="stat-card" style="border-top:3px solid #6366f1"><div class="stat-icon">👥</div><div class="stat-val">12,480</div><div class="stat-lbl">Total Users</div><div class="stat-delta up">+8.2% this week</div></div>
        <div class="stat-card" style="border-top:3px solid #10b981"><div class="stat-icon">📦</div><div class="stat-val">3,291</div><div class="stat-lbl">Total Orders</div><div class="stat-delta up">+12.5% this week</div></div>
        <div class="stat-card" style="border-top:3px solid #f59e0b"><div class="stat-icon">💰</div><div class="stat-val">$84,320</div><div class="stat-lbl">Revenue</div><div class="stat-delta up">+5.1% this week</div></div>
        <div class="stat-card" style="border-top:3px solid #ef4444"><div class="stat-icon">🎫</div><div class="stat-val">47</div><div class="stat-lbl">Open Tickets</div><div class="stat-delta dn">-3 today</div></div>
      </div>
      <div class="admin-grid2">
        <div class="admin-card"><h3 class="card-title">📈 Revenue (Last 7 Days)</h3><div class="chart-bars" id="rev-chart"></div></div>
        <div class="admin-card"><h3 class="card-title">🔔 Activity Feed</h3><div id="activity-feed" class="activity-feed"></div></div>
      </div>
      <div class="admin-card" style="margin-top:20px">
        <h3 class="card-title">📋 Recent Orders</h3>
        <div class="tbl-wrap"><table id="orders-tbl" class="data-tbl"><thead><tr><th>ID</th><th>Customer</th><th>Product</th><th>Amount</th><th>Status</th><th>Date</th></tr></thead><tbody id="tbl-body"></tbody></table></div>
      </div>
    </div>
    <div id="panel-users" class="panel" style="display:none"><div class="admin-card"><h3 class="card-title">👥 User Management</h3><p style="color:#64748b">Switch to Overview to see the full data table.</p></div></div>
    <div id="panel-orders" class="panel" style="display:none"><div class="admin-card"><h3 class="card-title">📦 Order Management</h3><p style="color:#64748b">All orders are displayed in the Overview table.</p></div></div>
    <div id="panel-analytics" class="panel" style="display:none"><div class="admin-card"><h3 class="card-title">📈 Analytics</h3><p style="color:#64748b">Full analytics charts coming soon.</p></div></div>
    <div id="panel-settings" class="panel" style="display:none"><div class="admin-card"><h3 class="card-title">⚙️ Settings</h3><p style="color:#64748b">System configuration panel.</p></div></div>
  </div>
</div>
"##, t=t)
}

fn html_files(t: &str) -> String {
    format!(r##"
<nav class="nav"><div class="nav-wrap"><span class="logo">📁 {t}</span>
  <div style="display:flex;gap:10px;align-items:center;margin-left:auto">
    <input id="file-search" class="nav-search" placeholder="Search files..." oninput="filterFiles()">
    <button class="btn" onclick="showUpload()">+ Upload</button>
  </div>
</div></nav>
<div style="max-width:1140px;margin:0 auto;padding:20px">
  <div class="stat-grid" style="margin-bottom:24px">
    <div class="stat-card" style="border-top:3px solid #6366f1"><div class="stat-icon">📄</div><div class="stat-val" id="fc-total">0</div><div class="stat-lbl">Total Files</div></div>
    <div class="stat-card" style="border-top:3px solid #10b981"><div class="stat-icon">✅</div><div class="stat-val" id="fc-active">0</div><div class="stat-lbl">Active</div></div>
    <div class="stat-card" style="border-top:3px solid #f59e0b"><div class="stat-icon">🔄</div><div class="stat-val" id="fc-review">0</div><div class="stat-lbl">In Review</div></div>
    <div class="stat-card" style="border-top:3px solid #ef4444"><div class="stat-icon">📦</div><div class="stat-val" id="fc-archived">0</div><div class="stat-lbl">Archived</div></div>
  </div>
  <div id="upload-modal" style="display:none" class="admin-card" style="margin-bottom:20px;padding:20px;border:2px dashed #6366f1;text-align:center">
    <div style="font-size:40px;margin-bottom:10px">📤</div>
    <p style="margin-bottom:12px;font-weight:600">Upload a new file</p>
    <input id="up-name" class="nav-search" style="width:240px;margin-right:8px" placeholder="File name...">
    <select id="up-cat" style="padding:8px 12px;border:1px solid #e2e8f0;border-radius:7px;margin-right:8px">
      <option value="Document">Document</option><option value="Image">Image</option><option value="Spreadsheet">Spreadsheet</option><option value="Archive">Archive</option>
    </select>
    <button class="btn" onclick="uploadFile()">Upload</button>
    <button class="btn" style="background:#64748b;margin-left:6px" onclick="hideUpload()">Cancel</button>
  </div>
  <div style="display:flex;gap:8px;margin-bottom:16px;flex-wrap:wrap">
    <button class="cat active" onclick="setCatFilter('all',this)">All</button>
    <button class="cat" onclick="setCatFilter('active',this)">✅ Active</button>
    <button class="cat" onclick="setCatFilter('review',this)">🔄 In Review</button>
    <button class="cat" onclick="setCatFilter('archived',this)">📦 Archived</button>
  </div>
  <div class="admin-card" style="padding:0;overflow:hidden">
    <div class="tbl-wrap"><table class="data-tbl"><thead><tr><th>File Name</th><th>Category</th><th>Owner</th><th>Size</th><th>Status</th><th>Modified</th><th>Actions</th></tr></thead><tbody id="files-tbl"></tbody></table></div>
  </div>
  <div class="admin-card" style="margin-top:20px"><h3 class="card-title">🕐 Activity Timeline</h3><div id="file-timeline" class="activity-feed"></div></div>
</div>
<footer class="footer"><p>📁 {t} · Secure File Management · © 2025</p></footer>
"##, t=t)
}

fn html_expense(t: &str) -> String {
    format!(r##"
<nav class="nav"><div class="nav-wrap"><span class="logo">💰 {t}</span>
  <select id="month-filter" style="padding:8px 12px;border:1px solid #e2e8f0;border-radius:7px;font-size:13px;margin-left:auto" onchange="renderAll()">
    <option value="all">All Time</option>
    <option value="2025-05">May 2025</option>
    <option value="2025-04">Apr 2025</option>
    <option value="2025-03">Mar 2025</option>
  </select>
</div></nav>
<div style="max-width:960px;margin:0 auto;padding:20px">
  <div class="stat-grid" style="margin-bottom:24px">
    <div class="stat-card" style="border-top:3px solid #10b981"><div class="stat-icon">📈</div><div class="stat-val" id="total-income">$0.00</div><div class="stat-lbl">Total Income</div></div>
    <div class="stat-card" style="border-top:3px solid #ef4444"><div class="stat-icon">📉</div><div class="stat-val" id="total-expense">$0.00</div><div class="stat-lbl">Total Expenses</div></div>
    <div class="stat-card" style="border-top:3px solid #6366f1"><div class="stat-icon">💳</div><div class="stat-val" id="balance">$0.00</div><div class="stat-lbl">Balance</div></div>
    <div class="stat-card" style="border-top:3px solid #f59e0b"><div class="stat-icon">🧾</div><div class="stat-val" id="tx-count">0</div><div class="stat-lbl">Transactions</div></div>
  </div>
  <div style="display:grid;grid-template-columns:1fr 1.4fr;gap:20px;margin-bottom:24px">
    <div class="admin-card">
      <h3 class="card-title">➕ Add Transaction</h3>
      <div style="display:flex;gap:6px;margin-bottom:12px">
        <button id="type-income"  class="cat active" onclick="setType('income',this)">📈 Income</button>
        <button id="type-expense" class="cat"        onclick="setType('expense',this)">📉 Expense</button>
      </div>
      <input id="tx-desc" class="nav-search" style="width:100%;margin-bottom:10px;border-radius:7px" placeholder="Description...">
      <input id="tx-amount" type="number" min="0" step="0.01" class="nav-search" style="width:100%;margin-bottom:10px;border-radius:7px" placeholder="Amount ($)">
      <select id="tx-cat" style="width:100%;padding:8px 12px;border:1px solid #e2e8f0;border-radius:7px;font-size:13px;margin-bottom:12px">
        <option value="Food">🍔 Food</option><option value="Transport">🚗 Transport</option><option value="Shopping">🛍️ Shopping</option>
        <option value="Health">💊 Health</option><option value="Entertainment">🎬 Entertainment</option><option value="Salary">💼 Salary</option>
        <option value="Freelance">💻 Freelance</option><option value="Other">📌 Other</option>
      </select>
      <input id="tx-date" type="date" class="nav-search" style="width:100%;margin-bottom:12px;border-radius:7px">
      <button class="btn" style="width:100%" onclick="addTx()">Add Transaction</button>
    </div>
    <div class="admin-card">
      <h3 class="card-title">📊 Spending by Category</h3>
      <div id="cat-chart" style="padding-top:8px"></div>
    </div>
  </div>
  <div class="admin-card">
    <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:16px">
      <h3 class="card-title" style="margin:0">🧾 Transactions</h3>
      <div style="display:flex;gap:8px">
        <button class="cat active" id="tf-all"     onclick="setTxFilter('all',this)">All</button>
        <button class="cat"        id="tf-income"  onclick="setTxFilter('income',this)">Income</button>
        <button class="cat"        id="tf-expense" onclick="setTxFilter('expense',this)">Expense</button>
      </div>
    </div>
    <div id="tx-list"></div>
  </div>
</div>
<footer class="footer"><p>💰 {t} · Track Every Dollar · © 2025</p></footer>
"##, t=t)
}

// ── Shared CSS ────────────────────────────────────────────────────────────────

const SHARED_CSS: &str = r##"
*,*::before,*::after{box-sizing:border-box;margin:0;padding:0}
body{font-family:'Segoe UI',system-ui,sans-serif;background:#f8fafc;color:#1e293b;line-height:1.6}
a{color:inherit;text-decoration:none}
/* Nav */
.nav{background:#fff;border-bottom:1px solid #e2e8f0;position:sticky;top:0;z-index:100;padding:10px 20px}
.nav-wrap{max-width:1140px;margin:0 auto;display:flex;align-items:center;gap:12px;flex-wrap:wrap}
.logo{font-size:20px;font-weight:800;flex-shrink:0}
.links{display:flex;gap:18px;font-size:14px;font-weight:500}.links a:hover{color:#6366f1}
.nav-search{flex:1;min-width:160px;padding:8px 14px;border:1px solid #e2e8f0;border-radius:20px;font-size:14px;outline:none}
.nav-search:focus{border-color:#6366f1}
.btn{background:#6366f1;color:#fff;padding:9px 20px;border-radius:7px;font-size:14px;font-weight:600;border:none;cursor:pointer;display:inline-block;transition:background .18s}
.btn:hover{background:#4f46e5}
.btn.ghost{background:transparent;border:2px solid #fff;color:#fff}
.btn.ghost:hover{background:rgba(255,255,255,.15)}
.cart-btn{background:transparent;border:2px solid #6366f1;color:#6366f1;padding:7px 16px;border-radius:7px;cursor:pointer;font-weight:600;font-size:14px;flex-shrink:0;transition:all .18s}
.cart-btn:hover{background:#6366f1;color:#fff}
.cbadge{background:#ef4444;color:#fff;border-radius:10px;padding:1px 7px;font-size:12px;margin-left:4px}
/* Hero */
.hero{background:linear-gradient(135deg,#6366f1 0%,#8b5cf6 100%);color:#fff;text-align:center;padding:80px 24px}
.avail,.hlabel{background:rgba(255,255,255,.2);display:inline-block;padding:5px 16px;border-radius:20px;font-size:13px;margin-bottom:18px}
.hero h1{font-size:44px;font-weight:800;line-height:1.15;margin-bottom:14px}
.hero p,.role{font-size:18px;opacity:.9;margin-bottom:8px}
.grad{background:linear-gradient(90deg,#fbbf24,#f472b6);-webkit-background-clip:text;-webkit-text-fill-color:transparent;background-clip:text}
.hbtns{display:flex;gap:12px;justify-content:center;flex-wrap:wrap;margin-top:28px}
/* Section */
.section{padding:64px 24px;max-width:1140px;margin:0 auto}
.section h2{font-size:30px;font-weight:700;margin-bottom:28px}
.sec-hdr{display:flex;align-items:center;gap:10px;padding:20px 20px 0}
.sec-title{font-size:22px;font-weight:700}
.badge{background:#e0e7ff;color:#3730a3;padding:3px 10px;border-radius:10px;font-size:12px}
/* Grids */
.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(220px,1fr));gap:18px;padding:20px}
.grid3{display:grid;grid-template-columns:repeat(auto-fit,minmax(240px,1fr));gap:20px}
.card{background:#fff;border:1px solid #e2e8f0;border-radius:12px;padding:22px;transition:box-shadow .2s,transform .2s}
.card:hover{box-shadow:0 8px 28px rgba(0,0,0,.09);transform:translateY(-2px)}
.card h3{font-size:16px;font-weight:700;margin-bottom:8px}
.card p{color:#64748b;font-size:14px}
/* Tags */
.tags{display:flex;flex-wrap:wrap;gap:8px}
.tags span{background:#e0e7ff;color:#3730a3;padding:5px 14px;border-radius:20px;font-size:13px;font-weight:500}
.tags.sm span{font-size:11px;padding:2px 9px;margin-top:8px}
/* Product card */
.prod-card{background:#fff;border:1px solid #e2e8f0;border-radius:12px;padding:18px;display:flex;flex-direction:column;transition:box-shadow .2s,transform .2s}
.prod-card:hover{box-shadow:0 8px 28px rgba(0,0,0,.09);transform:translateY(-2px)}
.prod-icon{font-size:38px;text-align:center;margin-bottom:10px}
.prod-name{font-weight:700;font-size:15px;margin-bottom:4px}
.prod-sub{font-size:12px;color:#94a3b8;margin-bottom:10px}
.prod-price{font-size:20px;font-weight:800;color:#6366f1;margin-bottom:8px}
.stk{font-size:11px;padding:2px 9px;border-radius:10px;width:fit-content;margin-bottom:10px}
.stk.ok{background:#dcfce7;color:#166534}.stk.low{background:#fef9c3;color:#854d0e}.stk.no{background:#fee2e2;color:#991b1b}
.add-btn{background:#6366f1;color:#fff;border:none;padding:9px;border-radius:7px;cursor:pointer;font-weight:600;font-size:13px;margin-top:auto;transition:background .18s}
.add-btn:hover{background:#4f46e5}.add-btn:disabled{background:#cbd5e1;cursor:not-allowed}
/* Categories */
.cats{background:#fff;border-bottom:1px solid #e2e8f0;padding:10px 20px;display:flex;gap:8px;flex-wrap:wrap;justify-content:center;position:sticky;top:56px;z-index:90}
.cat{padding:7px 18px;border:2px solid #6366f1;color:#6366f1;background:transparent;border-radius:20px;cursor:pointer;font-size:13px;font-weight:600;transition:all .18s}
.cat.active,.cat:hover{background:#6366f1;color:#fff}
/* Cart */
.cart-panel{position:fixed;right:-370px;top:0;height:100vh;width:350px;background:#fff;box-shadow:-4px 0 32px rgba(0,0,0,.12);z-index:200;display:flex;flex-direction:column;transition:right .28s ease}
.cart-panel.open{right:0}
.cart-hdr{display:flex;align-items:center;justify-content:space-between;padding:18px;border-bottom:1px solid #e2e8f0}
.cart-hdr b{font-size:16px}.cart-hdr button{background:none;border:none;font-size:20px;cursor:pointer;color:#94a3b8;line-height:1}
.citems{flex:1;overflow-y:auto;padding:12px}
.citem{display:flex;gap:10px;padding:10px 0;border-bottom:1px solid #f1f5f9}
.cname{font-size:13px;font-weight:600;flex:1}
.cprice{font-size:13px;color:#6366f1;font-weight:700}
.qbtns{display:flex;gap:4px;align-items:center;margin-top:5px}
.qb{background:#f1f5f9;border:1px solid #e2e8f0;width:26px;height:26px;border-radius:5px;cursor:pointer;font-size:15px;font-weight:700;display:flex;align-items:center;justify-content:center}
.qb:hover{background:#6366f1;color:#fff;border-color:#6366f1}
.cart-foot{padding:16px;border-top:1px solid #e2e8f0}
.total-row{display:flex;justify-content:space-between;font-size:16px;font-weight:700;margin-bottom:12px}
.cart-foot button{background:#6366f1;color:#fff;border:none;padding:13px;border-radius:8px;width:100%;font-size:15px;font-weight:700;cursor:pointer}
.cart-foot button:hover{background:#4f46e5}
.free-note{font-size:12px;text-align:center;color:#94a3b8;margin-top:8px}
.overlay{display:none;position:fixed;inset:0;background:rgba(0,0,0,.35);z-index:190}
.overlay.open{display:block}
.toast{position:fixed;bottom:22px;left:50%;transform:translateX(-50%) translateY(120px);background:#1e293b;color:#fff;padding:11px 22px;border-radius:8px;font-size:14px;transition:transform .28s;z-index:300}
.toast.show{transform:translateX(-50%) translateY(0)}
/* Delivery section */
.delivery{background:#fff;padding:52px 24px;text-align:center}
.delivery h2{font-size:26px;font-weight:700;margin-bottom:28px}
.row3{display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:16px;max-width:900px;margin:0 auto}
.dcard{background:#f8fafc;border-radius:12px;padding:22px;text-align:left}
.dcard b{display:block;margin-bottom:8px;font-size:15px}.dcard p{color:#64748b;font-size:13px}
/* Contact section */
.contact-s{padding:52px 24px;max-width:1100px;margin:0 auto}
.contact-s h2{font-size:26px;font-weight:700;margin-bottom:28px}
.row2{display:grid;grid-template-columns:1fr 1.6fr;gap:36px}
@media(max-width:720px){.row2{grid-template-columns:1fr}}
/* Stats section */
.stats-s{background:linear-gradient(135deg,#6366f1,#8b5cf6);color:#fff;padding:52px 24px}
.stats-row{display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:24px;max-width:900px;margin:0 auto;text-align:center}
.st-n{font-size:38px;font-weight:800;margin-bottom:4px}
/* Forms */
form input,form textarea{display:block;width:100%;padding:11px 14px;border:1px solid #e2e8f0;border-radius:7px;margin-bottom:12px;font-size:14px;font-family:inherit;outline:none;background:#fafafa}
form input:focus,form textarea:focus{border-color:#6366f1;background:#fff}
form button[type=submit]{background:#6366f1;color:#fff;border:none;padding:12px 26px;border-radius:7px;font-size:15px;font-weight:600;cursor:pointer;width:100%}
form button[type=submit]:hover{background:#4f46e5}
.frow{display:grid;grid-template-columns:1fr 1fr;gap:12px}
@media(max-width:600px){.frow{grid-template-columns:1fr}}
.fmsg{font-size:13px;margin-top:6px;color:#10b981}
/* Main */
.main{max-width:1140px;margin:0 auto}
/* Footer */
.footer{background:#0f172a;color:#64748b;text-align:center;padding:28px 20px;margin-top:64px;font-size:13px}
@media(max-width:600px){.hero h1{font-size:30px}.links{display:none}}
/* Admin / Chat layouts */
.admin-layout,.chat-layout{display:flex;min-height:100vh}
.admin-sidebar,.chat-sidebar{width:220px;background:#0f172a;color:#cbd5e1;display:flex;flex-direction:column;flex-shrink:0;padding:0}
.admin-logo,.chat-sidebar-hdr{padding:20px 16px;font-size:17px;font-weight:800;color:#fff;border-bottom:1px solid #1e293b}
.admin-nav{padding:12px 0;flex:1}.anav{display:block;padding:11px 20px;color:#94a3b8;font-size:13px;font-weight:500;cursor:pointer;border:none;background:none;text-align:left;width:100%;transition:all .15s;border-radius:0}
.anav:hover,.anav.active{background:#1e293b;color:#fff}
.admin-sidebar-foot,.chat-sidebar .sidebar-links{padding:16px}
.admin-user,.bot-name{font-size:13px;color:#94a3b8}
.admin-body,.chat-main{flex:1;display:flex;flex-direction:column;background:#f8fafc;overflow:hidden}
.admin-header{display:flex;justify-content:space-between;align-items:center;padding:20px 28px;background:#fff;border-bottom:1px solid #e2e8f0}
.panel{padding:24px 28px;flex:1;overflow-y:auto}
.stat-grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:16px}
.stat-card{background:#fff;border-radius:12px;padding:20px;box-shadow:0 1px 6px rgba(0,0,0,.06)}
.stat-icon{font-size:26px;margin-bottom:8px}.stat-val{font-size:26px;font-weight:800;margin-bottom:2px}.stat-lbl{font-size:12px;color:#64748b}
.stat-delta{font-size:11px;margin-top:4px}.stat-delta.up{color:#10b981}.stat-delta.dn{color:#ef4444}
.admin-grid2{display:grid;grid-template-columns:1fr 1fr;gap:20px;margin-top:20px}
.admin-card{background:#fff;border-radius:12px;padding:20px;box-shadow:0 1px 6px rgba(0,0,0,.06)}
.card-title{font-size:15px;font-weight:700;margin-bottom:14px}
.tbl-wrap{overflow-x:auto}.data-tbl{width:100%;border-collapse:collapse;font-size:13px}
.data-tbl th{background:#f8fafc;padding:10px 14px;text-align:left;font-weight:600;color:#475569;border-bottom:2px solid #e2e8f0}
.data-tbl td{padding:10px 14px;border-bottom:1px solid #f1f5f9;color:#334155}
.data-tbl tr:hover td{background:#f8fafc}
.chart-bars{display:flex;align-items:flex-end;gap:8px;height:120px;padding-top:10px}
.bar-wrap{display:flex;flex-direction:column;align-items:center;gap:4px;flex:1}
.bar{background:linear-gradient(180deg,#6366f1,#8b5cf6);border-radius:4px 4px 0 0;width:100%;transition:height .4s}
.bar-lbl{font-size:10px;color:#94a3b8}
.activity-feed{display:flex;flex-direction:column;gap:10px;max-height:220px;overflow-y:auto}
.act-item{display:flex;gap:10px;font-size:12px;color:#475569;padding:8px 0;border-bottom:1px solid #f1f5f9}
.act-dot{width:8px;height:8px;border-radius:50%;background:#6366f1;margin-top:4px;flex-shrink:0}
/* Chat UI */
.chat-info{text-align:center;padding:24px 16px;border-bottom:1px solid #1e293b}
.bot-avatar{font-size:48px;margin-bottom:8px}
.bot-status{font-size:11px;color:#10b981;margin-top:4px}
.suggested-section{padding:16px}.suggested-label{font-size:11px;color:#64748b;margin-bottom:8px}
.slink,.sugg{display:block;width:100%;padding:9px 12px;margin-bottom:6px;background:#1e293b;border:none;color:#94a3b8;border-radius:7px;font-size:12px;cursor:pointer;text-align:left;transition:all .15s}
.slink:hover,.sugg:hover{background:#6366f1;color:#fff}
.chat-topbar{display:flex;justify-content:space-between;align-items:center;padding:14px 20px;background:#fff;border-bottom:1px solid #e2e8f0}
.chat-sub{font-size:11px;color:#94a3b8;display:block}
.messages{flex:1;overflow-y:auto;padding:20px;display:flex;flex-direction:column;gap:14px;background:#f8fafc}
.msg{max-width:72%;padding:12px 16px;border-radius:16px;font-size:14px;line-height:1.55;word-break:break-word}
.msg.user{background:#6366f1;color:#fff;align-self:flex-end;border-bottom-right-radius:4px}
.msg.bot{background:#fff;color:#1e293b;align-self:flex-start;border-bottom-left-radius:4px;box-shadow:0 1px 6px rgba(0,0,0,.07)}
.msg-meta{font-size:10px;opacity:.65;margin-top:5px}
.typing{display:flex;gap:4px;padding:12px 16px;background:#fff;border-radius:16px;align-self:flex-start;box-shadow:0 1px 6px rgba(0,0,0,.07)}
.typing span{width:7px;height:7px;background:#94a3b8;border-radius:50%;animation:bounce .9s infinite}
.typing span:nth-child(2){animation-delay:.15s}.typing span:nth-child(3){animation-delay:.3s}
@keyframes bounce{0%,60%,100%{transform:translateY(0)}30%{transform:translateY(-8px)}}
.chat-input-row{display:flex;gap:10px;padding:14px 20px;background:#fff;border-top:1px solid #e2e8f0}
.chat-input{flex:1;padding:11px 16px;border:1px solid #e2e8f0;border-radius:24px;font-size:14px;outline:none;font-family:inherit}
.chat-input:focus{border-color:#6366f1}
.send-btn{background:#6366f1;color:#fff;border:none;padding:11px 22px;border-radius:24px;font-weight:600;cursor:pointer;font-size:14px;transition:background .18s}
.send-btn:hover{background:#4f46e5}
@media(max-width:700px){.admin-sidebar,.chat-sidebar{display:none}.admin-grid2{grid-template-columns:1fr}}
/* ── Adaptive builder extras ──────────────────────────────────────────────── */
.adp-badge{display:inline-block;padding:2px 9px;border-radius:10px;font-size:11px;font-weight:600}
.adp-badge.ok{background:#dcfce7;color:#166534}.adp-badge.warn{background:#fef9c3;color:#854d0e}.adp-badge.err{background:#fee2e2;color:#991b1b}.adp-badge.info{background:#dbeafe;color:#1e40af}
.upload-zone{border:2px dashed #e2e8f0;border-radius:12px;padding:36px;text-align:center;cursor:pointer;transition:border-color .2s,background .2s}
.upload-zone:hover{border-color:#6366f1;background:#f5f3ff}
.price-grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(240px,1fr));gap:20px;max-width:940px;margin:0 auto}
.price-card{background:#fff;border:2px solid #e2e8f0;border-radius:14px;padding:28px;transition:box-shadow .2s}
.price-card.featured{border-color:#6366f1;box-shadow:0 0 0 3px rgba(99,102,241,.15)}
.price-card h3{font-size:18px;font-weight:700;margin-bottom:4px}
.price-amt{font-size:2.2rem;font-weight:800;color:#6366f1;margin:12px 0}
.price-card ul{list-style:none;text-align:left;margin:16px 0;display:flex;flex-direction:column;gap:8px;font-size:13px;color:#475569}
.team-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(180px,1fr));gap:20px}
.team-card{background:#fff;border:1px solid #e2e8f0;border-radius:12px;padding:24px;text-align:center}
.team-avatar{width:72px;height:72px;border-radius:50%;background:linear-gradient(135deg,#6366f1,#8b5cf6);display:flex;align-items:center;justify-content:center;font-size:1.6rem;font-weight:700;color:#fff;margin:0 auto 12px}
.listing-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(280px,1fr));gap:20px}
.listing-card{background:#fff;border:1px solid #e2e8f0;border-radius:12px;overflow:hidden;transition:box-shadow .2s,transform .2s;cursor:pointer}
.listing-card:hover{box-shadow:0 8px 28px rgba(0,0,0,.1);transform:translateY(-2px)}
.listing-thumb{height:160px;background:linear-gradient(135deg,#e0e7ff,#ddd6fe);display:flex;align-items:center;justify-content:center;font-size:2.5rem}
.listing-body{padding:16px}
.listing-price{font-size:1.2rem;font-weight:800;color:#6366f1;margin:6px 0}
.filter-row{display:flex;gap:10px;flex-wrap:wrap;margin-bottom:24px;align-items:center}
.filter-row input,.filter-row select{padding:9px 14px;border:1px solid #e2e8f0;border-radius:8px;font-size:13px;outline:none;background:#fff;color:#1e293b}
.filter-row input:focus,.filter-row select:focus{border-color:#6366f1}
"##;

// ── JavaScript ────────────────────────────────────────────────────────────────

const JS_MEDICAL: &str = r##"
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
"##;

const JS_ECOMMERCE: &str = r##"
const P=[
  {id:1,n:'Wireless Headphones',s:'Electronics',p:79.99,i:'🎧',r:4.8},
  {id:2,n:'Mechanical Keyboard',s:'Electronics',p:129.99,i:'⌨️',r:4.7},
  {id:3,n:'Smart Watch',        s:'Electronics',p:199.99,i:'⌚',r:4.4},
  {id:4,n:'Running Sneakers',   s:'Clothing',   p:89.99, i:'👟',r:4.6},
  {id:5,n:'Yoga Mat',           s:'Clothing',   p:29.99, i:'🧘',r:4.7},
  {id:6,n:'JavaScript Book',    s:'Books',      p:39.99, i:'📚',r:4.9},
  {id:7,n:'Design Patterns',    s:'Books',      p:49.99, i:'📖',r:4.8},
  {id:8,n:'Leather Wallet',     s:'Accessories',p:34.99, i:'👛',r:4.5},
  {id:9,n:'Sunglasses',         s:'Accessories',p:49.99, i:'🕶️',r:4.3},
];
const catMap={Electronics:'electronics',Clothing:'clothing',Books:'books',Accessories:'accessories'};
let cat='all', q='', cart=JSON.parse(localStorage.getItem('shop-cart')||'[]');
function filterProds(){q=document.getElementById('search').value.toLowerCase();render();}
function setCat(c,el){cat=c;document.querySelectorAll('.cat').forEach(b=>b.classList.remove('active'));el.classList.add('active');render();}
function render(){
  const f=P.filter(p=>(cat==='all'||catMap[p.s]===cat)&&(p.n.toLowerCase().includes(q)));
  document.getElementById('grid').innerHTML=f.map(p=>`
<div class="prod-card">
  <div class="prod-icon">${p.i}</div>
  <div class="prod-name">${p.n}</div>
  <div class="prod-sub">${p.s} · ${'★'.repeat(Math.round(p.r))} ${p.r}</div>
  <div class="prod-price">$${p.p.toFixed(2)}</div>
  <button class="add-btn" onclick="addCart(${p.id})">+ Add to Cart</button>
</div>`).join('');
}
function addCart(id){
  const p=P.find(x=>x.id===id); if(!p) return;
  const ex=cart.find(x=>x.id===id);
  if(ex) ex.qty++; else cart.push({...p,qty:1});
  save(); updateCart(); showToast(p.n+' added to cart!');
}
function changeQty(id,d){
  const i=cart.findIndex(x=>x.id===id); if(i<0) return;
  cart[i].qty+=d; if(cart[i].qty<=0) cart.splice(i,1);
  save(); updateCart();
}
function save(){localStorage.setItem('shop-cart',JSON.stringify(cart));}
function updateCart(){
  document.getElementById('cnt').textContent=cart.reduce((s,x)=>s+x.qty,0);
  document.getElementById('ctotal').textContent='$'+cart.reduce((s,x)=>s+x.p*x.qty,0).toFixed(2);
  document.getElementById('citems').innerHTML=cart.length
    ? cart.map(x=>`<div class="citem"><div style="flex:1"><div class="cname">${x.i} ${x.n}</div><div class="qbtns"><button class="qb" onclick="changeQty(${x.id},-1)">−</button><span style="font-size:13px;min-width:20px;text-align:center">${x.qty}</span><button class="qb" onclick="changeQty(${x.id},1)">+</button></div></div><div class="cprice">$${(x.p*x.qty).toFixed(2)}</div></div>`).join('')
    : '<p style="text-align:center;color:#94a3b8;padding:24px 0">Cart is empty</p>';
}
function toggleCart(){document.getElementById('cart').classList.toggle('open');document.getElementById('ov').classList.toggle('open');}
function checkout(){
  if(!cart.length) return;
  const t=cart.reduce((s,x)=>s+x.p*x.qty,0);
  cart=[];save();updateCart();toggleCart();showToast('Order placed! Total: $'+t.toFixed(2));
}
function showToast(m){const t=document.getElementById('toast');t.textContent=m;t.classList.add('show');setTimeout(()=>t.classList.remove('show'),3000);}
render(); updateCart();
"##;

fn js_adaptive(idea: &str) -> String {
    let s = idea.to_lowercase();
    let has = |kw: &[&str]| kw.iter().any(|k| s.contains(k));
    let mut js = JS_SIMPLE.to_string();

    let is_dash  = has(&["dashboard","management system","management portal","management app","management dashboard"]);
    let is_files = has(&["upload area","file sharing","file list","storage stats","document manager"]);
    let has_list = has(&["listing","directory","catalog","property cards","hotel cards","listing app","browse"]) ||
                   (has(&["filter","filters"]) && has(&["cards","grid","results"]));
    let has_bkng = has(&["booking form","appointment","book a","reserve a","time slot","reservation"]);
    let has_pric = has(&["pricing plan","membership plan","subscription plan","price tier","our plans","pricing"])
        || (has(&["plans"]) && has(&["membership","subscription","pricing","tier","starter","pro","enterprise"]));

    if is_dash {
        js.push_str(r##"
function switchNav(name,el){
  document.querySelectorAll('.anav').forEach(a=>a.classList.remove('active'));
  el.classList.add('active');
  const h=document.querySelector('.admin-header span');
  if(h)h.textContent=name;
}
function dashFilter(){
  const q=(document.getElementById('dashSearch')||{value:''}).value.toLowerCase();
  document.querySelectorAll('#tblBody tr').forEach(r=>r.style.display=r.textContent.toLowerCase().includes(q)?'':'none');
  const vis=[...document.querySelectorAll('#tblBody tr')].filter(r=>r.style.display!=='none').length;
  const cnt=document.getElementById('recCnt');if(cnt)cnt.textContent=vis+' records';
}
function exportCSV(){
  const rows=[...document.querySelectorAll('#mainTbl tr')].map(r=>[...r.cells].map(c=>'"'+c.textContent.trim()+'"').join(',')).join('\n');
  const a=document.createElement('a');a.href='data:text/csv,'+encodeURIComponent(rows);a.download='export.csv';a.click();
}
function addNew(){
  const name=prompt('Enter name:');if(!name)return;
  const tb=document.getElementById('tblBody');if(!tb)return;
  const tr=document.createElement('tr');
  const today=new Date().toISOString().split('T')[0];
  tr.innerHTML=`<td>${name}</td><td>General</td><td>${today}</td><td><span class="adp-badge ok">Active</span></td><td><button onclick="editRow(this)" class="btn" style="padding:3px 10px;font-size:11px">Edit</button> <button onclick="this.closest('tr').remove()" class="btn" style="padding:3px 10px;font-size:11px;background:#ef4444">Del</button></td>`;
  tb.prepend(tr);
  const cnt=document.getElementById('recCnt');if(cnt)cnt.textContent=tb.rows.length+' records';
}
function editRow(btn){
  const td=btn.closest('tr').cells[0];
  const n=prompt('Edit name:',td.textContent);if(n)td.textContent=n;
}
"##);
    }

    if is_files {
        js.push_str(r##"
function triggerUpload(){document.getElementById('fileInput').click();}
function handleUpload(input){
  const tb=document.getElementById('fileBody');if(!tb||!input.files.length)return;
  [...input.files].forEach(file=>{
    const tr=document.createElement('tr');
    const size=(file.size/1048576).toFixed(1)+' MB';
    const ext=file.name.split('.').pop().toUpperCase();
    const today=new Date().toISOString().split('T')[0];
    tr.innerHTML=`<td><input type="checkbox" class="rowCheck"></td><td>📄 ${file.name}</td><td>${size}</td><td>${ext}</td><td>${today}</td><td style="display:flex;gap:5px"><button class="btn" style="padding:3px 8px;font-size:11px">↓</button><button onclick="deleteFile(this)" class="btn" style="padding:3px 8px;font-size:11px;background:#ef4444">✕</button></td>`;
    tb.prepend(tr);
  });
}
function fileSearch(q){document.querySelectorAll('#fileBody tr').forEach(r=>r.style.display=r.textContent.toLowerCase().includes(q.toLowerCase())?'':'none');}
function sortFiles(){}
function deleteFile(btn){if(confirm('Delete this file?'))btn.closest('tr').remove();}
function downloadFile(name){alert('Downloading: '+name);}
function shareFile(name){const link=window.location.href+'?share='+encodeURIComponent(name);navigator.clipboard.writeText(link).then(()=>alert('Link copied!')).catch(()=>prompt('Copy link:',link));}
function toggleAll(cb){document.querySelectorAll('.rowCheck').forEach(c=>c.checked=cb.checked);}
function deleteSelected(){if(!confirm('Delete selected files?'))return;document.querySelectorAll('.rowCheck:checked').forEach(cb=>cb.closest('tr').remove());}
"##);
    }

    if has_list {
        js.push_str(r##"
function filterCards(){
  const q=(document.getElementById('lSearch')||{value:''}).value.toLowerCase();
  const f=(document.getElementById('lType')||{value:''}).value.toLowerCase();
  document.querySelectorAll('#cardGrid .listing-card').forEach(c=>{
    const text=(c.dataset.q||c.textContent).toLowerCase();
    c.style.display=(!q||text.includes(q))&&(!f||text.includes(f))?'':'none';
  });
}
function viewDetail(id){alert('Item '+id+' details\n\nIn a real app this would open a full detail page or modal.');}
"##);
    }

    if has_pric {
        js.push_str(r##"
function choosePlan(name){alert('You selected the '+name+' plan!\nIn a real app this would redirect to secure checkout.');}
"##);
    }

    if has_bkng {
        js.push_str(r##"
function bookSlot(e,f){
  e.preventDefault();
  const d=document.getElementById('bdate'),ti=document.getElementById('btime');
  const msg=document.getElementById('fmsg');
  if(msg){msg.textContent='✓ Booking confirmed for '+(d?d.value:'')+' at '+(ti?ti.value:'')+'. Check your email!';msg.style.color='#10b981';}
  f.reset();return false;
}
"##);
    }

    js
}

const JS_SIMPLE: &str = r##"
function sub(e,f){
  e.preventDefault();
  const msg=document.getElementById('fmsg');
  if(msg){msg.textContent='✓ Message sent! We will get back to you soon.';msg.style.color='#10b981';}
  f.reset();
  return false;
}
"##;

const JS_RESTAURANT: &str = r##"
const MENU=[
  {id:1,n:'Caesar Salad',s:'Fresh romaine, croutons, parmesan',p:8.99,c:'starters',i:'🥗'},
  {id:2,n:'Garlic Bread',s:'Toasted with herb butter',p:4.99,c:'starters',i:'🍞'},
  {id:3,n:'Spring Rolls',s:'Crispy, with sweet chili dip',p:6.99,c:'starters',i:'🥚'},
  {id:4,n:'Grilled Salmon',s:'Lemon butter & asparagus',p:22.99,c:'mains',i:'🐟'},
  {id:5,n:'BBQ Chicken',s:'Slow-cooked with house sauce',p:18.99,c:'mains',i:'🍗'},
  {id:6,n:'Pasta Carbonara',s:'Creamy, bacon & parmesan',p:16.99,c:'mains',i:'🍝'},
  {id:7,n:'Veggie Burger',s:'Beyond patty, avocado, fries',p:14.99,c:'mains',i:'🍔'},
  {id:8,n:'Choc Lava Cake',s:'Warm cake, vanilla ice cream',p:7.99,c:'desserts',i:'🍫'},
  {id:9,n:'NY Cheesecake',s:'Classic style, berry coulis',p:6.99,c:'desserts',i:'🍰'},
  {id:10,n:'Tiramisu',s:'Italian classic, espresso soaked',p:7.49,c:'desserts',i:'☕'},
  {id:11,n:'Fresh Lemonade',s:'Mint & basil, sparkling',p:3.99,c:'drinks',i:'🍋'},
  {id:12,n:'Cold Brew Coffee',s:'Oat milk, house blend',p:4.49,c:'drinks',i:'☕'},
  {id:13,n:'Mango Smoothie',s:'Tropical blend, fresh fruit',p:5.49,c:'drinks',i:'🥭'},
];
let cat='all',q='',cart=JSON.parse(localStorage.getItem('rest-cart')||'[]');
function filterMenu(){q=document.getElementById('search').value.toLowerCase();render();}
function setCat(c,el){cat=c;document.querySelectorAll('.cat').forEach(b=>b.classList.remove('active'));el.classList.add('active');render();}
function render(){
  const f=MENU.filter(p=>(cat==='all'||p.c===cat)&&(p.n.toLowerCase().includes(q)||p.s.toLowerCase().includes(q)));
  document.getElementById('mcnt').textContent=f.length+' items';
  document.getElementById('grid').innerHTML=f.map(p=>`
<div class="prod-card">
  <div class="prod-icon">${p.i}</div>
  <div class="prod-name">${p.n}</div>
  <div class="prod-sub">${p.s}</div>
  <div class="prod-price">$${p.p.toFixed(2)}</div>
  <button class="add-btn" onclick="addCart(${p.id})">+ Add to Order</button>
</div>`).join('');
}
function addCart(id){
  const p=MENU.find(x=>x.id===id);if(!p)return;
  const ex=cart.find(x=>x.id===id);
  if(ex)ex.qty++;else cart.push({...p,qty:1});
  save();updateCart();
}
function changeQty(id,d){
  const i=cart.findIndex(x=>x.id===id);if(i<0)return;
  cart[i].qty+=d;if(cart[i].qty<=0)cart.splice(i,1);
  save();updateCart();
}
function save(){localStorage.setItem('rest-cart',JSON.stringify(cart));}
function updateCart(){
  document.getElementById('cnt').textContent=cart.reduce((s,x)=>s+x.qty,0);
  document.getElementById('ctotal').textContent='$'+cart.reduce((s,x)=>s+x.p*x.qty,0).toFixed(2);
  document.getElementById('citems').innerHTML=cart.length
    ?cart.map(x=>`<div class="citem"><div style="flex:1"><div class="cname">${x.i} ${x.n}</div><div class="qbtns"><button class="qb" onclick="changeQty(${x.id},-1)">−</button><span style="font-size:13px;min-width:20px;text-align:center">${x.qty}</span><button class="qb" onclick="changeQty(${x.id},1)">+</button></div></div><div class="cprice">$${(x.p*x.qty).toFixed(2)}</div></div>`).join('')
    :'<p style="text-align:center;color:#94a3b8;padding:24px 0">No items yet</p>';
}
function toggleCart(){document.getElementById('cart').classList.toggle('open');document.getElementById('ov').classList.toggle('open');}
function checkout(){
  if(!cart.length){alert('Add items to your order first.');return;}
  const t=cart.reduce((s,x)=>s+x.p*x.qty,0);
  cart=[];save();updateCart();toggleCart();
  alert('Order placed! Total: $'+t.toFixed(2)+'\nEstimated time: 25-40 min. Enjoy!');
}
function sub(e,f){e.preventDefault();const m=document.getElementById('fmsg');if(m)m.textContent='Table booked! We will confirm shortly.';f.reset();return false;}
render();updateCart();
"##;

const JS_STUDENT: &str = r##"
const ASGN=[
  {s:'Data Structures',t:'Binary Tree Implementation',d:'2025-05-18',p:'high'},
  {s:'Web Development',t:'React Portfolio Project',d:'2025-05-20',p:'high'},
  {s:'Linear Algebra',t:'Chapter 8 Problem Set',d:'2025-05-22',p:'medium'},
  {s:'English Comp',t:'Essay: Modern Technology',d:'2025-05-25',p:'low'},
  {s:'Physics Lab',t:'Lab Report: Wave Motion',d:'2025-05-28',p:'medium'},
];
const priSt={high:'background:#fee2e2;color:#991b1b',medium:'background:#fef9c3;color:#854d0e',low:'background:#dcfce7;color:#166534'};
const asgnEl=document.getElementById('asgn-list');
if(asgnEl)asgnEl.innerHTML=ASGN.map(a=>`<div class="card">
  <div style="display:flex;justify-content:space-between;align-items:flex-start;margin-bottom:8px">
    <h3 style="font-size:14px">${a.t}</h3>
    <span style="${priSt[a.p]};padding:2px 9px;border-radius:10px;font-size:11px;margin-left:8px">${a.p}</span>
  </div>
  <p style="color:#64748b;font-size:13px;margin-bottom:8px">📘 ${a.s}</p>
  <div class="tags sm"><span>📅 Due: ${a.d}</span></div>
</div>`).join('');
function sub(e,f){e.preventDefault();const m=document.getElementById('fmsg');if(m)m.textContent='Request sent to academic advisor!';f.reset();return false;}
"##;

const JS_QUIZ: &str = r##"
const QS=[
  {q:'What does HTML stand for?',a:['HyperText Markup Language','High Tech Modern Language','Hyper Transfer Markup Link','Home Tool Markup Language'],c:0},
  {q:'Which language runs natively in the browser?',a:['Python','Java','JavaScript','C++'],c:2},
  {q:'What does CSS stand for?',a:['Computer Style Sheets','Creative Style System','Cascading Style Sheets','Colorful Style Sheets'],c:2},
  {q:'Which tag makes the largest heading?',a:['<h6>','<head>','<h1>','<heading>'],c:2},
  {q:'What does API stand for?',a:['Application Programming Interface','Advanced Program Integration','Automated Process Interface','Application Protocol Information'],c:0},
  {q:'Which is a JavaScript framework?',a:['Django','Laravel','Spring','React'],c:3},
  {q:'What does DOM stand for?',a:['Document Object Model','Data Object Management','Digital Output Mode','Document Order Manager'],c:0},
  {q:'Which HTTP method retrieves data?',a:['POST','PUT','DELETE','GET'],c:3},
  {q:'What does SQL stand for?',a:['Structured Query Language','Simple Queue Language','Server Query Logic','Standard Question List'],c:0},
  {q:'What is a closure in JavaScript?',a:['A self-closing loop','A function accessing its outer scope','A closed HTML tag','A method returning null'],c:1},
];
let cur=0,score=0,tmr=null,tLeft=30,done=false;
function startQuiz(){cur=0;score=0;showQ();document.getElementById('start-screen').style.display='none';document.getElementById('quiz-screen').style.display='block';}
function showQ(){
  done=false;document.getElementById('nxt').style.display='none';
  const p=QS[cur];
  document.getElementById('qnum').textContent='Q '+(cur+1)+' / '+QS.length;
  document.getElementById('score-live').textContent='Score: '+score;
  document.getElementById('prog').style.width=((cur/QS.length)*100)+'%';
  document.getElementById('qtext').textContent=p.q;
  document.getElementById('opts').innerHTML=p.a.map((a,i)=>`<button onclick="pick(${i})" style="padding:12px 14px;border:2px solid #e2e8f0;border-radius:8px;cursor:pointer;text-align:left;font-size:13px;font-family:inherit;background:#fff;line-height:1.4;transition:background .15s,border-color .15s">${a}</button>`).join('');
  tick();
}
function tick(){clearInterval(tmr);tLeft=30;upTimer();tmr=setInterval(()=>{tLeft--;upTimer();if(tLeft<=0){clearInterval(tmr);if(!done)timeUp();}},1000);}
function upTimer(){document.getElementById('timer').textContent='⏱ '+tLeft+'s';}
function timeUp(){done=true;markAnswers(-1);document.getElementById('nxt').style.display='inline-block';}
function pick(i){if(done)return;done=true;clearInterval(tmr);if(i===QS[cur].c)score++;markAnswers(i);document.getElementById('nxt').style.display='inline-block';}
function markAnswers(chosen){
  document.getElementById('opts').querySelectorAll('button').forEach((b,i)=>{
    b.disabled=true;
    if(i===QS[cur].c){b.style.background='#10b981';b.style.color='#fff';b.style.borderColor='#10b981';}
    else if(i===chosen){b.style.background='#ef4444';b.style.color='#fff';b.style.borderColor='#ef4444';}
  });
}
function nextQ(){cur++;if(cur<QS.length)showQ();else showResult();}
function showResult(){
  clearInterval(tmr);
  document.getElementById('quiz-screen').style.display='none';
  document.getElementById('result-screen').style.display='block';
  const pct=Math.round((score/QS.length)*100);
  document.getElementById('final-score').textContent=score+'/'+QS.length+' ('+pct+'%)';
  const icon=pct>=80?'🏆':pct>=60?'👍':'📚';
  const msg=pct>=80?'Excellent! You are a master!':pct>=60?'Good job! Keep it up!':'Keep studying, you will get there!';
  document.getElementById('grade-icon').textContent=icon;
  document.getElementById('grade-msg').textContent=msg;
}
function restartQuiz(){document.getElementById('result-screen').style.display='none';document.getElementById('start-screen').style.display='block';}
"##;

const JS_TODO: &str = r##"
let notes=JSON.parse(localStorage.getItem('syn-notes')||'[]');
let todos=JSON.parse(localStorage.getItem('syn-todos')||'[]');
let tFilter='all';
const tagBg={general:'#e0e7ff',work:'#fef9c3',personal:'#fce7f3',ideas:'#dcfce7'};
const tagFg={general:'#3730a3',work:'#854d0e',personal:'#9d174d',ideas:'#166534'};
const tagIco={general:'📌',work:'💼',personal:'👤',ideas:'💡'};
const priBg={low:'#dcfce7',medium:'#fef9c3',high:'#fee2e2'};
const priFg={low:'#166534',medium:'#854d0e',high:'#991b1b'};
function setView(v){
  document.getElementById('view-notes').style.display=v==='notes'?'block':'none';
  document.getElementById('view-todos').style.display=v==='todos'?'block':'none';
}
function addNote(){
  const title=document.getElementById('note-title').value.trim();
  const body=document.getElementById('note-body').value.trim();
  if(!title)return;
  const tag=document.getElementById('note-tag').value;
  notes.unshift({id:Date.now(),title,body,tag,date:new Date().toLocaleDateString()});
  saveAll();renderNotes();
  document.getElementById('note-title').value='';document.getElementById('note-body').value='';
}
function delNote(id){notes=notes.filter(n=>n.id!==id);saveAll();renderNotes();}
function renderNotes(){
  document.getElementById('notes-list').innerHTML=notes.length
    ?notes.map(n=>`<div class="card" style="position:relative">
  <div style="position:absolute;top:12px;right:12px;background:${tagBg[n.tag]};color:${tagFg[n.tag]};padding:2px 9px;border-radius:10px;font-size:11px">${tagIco[n.tag]} ${n.tag}</div>
  <h3 style="padding-right:80px;font-size:15px;margin-bottom:8px">${n.title}</h3>
  <p style="color:#64748b;font-size:13px;white-space:pre-wrap">${n.body||'No content.'}</p>
  <div style="display:flex;justify-content:space-between;margin-top:12px;font-size:11px;color:#94a3b8"><span>${n.date}</span><button onclick="delNote(${n.id})" style="background:none;border:none;cursor:pointer;color:#ef4444;font-size:12px">🗑 Delete</button></div>
</div>`).join('')
    :'<div class="card" style="text-align:center;padding:36px;color:#94a3b8"><p>No notes yet. Add your first note above!</p></div>';
}
function addTodo(){
  const txt=document.getElementById('todo-input').value.trim();if(!txt)return;
  const pri=document.getElementById('todo-pri').value;
  todos.unshift({id:Date.now(),text:txt,pri,done:false,date:new Date().toLocaleDateString()});
  saveAll();renderTodos();document.getElementById('todo-input').value='';
}
function toggleTodo(id){const t=todos.find(x=>x.id===id);if(t){t.done=!t.done;saveAll();renderTodos();}}
function delTodo(id){todos=todos.filter(t=>t.id!==id);saveAll();renderTodos();}
function filterTodos(f,el){tFilter=f;document.querySelectorAll('[id^="tf-"]').forEach(b=>b.classList.remove('active'));el.classList.add('active');renderTodos();}
function renderTodos(){
  const list=todos.filter(t=>tFilter==='all'||(tFilter==='done')===t.done);
  const cntEl=document.getElementById('task-count');
  if(cntEl)cntEl.textContent=todos.filter(t=>!t.done).length+' tasks';
  document.getElementById('todo-list').innerHTML=list.length
    ?list.map(t=>`<div style="display:flex;align-items:center;gap:12px;padding:12px 0;border-bottom:1px solid #f1f5f9">
  <input type="checkbox" ${t.done?'checked':''} onchange="toggleTodo(${t.id})" style="width:18px;height:18px;cursor:pointer;accent-color:#6366f1">
  <div style="flex:1"><div style="font-size:14px;font-weight:500;${t.done?'text-decoration:line-through;color:#94a3b8':''}">${t.text}</div><div style="font-size:11px;color:#94a3b8;margin-top:2px">${t.date}</div></div>
  <span style="background:${priBg[t.pri]};color:${priFg[t.pri]};padding:2px 9px;border-radius:10px;font-size:11px">${t.pri}</span>
  <button onclick="delTodo(${t.id})" style="background:none;border:none;cursor:pointer;color:#ef4444">🗑</button>
</div>`).join('')
    :'<p style="text-align:center;color:#94a3b8;padding:24px 0">No tasks here!</p>';
}
function saveAll(){localStorage.setItem('syn-notes',JSON.stringify(notes));localStorage.setItem('syn-todos',JSON.stringify(todos));}
renderNotes();renderTodos();
"##;

const JS_CHATBOT: &str = r##"
const REPLIES=[
  ['hello','hi','hey','greet'],
  ['help','can you','support','assist'],
  ['feature','capabilit','what do you','tell me about yourself'],
  ['fact','interesting','did you know','surprise'],
  ['thank','great','awesome','perfect','nice'],
  ['bye','goodbye','see you','later'],
];
const ANSWERS=[
  ['👋 Hello! Great to see you. How can I assist you today?','Hey there! I\'m here and ready to help.'],
  ['I can help with answering questions, brainstorming ideas, writing, and much more! What do you need?','Sure! Tell me more about what you\'re trying to accomplish.'],
  ['I\'m an AI assistant built to help you think, write, and solve problems. Ask me anything!','My capabilities include answering questions, summarizing content, and generating ideas. What\'s on your mind?'],
  ['Did you know honey never spoils? Archaeologists found 3000-year-old honey in Egyptian tombs and it was still edible!','Fun fact: A group of flamingos is called a "flamboyance." 🦩'],
  ['You\'re welcome! 😊 Is there anything else I can help with?','Happy to help! Let me know if you need anything else.'],
  ['Goodbye! 👋 Come back anytime — I\'m always here to help.','See you later! Have a great day! 😊'],
];
const FALLBACK=[
  'That\'s an interesting question! Let me think... I\'d say the answer depends on context. Can you tell me more?',
  'Great question! I\'m not 100% certain, but here\'s my best take: every problem has a creative solution.',
  'I appreciate you asking! Based on what I know, I\'d recommend exploring multiple perspectives on this.',
  'Hmm, that\'s something worth exploring. Could you elaborate a bit more so I can give a better answer?',
  'Interesting! I think the key insight here is to approach it step by step. What aspect interests you most?',
];
let history=JSON.parse(localStorage.getItem('chat-history')||'[]');
function ts(){return new Date().toLocaleTimeString([],{hour:'2-digit',minute:'2-digit'});}
function addMsg(text,role){
  const div=document.createElement('div');
  div.className='msg '+role;
  div.innerHTML=text+'<div class="msg-meta">'+ts()+'</div>';
  document.getElementById('messages').appendChild(div);
  document.getElementById('messages').scrollTop=9999;
  history.push({role,text,time:ts()});
  localStorage.setItem('chat-history',JSON.stringify(history.slice(-60)));
}
function showTyping(){
  const t=document.createElement('div');t.className='typing';t.id='typing';
  t.innerHTML='<span></span><span></span><span></span>';
  document.getElementById('messages').appendChild(t);
  document.getElementById('messages').scrollTop=9999;
}
function hideTyping(){const t=document.getElementById('typing');if(t)t.remove();}
function getBotReply(msg){
  const m=msg.toLowerCase();
  for(let i=0;i<REPLIES.length;i++){
    if(REPLIES[i].some(k=>m.includes(k))){
      const set=ANSWERS[i];return set[Math.floor(Math.random()*set.length)];
    }
  }
  return FALLBACK[Math.floor(Math.random()*FALLBACK.length)];
}
function sendMsg(text){
  const inp=document.getElementById('msg-input');
  const msg=(text||inp.value).trim();
  if(!msg)return;
  inp.value='';
  addMsg(msg,'user');
  showTyping();
  setTimeout(()=>{hideTyping();addMsg(getBotReply(msg),'bot');},700+Math.random()*600);
}
function clearChat(){
  document.getElementById('messages').innerHTML='';
  history=[];localStorage.removeItem('chat-history');
  setTimeout(()=>addMsg('👋 Hi! I\'m your AI assistant. How can I help you today?','bot'),300);
}
function loadHistory(){
  document.getElementById('messages').innerHTML='';
  if(!history.length){addMsg('No chat history found.','bot');return;}
  history.forEach(h=>addMsg(h.text,h.role));
}
window.addEventListener('load',()=>{
  if(history.length)history.forEach(h=>addMsg(h.text,h.role));
  else addMsg('👋 Hi! I\'m your AI assistant. Ask me anything or pick a quick prompt from the sidebar.','bot');
});
"##;

const JS_ADMIN: &str = r##"
const ORDERS=[
  {id:'#1042',cust:'Alice Johnson',prod:'Analytics Pro',amt:'$129',status:'Completed',date:'2025-05-14'},
  {id:'#1041',cust:'Bob Smith',    prod:'Starter Plan', amt:'$49', status:'Pending',   date:'2025-05-14'},
  {id:'#1040',cust:'Carol White',  prod:'Team Bundle',  amt:'$299',status:'Completed',date:'2025-05-13'},
  {id:'#1039',cust:'David Lee',    prod:'Analytics Pro',amt:'$129',status:'Refunded',  date:'2025-05-13'},
  {id:'#1038',cust:'Eva Brown',    prod:'Enterprise',   amt:'$799',status:'Completed',date:'2025-05-12'},
  {id:'#1037',cust:'Frank Garcia', prod:'Starter Plan', amt:'$49', status:'Pending',   date:'2025-05-12'},
  {id:'#1036',cust:'Grace Kim',    prod:'Team Bundle',  amt:'$299',status:'Completed',date:'2025-05-11'},
  {id:'#1035',cust:'Henry Patel',  prod:'Analytics Pro',amt:'$129',status:'Completed',date:'2025-05-11'},
];
const REV=[42,58,75,61,89,104,97];
const DAYS=['Mon','Tue','Wed','Thu','Fri','Sat','Sun'];
const ACTS=[
  {t:'New user registered',d:'2 min ago',i:'👤'},
  {t:'Order #1042 completed',d:'15 min ago',i:'✅'},
  {t:'Support ticket opened',d:'32 min ago',i:'🎫'},
  {t:'New subscription: Enterprise',d:'1h ago',i:'💰'},
  {t:'Server backup completed',d:'2h ago',i:'🔒'},
  {t:'Analytics report exported',d:'3h ago',i:'📊'},
];
const stColors={Completed:'background:#dcfce7;color:#166534',Pending:'background:#fef9c3;color:#854d0e',Refunded:'background:#fee2e2;color:#991b1b'};
let allOrders=[...ORDERS],query='';
function renderChart(){
  const mx=Math.max(...REV);
  document.getElementById('rev-chart').innerHTML=REV.map((v,i)=>`<div class="bar-wrap"><div class="bar" style="height:${Math.round((v/mx)*100)}px" title="$${v}k"></div><div class="bar-lbl">${DAYS[i]}</div></div>`).join('');
}
function renderActivity(){
  document.getElementById('activity-feed').innerHTML=ACTS.map(a=>`<div class="act-item"><div class="act-dot"></div><div><div style="font-weight:600">${a.i} ${a.t}</div><div style="color:#94a3b8">${a.d}</div></div></div>`).join('');
}
function renderTable(rows){
  document.getElementById('tbl-body').innerHTML=rows.map(o=>`<tr><td>${o.id}</td><td>${o.cust}</td><td>${o.prod}</td><td style="font-weight:700;color:#6366f1">${o.amt}</td><td><span style="${stColors[o.status]||''};padding:2px 9px;border-radius:10px;font-size:11px">${o.status}</span></td><td>${o.date}</td></tr>`).join('');
}
function filterTable(){
  query=document.getElementById('tbl-search').value.toLowerCase();
  renderTable(allOrders.filter(o=>Object.values(o).join(' ').toLowerCase().includes(query)));
}
function exportCSV(){
  const rows=[Object.keys(ORDERS[0]).join(','),...ORDERS.map(o=>Object.values(o).join(','))].join('\n');
  const a=document.createElement('a');a.href='data:text/csv;charset=utf-8,'+encodeURIComponent(rows);a.download='orders.csv';a.click();
}
function showPanel(id,el){
  document.querySelectorAll('.panel').forEach(p=>p.style.display='none');
  document.querySelectorAll('.anav').forEach(a=>a.classList.remove('active'));
  document.getElementById('panel-'+id).style.display='block';
  el.classList.add('active');
  document.getElementById('panel-title').textContent=el.textContent.trim().replace(/^./,'').trim();
}
renderChart();renderActivity();renderTable(ORDERS);
"##;

const JS_FILES: &str = r##"
let FILES=[
  {id:1,name:'Q1_Report_2025.pdf',  cat:'Document',   owner:'Alice',size:'2.4 MB',status:'active',  date:'2025-05-14'},
  {id:2,name:'product_photo.jpg',   cat:'Image',      owner:'Bob',  size:'840 KB',status:'active',  date:'2025-05-14'},
  {id:3,name:'budget_v3.xlsx',      cat:'Spreadsheet',owner:'Carol',size:'1.1 MB',status:'review',  date:'2025-05-13'},
  {id:4,name:'contract_2024.pdf',   cat:'Document',   owner:'David',size:'512 KB',status:'archived',date:'2025-05-12'},
  {id:5,name:'logo_final.png',      cat:'Image',      owner:'Eva',  size:'380 KB',status:'active',  date:'2025-05-12'},
  {id:6,name:'server_backup.zip',   cat:'Archive',    owner:'Frank',size:'88 MB', status:'review',  date:'2025-05-11'},
  {id:7,name:'client_notes.docx',   cat:'Document',   owner:'Grace',size:'220 KB',status:'active',  date:'2025-05-10'},
  {id:8,name:'analytics_export.csv',cat:'Spreadsheet',owner:'Henry',size:'640 KB',status:'archived',date:'2025-05-09'},
];
const TIMELINE=[
  {t:'Q1_Report_2025.pdf uploaded',d:'Today 14:23',i:'📤'},
  {t:'budget_v3.xlsx sent for review',d:'Today 11:05',i:'🔄'},
  {t:'contract_2024.pdf archived',d:'Yesterday 16:40',i:'📦'},
  {t:'server_backup.zip marked for review',d:'Yesterday 09:12',i:'🔄'},
  {t:'analytics_export.csv archived',d:'May 9, 10:33',i:'📦'},
];
const stCol={active:'background:#dcfce7;color:#166534',review:'background:#fef9c3;color:#854d0e',archived:'background:#e2e8f0;color:#475569'};
const stLbl={active:'✅ Active',review:'🔄 In Review',archived:'📦 Archived'};
let catFilter='all',searchQ='';
function showUpload(){document.getElementById('upload-modal').style.display='block';}
function hideUpload(){document.getElementById('upload-modal').style.display='none';}
function uploadFile(){
  const name=document.getElementById('up-name').value.trim();
  const cat=document.getElementById('up-cat').value;
  if(!name)return;
  FILES.unshift({id:Date.now(),name,cat,owner:'You',size:'—',status:'active',date:new Date().toLocaleDateString()});
  TIMELINE.unshift({t:name+' uploaded',d:'Just now',i:'📤'});
  hideUpload();document.getElementById('up-name').value='';
  renderAll();
}
function setCatFilter(f,el){catFilter=f;document.querySelectorAll('.cat').forEach(b=>b.classList.remove('active'));el.classList.add('active');renderAll();}
function filterFiles(){searchQ=document.getElementById('file-search').value.toLowerCase();renderAll();}
function renderAll(){
  const f=FILES.filter(x=>(catFilter==='all'||x.status===catFilter)&&(x.name.toLowerCase().includes(searchQ)||x.cat.toLowerCase().includes(searchQ)));
  document.getElementById('fc-total').textContent=FILES.length;
  document.getElementById('fc-active').textContent=FILES.filter(x=>x.status==='active').length;
  document.getElementById('fc-review').textContent=FILES.filter(x=>x.status==='review').length;
  document.getElementById('fc-archived').textContent=FILES.filter(x=>x.status==='archived').length;
  document.getElementById('files-tbl').innerHTML=f.map(x=>`<tr>
    <td>📄 ${x.name}</td><td>${x.cat}</td><td>${x.owner}</td><td>${x.size}</td>
    <td><span style="${stCol[x.status]};padding:2px 9px;border-radius:10px;font-size:11px">${stLbl[x.status]}</span></td>
    <td>${x.date}</td>
    <td><button onclick="changeStatus(${x.id})" style="background:none;border:1px solid #e2e8f0;padding:4px 10px;border-radius:6px;cursor:pointer;font-size:12px">Change Status</button></td>
  </tr>`).join('');
  document.getElementById('file-timeline').innerHTML=TIMELINE.map(a=>`<div class="act-item"><div class="act-dot"></div><div><div style="font-weight:600">${a.i} ${a.t}</div><div style="color:#94a3b8">${a.d}</div></div></div>`).join('');
}
function changeStatus(id){
  const f=FILES.find(x=>x.id===id);if(!f)return;
  const cycle={active:'review',review:'archived',archived:'active'};
  TIMELINE.unshift({t:f.name+' → '+stLbl[cycle[f.status]],d:'Just now',i:'🔄'});
  f.status=cycle[f.status];renderAll();
}
renderAll();
"##;

const JS_EXPENSE: &str = r##"
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
"##;
