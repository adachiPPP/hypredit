use adw::prelude::*;
use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, DrawingArea, Entry, Label, ListBox,
    Stack, Orientation, ScrolledWindow, TextView, WrapMode,
};
use adw::{ApplicationWindow as AdwApplicationWindow, HeaderBar, Toast, ToastOverlay};
use libadwaita as adw;
use serde_json::Value;
use std::cell::RefCell;
use std::process::Command;
use std::rc::Rc;
use std::fs::OpenOptions;
use std::time::{SystemTime, UNIX_EPOCH};
use include_dir::{include_dir, Dir};
use webkit6::WebView;
use webkit6::prelude::*;

const APP_ID: &str = "io.github.hypredit.Hypredit";
static WIKI: Dir = include_dir!("$CARGO_MANIFEST_DIR/wiki-content");

#[derive(Clone)]
struct PendingChange {
    key: String,
    value: String,
    lua: String,
}

#[derive(Clone)]
struct WikiOption {
    category: String,
    name: String,
    description: String,
    ty: String,
    default: String,
    limits: String,
}

#[derive(Clone)]
struct AppState {
    pending: Rc<RefCell<Vec<PendingChange>>>,
    undo: Rc<RefCell<Vec<String>>>,
    config_path: Rc<RefCell<String>>,
    profile_dir: Rc<RefCell<String>>,
}

impl AppState {
    fn new() -> Self {
        let home = std::env::var("HOME").unwrap_or_default();
        Self {
            pending: Rc::new(RefCell::new(Vec::new())),
            undo: Rc::new(RefCell::new(Vec::new())),
            config_path: Rc::new(RefCell::new(std::env::var("HYPREDIT_CONFIG_PATH").unwrap_or_else(|_| format!("{}/.config/hypr/hyprland.lua", home)))),
            profile_dir: Rc::new(RefCell::new(format!("{}/.config/hypredit/profiles", home))),
        }
    }
}

fn log_line(msg: &str) {
    let home = std::env::var("HOME").unwrap_or_default();
    let dir = format!("{}/.local/state/hypredit", home);
    let _ = std::fs::create_dir_all(&dir);
    let path = format!("{}/hypredit.log", dir);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        use std::io::Write;
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let _ = writeln!(f, "[{}] {}", ts, msg);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    let mut config_override: Option<String> = None;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print_help();
                return;
            }
            "-c" | "--config" => {
                if i + 1 < args.len() {
                    config_override = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "profile" => {
                handle_profile_cli(&args[i + 1..]);
                return;
            }
            _ => {}
        }
        i += 1;
    }
    if let Some(path) = config_override {
        unsafe { std::env::set_var("HYPREDIT_CONFIG_PATH", path); }
    }
    log_line("hypredit started");
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(|app| build_ui(app));
    app.run();
}

fn print_help() {
    println!("Hypredit - Hyprland configuration and runtime tool");
    println!();
    println!("Usage:");
    println!("  hypredit                     Launch the GTK application");
    println!("  hypredit -c, --config PATH   Use a different config file (default ~/.config/hypr/hyprland.lua)");
    println!("  hypredit profile list        List saved profiles");
    println!("  hypredit profile apply NAME  Apply a profile by name");
    println!("  hypredit profile next        Apply first profile alphabetically");
    println!("  hypredit profile previous    Apply last profile alphabetically");
    println!("  hypredit -h, --help          Show this help");
    println!();
    println!("Options include all documented Hyprland wiki variables plus monitors,");
    println!("keybinds, rules, autostart, environment, cursor, and animations.");
    println!("Log file: ~/.local/state/hypredit/hypredit.log");
}


fn handle_profile_cli(args: &[String]) {
    let home = std::env::var("HOME").unwrap_or_default();
    let cfg = format!("{}/.config/hypr/hyprland.lua", home);
    let prof_dir = format!("{}/.config/hypredit/profiles", home);
    let _ = std::fs::create_dir_all(&prof_dir);
    match args.first().map(String::as_str).unwrap_or("") {
        "list" => {
            println!("Profiles:");
            for e in std::fs::read_dir(&prof_dir).into_iter().flatten().flatten() {
                println!("  {}", e.file_name().to_string_lossy());
            }
        }
        "apply" => {
            let name = args.get(1).cloned().unwrap_or_default();
            let path = format!("{}/{}", prof_dir, name);
            let alt = format!("{}/{}.lua", prof_dir, name);
            let src = if std::path::Path::new(&path).exists() { path } else { alt };
            match std::fs::read_to_string(&src) {
                Ok(content) => {
                    let _ = std::fs::write(&cfg, content);
                    let _ = run_cmd("hyprctl", &["reload"]);
                    println!("Applied profile {}", name);
                }
                Err(e) => eprintln!("profile not found: {} ({})", src, e),
            }
        }
        "next" | "previous" | "prev" => {
            let mut names: Vec<String> = std::fs::read_dir(&prof_dir).into_iter().flatten().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
            names.sort();
            if names.is_empty() { eprintln!("no profiles"); return; }
            let target = if args[0] == "previous" || args[0] == "prev" { names.last().cloned().unwrap_or_default() } else { names.first().cloned().unwrap_or_default() };
            let content = std::fs::read_to_string(format!("{}/{}", prof_dir, target)).unwrap_or_default();
            let _ = std::fs::write(&cfg, content);
            let _ = run_cmd("hyprctl", &["reload"]);
            println!("Applied profile {}", target);
        }
        _ => eprintln!("usage: hypredit profile [list|apply <name>|next|previous]"),
    }
}

fn run_cmd(program: &str, args: &[&str]) -> (i32, String, String) {
    let res = match Command::new(program).args(args).output() {
        Ok(o) => (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).to_string(), String::from_utf8_lossy(&o.stderr).to_string()),
        Err(e) => (-1, String::new(), format!("spawn error: {}", e)),
    };
    log_line(&format!("{} {} => exit={} stdout={} stderr={}", program, args.join(" "), res.0, summarize(&res.1), summarize(&res.2)));
    res
}

fn summarize(s: &str) -> String {
    s.chars().take(120).collect()
}


fn hyprctl(args: &[&str]) -> String {
    let (code, out, err) = run_cmd("hyprctl", args);
    if !err.trim().is_empty() { format!("exit={code}\nSTDOUT:\n{out}\nSTDERR:\n{err}") } else { format!("exit={code}\n{out}") }
}

fn toast(overlay: &ToastOverlay, msg: &str) {
    overlay.add_toast(Toast::new(msg));
}

fn build_ui(app: &adw::Application) {
    let window = AdwApplicationWindow::builder().application(app).title("Hypredit").default_width(1180).default_height(820).build();
    let state = AppState::new();
    let toast_overlay = ToastOverlay::new();

    let display = gtk4::gdk::Display::default();
    if let Some(display) = display {
        let provider = gtk4::CssProvider::new();
        provider.load_from_data(
            "window { background-color: #111827; }
             .title-1 { font-weight: 800; font-size: 1.15rem; color: #f8fafc; }
             button { border-radius: 10px; padding: 6px 12px; background: #1f2937; color: #f8fafc; }
             button:hover { background: #2563eb; }
             entry { border-radius: 8px; background: #1f2937; color: #f8fafc; }
             textview { border-radius: 10px; background: #0b1220; color: #e2e8f0; }
             notebook > header { background: #111827; }
             notebook tab { padding: 8px; border-radius: 8px; }
"
        );
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
    let root = GtkBox::new(Orientation::Vertical, 0);
    root.set_vexpand(true);
    root.set_hexpand(true);
    let header = HeaderBar::new();
    header.set_title_widget(Some(&Label::new(Some("Hypredit — Hyprland control center"))));
    root.append(&header);
    let stack = Stack::new();
    stack.set_vexpand(true);
    stack.set_hexpand(true);
    stack.set_hhomogeneous(false);
    stack.set_vhomogeneous(false);

    let sidebar_box = GtkBox::new(Orientation::Vertical, 4);
    sidebar_box.set_margin_start(8);
    sidebar_box.set_margin_end(8);
    sidebar_box.set_margin_top(8);
    sidebar_box.set_margin_bottom(8);
    let nav_title = Label::new(Some("Settings"));
    nav_title.set_halign(gtk4::Align::Start);
    nav_title.add_css_class("title-1");
    sidebar_box.append(&nav_title);

    let mut add_page = |key: &str, label: &str, widget: &GtkBox, sidebar_box: &GtkBox, stack: &Stack| {
        stack.add_named(widget, Some(key));
        let btn = Button::with_label(label);
        btn.set_halign(gtk4::Align::Start);
        btn.set_hexpand(true);
        btn.add_css_class("flat");
        let key_owned = key.to_string();
        let stack_clone = stack.clone();
        btn.connect_clicked(move |_| {
            stack_clone.set_visible_child_name(&key_owned);
        });
        sidebar_box.append(&btn);
    };

    let start_page = page_start(stack.clone(), state.clone(), toast_overlay.clone());
    add_page("start", "Start here", &start_page, &sidebar_box, &stack);
    let live_page = page_live(state.clone(), toast_overlay.clone());
    add_page("live", "Live options", &live_page, &sidebar_box, &stack);
    let mon_page = page_monitors(state.clone(), toast_overlay.clone());
    add_page("monitors", "Monitors", &mon_page, &sidebar_box, &stack);
    let binds_page = page_binds(state.clone(), toast_overlay.clone());
    add_page("keybinds", "Keybinds", &binds_page, &sidebar_box, &stack);
    let rules_page = page_rules(state.clone(), toast_overlay.clone());
    add_page("rules", "Rules", &rules_page, &sidebar_box, &stack);
    let as_page = page_autostart_env(state.clone(), toast_overlay.clone());
    add_page("autostart", "Autostart and env", &as_page, &sidebar_box, &stack);
    let cursor_page = page_cursor(state.clone(), toast_overlay.clone());
    add_page("cursor", "Cursor", &cursor_page, &sidebar_box, &stack);
    let anim_page = page_animations(state.clone(), toast_overlay.clone());
    add_page("animations", "Animations", &anim_page, &sidebar_box, &stack);
    let config_page = page_config(state.clone(), toast_overlay.clone());
    add_page("config", "Config editor", &config_page, &sidebar_box, &stack);
    let prof_page = page_profiles(state.clone(), toast_overlay.clone());
    add_page("profiles", "Profiles", &prof_page, &sidebar_box, &stack);
    let pending_page = page_pending(state.clone(), toast_overlay.clone());
    add_page("pending", "Pending and undo", &pending_page, &sidebar_box, &stack);
    let search_page = page_search(state.clone(), toast_overlay.clone());
    add_page("search", "Search", &search_page, &sidebar_box, &stack);
    let console_page = page_console(state.clone(), toast_overlay.clone());
    add_page("console", "Console", &console_page, &sidebar_box, &stack);
    let wopt_page = page_wiki_options(state.clone(), toast_overlay.clone());
    add_page("wiki_options", "Wiki options", &wopt_page, &sidebar_box, &stack);
    let wbrowser_page = page_wiki_browser();
    add_page("wikibrowser", "Wiki browser", &wbrowser_page, &sidebar_box, &stack);
    let live_wiki_page = page_live_wiki();
    add_page("live_wiki", "Live website", &live_wiki_page, &sidebar_box, &stack);

    stack.set_visible_child_name("start");

    let sidebar_scroll = ScrolledWindow::builder().child(&sidebar_box).width_request(250).vexpand(true).hexpand(false).build();
    let layout_box = GtkBox::new(Orientation::Horizontal, 0);
    layout_box.set_vexpand(true);
    layout_box.set_hexpand(true);
    layout_box.append(&sidebar_scroll);
    layout_box.append(&stack);
    root.append(&layout_box);
    toast_overlay.set_halign(gtk4::Align::Fill);
    toast_overlay.set_valign(gtk4::Align::Fill);
    root.set_halign(gtk4::Align::Fill);
    root.set_valign(gtk4::Align::Fill);
    toast_overlay.set_child(Some(&root));
    window.set_content(Some(&toast_overlay));
    window.set_icon_name(Some("hypredit"));
    window.maximize();
    window.present();
}

fn extract_json(s: &str) -> String {
    let brace = s.find('{');
    let bracket = s.find('[');
    let choose_bracket = match (brace, bracket) {
        (Some(b), Some(r)) => r < b,
        (None, Some(_)) => true,
        _ => false,
    };
    if choose_bracket {
        if let Some(start) = s.find('[') {
            if let Some(end) = s.rfind(']') { return s[start..=end].to_string(); }
        }
    }
    if let Some(start) = s.find('{') {
        if let Some(end) = s.rfind('}') { return s[start..=end].to_string(); }
    }
    s.to_string()
}

fn to_lua_value(v: &str) -> String {
    let v = v.trim();
    if v == "true" || v == "false" { return v.to_string(); }
    if let Ok(n) = v.parse::<i64>() { return n.to_string(); }
    if let Ok(f) = v.parse::<f64>() { return f.to_string(); }
    format!("\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\""))
}

fn option_to_lua_config(name: &str, value: &str) -> String {
    let (cat, keypath) = if let Some((a, b)) = name.split_once(':') {
        (a.trim().to_string(), b.trim().replace(':', "."))
    } else if let Some((a, b)) = name.split_once('.') {
        (a.trim().to_string(), b.trim().to_string())
    } else {
        (String::new(), name.trim().to_string())
    };
    let keys: Vec<&str> = keypath.split('.').map(|x| x.trim()).filter(|x| !x.is_empty()).collect();
    let lua_val = to_lua_value(value);
    if cat.is_empty() {
        return format!("hl.config({{ general = {{ {} = {} }} }})", keys.join("."), lua_val);
    }
    let mut table = lua_val;
    for k in keys.iter().rev() {
        table = format!("{{ {} = {} }}", k, table);
    }
    format!("hl.config({{ {} = {} }})", cat, table)
}

#[allow(dead_code)]
fn lua_to_options(lua: &str) -> String {
    let chars: Vec<char> = lua.chars().collect();
    let n = chars.len();
    let mut i = 0;
    let mut stack: Vec<String> = Vec::new();
    let mut out = String::new();
    while i < n {
        if i + 1 < n && chars[i] == '-' && chars[i + 1] == '-' {
            while i < n && chars[i] != '\n' { i += 1; }
            continue;
        }
        let c = chars[i];
        if c == '{' { i += 1; continue; }
        if c == '}' { if !stack.is_empty() { stack.pop(); } i += 1; continue; }
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < n && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.') { i += 1; }
            let ident: String = chars[start..i].iter().collect();
            while i < n && chars[i].is_whitespace() { i += 1; }
            if i < n && chars[i] == '=' {
                i += 1;
                while i < n && chars[i].is_whitespace() { i += 1; }
                if i < n && chars[i] == '{' {
                    stack.push(ident);
                    i += 1;
                    continue;
                } else {
                    let mut val = String::new();
                    if i < n && chars[i] == '"' {
                        val.push(chars[i]); i += 1;
                        while i < n {
                            val.push(chars[i]);
                            if chars[i] == '\\' { i += 1; if i < n { val.push(chars[i]); } }
                            else if chars[i] == '"' { i += 1; break; }
                            i += 1;
                        }
                    } else {
                        while i < n && chars[i] != ',' && chars[i] != '}' && chars[i] != '\n' && chars[i] != ')' {
                            val.push(chars[i]); i += 1;
                        }
                    }
                    let val = val.trim().to_string();
                    if !val.is_empty() && !val.starts_with('{') {
                        let path = if stack.is_empty() { ident.clone() } else { format!("{}.{}", stack.join("."), ident) };
                        out.push_str(&format!("{} = {}\n", path, val));
                    }
                    while i < n && (chars[i] == ',' || chars[i].is_whitespace()) { i += 1; }
                    continue;
                }
            } else {
                continue;
            }
        } else {
            i += 1;
        }
    }
    if out.is_empty() { "No scalar options found.".to_string() } else { out }
}

fn record_undo(state: &AppState) {
    let path = state.config_path.borrow().clone();
    if let Ok(content) = std::fs::read_to_string(&path) {
        state.undo.borrow_mut().push(content);
    }
}

fn apply_lua(state: &AppState, toast_overlay: &ToastOverlay, lua: &str, record: bool) {
    if record { record_undo(state); }
    state.pending.borrow_mut().push(PendingChange { key: "eval".to_string(), value: lua.to_string(), lua: lua.to_string() });
    log_line(&format!("apply_lua: {}", lua));
    let out = hyprctl(&["eval", lua]);
    toast(toast_overlay, &format!("eval: {}", out.lines().next().unwrap_or("ok")));
}

fn append_config(state: &AppState, snippet: &str) {
    let path = state.config_path.borrow().clone();
    let mut cur = std::fs::read_to_string(&path).unwrap_or_default();
    if !cur.ends_with('\n') { cur.push('\n'); }
    cur.push_str(snippet);
    cur.push('\n');
    let _ = std::fs::write(&path, cur);
}

fn page_scaffold(title: &str) -> GtkBox {
    let root = GtkBox::new(Orientation::Vertical, 8);
    root.set_margin_start(10); root.set_margin_end(10); root.set_margin_top(10); root.set_margin_bottom(10);
    root.set_vexpand(true);
    root.set_hexpand(true);
    let l = Label::new(Some(title));
    l.set_halign(gtk4::Align::Start);
    root.append(&l);
    root
}

fn page_live(state: AppState, to: ToastOverlay) -> GtkBox {
    let root = page_scaffold("Live option (get/set)");
    let row = GtkBox::new(Orientation::Horizontal, 6);
    let opt = Entry::new(); opt.set_placeholder_text(Some("option, e.g. general:gaps_in")); opt.set_hexpand(true);
    let val = Entry::new(); val.set_placeholder_text(Some("value")); val.set_hexpand(true);
    let getb = Button::with_label("Get");
    let setb = Button::with_label("Set live");
    row.append(&opt); row.append(&val); row.append(&getb); row.append(&setb);
    root.append(&row);
    let res = TextView::new(); res.set_monospace(true); res.set_editable(false); res.set_wrap_mode(WrapMode::WordChar);
    let rbuf = res.buffer();
    let sw = ScrolledWindow::builder().child(&res).vexpand(true).hexpand(true).build();
    root.append(&sw);
    {
        let optc = opt.clone(); let rbufc = rbuf.clone();
        getb.connect_clicked(move |_| {
            let name = optc.text().to_string();
            let out = hyprctl(&["getoption", &name, "-j"]);
            let pretty = serde_json::from_str::<Value>(&extract_json(&out)).ok().and_then(|v| serde_json::to_string_pretty(&v).ok()).unwrap_or(out.clone());
            rbufc.set_text(&pretty);
        });
    }
    {
        let optc = opt.clone(); let valc = val.clone(); let st = state.clone(); let to2 = to.clone(); let rbufc = rbuf.clone();
        setb.connect_clicked(move |_| {
            let name = optc.text().to_string();
            let value = valc.text().to_string();
            let lua = option_to_lua_config(&name, &value);
            apply_lua(&st, &to2, &lua, true);
            append_config(&st, &lua);
            rbufc.set_text(&format!("applied: {}\n{}", lua, hyprctl(&["eval", &lua])));
        });
    }
    let el = Label::new(Some("hyprctl eval (Lua):"));
    el.set_halign(gtk4::Align::Start);
    root.append(&el);
    let ev = TextView::new(); ev.set_monospace(true);
    ev.buffer().set_text("hl.config({ general = { gaps_in = 0 } })");
    let esw = ScrolledWindow::builder().child(&ev).height_request(120).hexpand(true).build();
    root.append(&esw);
    let erow = GtkBox::new(Orientation::Horizontal, 6);
    let runb = Button::with_label("Run eval");
    let reloadb = Button::with_label("Reload config");
    erow.append(&runb); erow.append(&reloadb);
    root.append(&erow);
    {
        let evc = ev.clone(); let st = state.clone(); let to2 = to.clone(); let rbufc = rbuf.clone();
        runb.connect_clicked(move |_| {
            let buf = evc.buffer();
            let (s,e) = (buf.start_iter(), buf.end_iter());
            let code = buf.text(&s,&e,false).to_string();
            apply_lua(&st, &to2, &code, true);
            append_config(&st, &code);
            rbufc.set_text(&hyprctl(&["eval", &code]));
        });
    }
    {
        let to2 = to.clone(); let rbufc = rbuf.clone();
        reloadb.connect_clicked(move |_| {
            rbufc.set_text(&hyprctl(&["reload"]));
            toast(&to2, "config reloaded");
        });
    }
    root
}

fn page_monitors(state: AppState, to: ToastOverlay) -> GtkBox {
    let root = page_scaffold("Monitor layout");

    #[derive(Clone)]
    struct MonInfo {
        name: String,
        width: i64,
        height: i64,
        x: i64,
        y: i64,
        scale: f64,
        vrr: bool,
        rr: f64,
        modes: Vec<String>,
    }

    let monitors: std::rc::Rc<std::cell::RefCell<Vec<MonInfo>>> = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));

    let preview = DrawingArea::new();
    preview.set_content_width(760);
    preview.set_content_height(360);
    let monitors_for_draw = monitors.clone();
    preview.set_draw_func(move |_, cr, w, h| {
        cr.set_source_rgb(0.05, 0.08, 0.14);
        cr.rectangle(0.0, 0.0, w as f64, h as f64);
        cr.fill().ok();

        let mons = monitors_for_draw.borrow();
        if mons.is_empty() { return; }
        let mut min_x = i64::MAX;
        let mut min_y = i64::MAX;
        let mut max_x = i64::MIN;
        let mut max_y = i64::MIN;
        for m in mons.iter() {
            min_x = min_x.min(m.x);
            min_y = min_y.min(m.y);
            max_x = max_x.max(m.x + m.width);
            max_y = max_y.max(m.y + m.height);
        }
        let world_w = (max_x - min_x).max(1) as f64;
        let world_h = (max_y - min_y).max(1) as f64;
        let scale = ((w as f64 - 40.0) / world_w).min((h as f64 - 40.0) / world_h).max(0.05);
        let off_x = (w as f64 - world_w * scale) * 0.5 - (min_x as f64 * scale);
        let off_y = (h as f64 - world_h * scale) * 0.5 - (min_y as f64 * scale);

        for (idx, m) in mons.iter().enumerate() {
            let rx = (m.x as f64 * scale) + off_x;
            let ry = (m.y as f64 * scale) + off_y;
            let rw = m.width as f64 * scale;
            let rh = m.height as f64 * scale;
            if idx == 0 {
                cr.set_source_rgb(0.16, 0.36, 0.90);
            } else {
                cr.set_source_rgb(0.20, 0.55, 0.45);
            }
            cr.rectangle(rx, ry, rw, rh);
            cr.fill().ok();
            cr.set_source_rgb(0.0, 0.0, 0.0);
            cr.rectangle(rx, ry, rw, rh);
            cr.stroke().ok();
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.select_font_face("Sans", gtk4::cairo::FontSlant::Normal, gtk4::cairo::FontWeight::Bold);
            cr.set_font_size(12.0_f64.max(scale * 0.04 * m.width as f64).min(18.0));
            cr.move_to(rx + 6.0, ry + 18.0);
            let _ = cr.show_text(&m.name);
            cr.move_to(rx + 6.0, ry + 34.0);
            let _ = cr.show_text(&format!("{}x{}", m.width, m.height));
        }
    });
    let preview_sw = ScrolledWindow::builder().child(&preview).vexpand(true).hexpand(true).build();
    root.append(&preview_sw);

    let status = Label::new(Some("Detecting monitors..."));
    status.set_halign(gtk4::Align::Start);
    let refresh = Button::with_label("Detect monitors");
    root.append(&refresh);
    root.append(&status);

    let container = GtkBox::new(Orientation::Vertical, 8);
    let csw = ScrolledWindow::builder().child(&container).height_request(220).hexpand(true).build();
    root.append(&csw);

    let last_mode_strings: std::rc::Rc<std::cell::RefCell<Vec<String>>> = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));

    let load = {
        let container = container.clone();
        let monitors = monitors.clone();
        let preview = preview.clone();
        let state = state.clone();
        let to = to.clone();
        let last_mode_strings = last_mode_strings.clone();
        let status = status.clone();
        move || {
            while let Some(child) = container.first_child() { container.remove(&child); }
            let out = hyprctl(&["monitors", "-j"]);
            let json = extract_json(&out);
            let parsed: Value = match serde_json::from_str(&json) {
                Ok(v) => v,
                Err(e) => {
                    log_line(&format!("monitor json parse error: {}", e));
                    log_line(&format!("monitor json head: {}", summarize(&json)));
                    Value::Array(vec![])
                }
            };
            let mut collected = Vec::new();
            if let Some(arr) = parsed.as_array() {
                for m in arr {
                    let name = m.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    let width = m.get("width").and_then(|x| x.as_i64()).unwrap_or(1920);
                    let height = m.get("height").and_then(|x| x.as_i64()).unwrap_or(1080);
                    let x = m.get("x").and_then(|x| x.as_i64()).unwrap_or(0);
                    let y = m.get("y").and_then(|x| x.as_i64()).unwrap_or(0);
                    let scale = m.get("scale").and_then(|x| x.as_f64()).unwrap_or(1.0);
                    let vrr = m.get("vrr").and_then(|x| x.as_bool()).unwrap_or(false);
                    let rr = m.get("refreshRate").and_then(|x| x.as_f64()).unwrap_or(60.0);
                    let modes: Vec<String> = m.get("availableModes")
                        .and_then(|x| x.as_array())
                        .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                        .unwrap_or_default();

                    let group = GtkBox::new(Orientation::Vertical, 4);
                    group.set_margin_bottom(8);
                    let title = Label::new(Some(&format!("{}  {}x{} @ {:.2}Hz", name, width, height, rr)));
                    title.set_halign(gtk4::Align::Start);
                    group.append(&title);

                    let mode_box = GtkBox::new(Orientation::Horizontal, 6);
                    let mode_combo = gtk4::ComboBoxText::new();
                    for mode in &modes {
                        mode_combo.append_text(mode);
                    }
                    mode_combo.append_text(&format!("{}x{}@{:.2}", width, height, rr));
                    if !modes.is_empty() { mode_combo.set_active(Some(0)); }
                    let mode_entry = Entry::new();
                    mode_entry.set_text(&format!("{}x{}@{:.2}", width, height, rr));
                    mode_entry.set_hexpand(true);
                    {
                        let me = mode_entry.clone();
                        mode_combo.connect_changed(move |combo| {
                            if let Some(t) = combo.active_text() { me.set_text(&t); }
                        });
                    }
                    mode_box.append(&mode_combo);
                    mode_box.append(&mode_entry);
                    group.append(&mode_box);

                    let row = GtkBox::new(Orientation::Horizontal, 6);
                    let pos_entry = Entry::new(); pos_entry.set_text(&format!("{}x{}", x, y)); pos_entry.set_hexpand(true);
                    let scale_entry = Entry::new(); scale_entry.set_text(&format!("{}", scale)); scale_entry.set_width_chars(6);
                    let vrr_entry = Entry::new(); vrr_entry.set_text(if vrr { "1" } else { "0" }); vrr_entry.set_width_chars(4);
                    row.append(&Label::new(Some("position"))); row.append(&pos_entry);
                    row.append(&Label::new(Some("scale"))); row.append(&scale_entry);
                    row.append(&Label::new(Some("vrr"))); row.append(&vrr_entry);
                    group.append(&row);

                    let apply = Button::with_label("Apply this monitor");
                    let n2 = name.clone();
                    let me = mode_entry.clone();
                    let pe = pos_entry.clone();
                    let se = scale_entry.clone();
                    let ve = vrr_entry.clone();
                    let st2 = state.clone();
                    let to2 = to.clone();
                    apply.connect_clicked(move |_| {
                        let mode = me.text().to_string();
                        let pos = pe.text().to_string();
                        let scale: f64 = se.text().parse().unwrap_or(1.0);
                        let vrr: i32 = ve.text().parse().unwrap_or(0);
                        let lua = format!(
                            "hl.monitor({{ output = \"{}\", mode = \"{}\", position = \"{}\", scale = {}, vrr = {} }})",
                            n2, mode, pos, scale, vrr
                        );
                        apply_lua(&st2, &to2, &lua, true);
                        append_config(&st2, &lua);
                        let _ = hyprctl(&["reload"]);
                    });
                    group.append(&apply);

                    collected.push(MonInfo {
                        name,
                        width,
                        height,
                        x,
                        y,
                        scale,
                        vrr,
                        rr,
                        modes,
                    });

                    container.append(&group);
                }
            }
            log_line(&format!("monitors detected: {}", collected.len()));
            status.set_text(&format!("{} monitor(s) detected", collected.len()));
            *monitors.borrow_mut() = collected;
            last_mode_strings.borrow_mut().clear();
            preview.queue_draw();
        }
    };
    load();
    {
        let cb = load.clone();
        refresh.connect_clicked(move |_| cb());
    }
    root
}

fn page_binds(state: AppState, to: ToastOverlay) -> GtkBox {
    let root = page_scaffold("Keybinds");
    let refresh = Button::with_label("Refresh binds");
    root.append(&refresh);
    let view = TextView::new(); view.set_monospace(true); view.set_editable(false);
    let vbuf = view.buffer();
    let sw = ScrolledWindow::builder().child(&view).vexpand(true).hexpand(true).build();
    root.append(&sw);
    let form = GtkBox::new(Orientation::Horizontal, 6);
    let key_entry = Entry::new(); key_entry.set_placeholder_text(Some("key combo, e.g. SUPER + A")); key_entry.set_hexpand(true);
    let cmd_entry = Entry::new(); cmd_entry.set_placeholder_text(Some("command")); cmd_entry.set_hexpand(true);
    let addb = Button::with_label("Add bind");
    form.append(&key_entry); form.append(&cmd_entry); form.append(&addb);
    root.append(&form);

    let refresh_fn = {
        let vbuf = vbuf.clone();
        move || {
            let out = hyprctl(&["binds", "-j"]);
            let pretty = serde_json::from_str::<Value>(&extract_json(&out)).ok().and_then(|v| serde_json::to_string_pretty(&v).ok()).unwrap_or(out);
            let trimmed: String = pretty.lines().take(300).collect::<Vec<_>>().join("\n");
            vbuf.set_text(&trimmed);
        }
    };
    refresh_fn();
    {
        let rf = refresh_fn.clone();
        refresh.connect_clicked(move |_| rf());
    }
    {
        let ke = key_entry.clone(); let ce = cmd_entry.clone(); let st = state.clone(); let to2 = to.clone();
        addb.connect_clicked(move |_| {
            let key = ke.text().to_string();
            let cmd = ce.text().to_string();
            let lua = format!("hl.bind(\"{}\", hl.dsp.exec_cmd(\"{}\"))", key, cmd.replace('"', "\\\""));
            apply_lua(&st, &to2, &lua, true);
            append_config(&st, &lua);
        });
    }
    root
}

fn page_rules(state: AppState, to: ToastOverlay) -> GtkBox {
    let root = page_scaffold("Rules editors");
    let info = Label::new(Some("Append window/layer rule snippets."));
    info.set_halign(gtk4::Align::Start);
    root.append(&info);

    let wr_class = Entry::new(); wr_class.set_placeholder_text(Some("window class regex"));
    let wr_action = Entry::new(); wr_action.set_placeholder_text(Some("action, e.g. float = true")); wr_action.set_hexpand(true);
    let wr_btn = Button::with_label("Add window rule");
    let r1 = GtkBox::new(Orientation::Horizontal, 6);
    r1.append(&wr_class); r1.append(&wr_action); r1.append(&wr_btn);
    root.append(&r1);
    {
        let c = wr_class.clone(); let a = wr_action.clone(); let st = state.clone(); let to2 = to.clone();
        wr_btn.connect_clicked(move |_| {
            let class = c.text().to_string();
            let action = a.text().to_string();
            let lua = format!("hl.window_rule({{ match = {{ class = \"^{}$\" }}, {} }})", class, action);
            apply_lua(&st, &to2, &lua, true);
            append_config(&st, &lua);
        });
    }

    let lr_ns = Entry::new(); lr_ns.set_placeholder_text(Some("layer namespace regex"));
    let lr_action = Entry::new(); lr_action.set_placeholder_text(Some("action")); lr_action.set_hexpand(true);
    let lr_btn = Button::with_label("Add layer rule");
    let r2 = GtkBox::new(Orientation::Horizontal, 6);
    r2.append(&lr_ns); r2.append(&lr_action); r2.append(&lr_btn);
    root.append(&r2);
    {
        let n = lr_ns.clone(); let a = lr_action.clone(); let st = state.clone(); let to2 = to.clone();
        lr_btn.connect_clicked(move |_| {
            let ns = n.text().to_string();
            let action = a.text().to_string();
            let lua = format!("hl.layer_rule({{ match = {{ namespace = \"^{}$\" }}, {} }})", ns, action);
            apply_lua(&st, &to2, &lua, true);
            append_config(&st, &lua);
        });
    }

    let view = TextView::new(); view.set_monospace(true); view.set_editable(false);
    let vbuf = view.buffer();
    let sw = ScrolledWindow::builder().child(&view).vexpand(true).hexpand(true).build();
    root.append(&sw);
    let b = Button::with_label("Show current rules (hyprctl)");
    root.append(&b);
    {
        let vbufc = vbuf.clone();
        b.connect_clicked(move |_| {
            let out = format!("{}\n--- layers ---\n{}", hyprctl(&["workspacerules"]), hyprctl(&["layers"]));
            vbufc.set_text(&out);
        });
    }
    root
}

fn page_autostart_env(state: AppState, to: ToastOverlay) -> GtkBox {
    let root = page_scaffold("Autostart & environment");
    let cmd_entry = Entry::new(); cmd_entry.set_placeholder_text(Some("exec command")); cmd_entry.set_hexpand(true);
    let cmd_btn = Button::with_label("Add exec");
    let r1 = GtkBox::new(Orientation::Horizontal, 6);
    r1.append(&cmd_entry); r1.append(&cmd_btn);
    root.append(&r1);
    {
        let e = cmd_entry.clone(); let st = state.clone(); let to2 = to.clone();
        cmd_btn.connect_clicked(move |_| {
            let cmd = e.text().to_string();
            let lua = format!("hl.exec_cmd(\"{}\")", cmd.replace('"', "\\\""));
            apply_lua(&st, &to2, &lua, true);
            append_config(&st, &lua);
        });
    }
    let k_entry = Entry::new(); k_entry.set_placeholder_text(Some("ENV key"));
    let v_entry = Entry::new(); v_entry.set_placeholder_text(Some("value")); v_entry.set_hexpand(true);
    let env_btn = Button::with_label("Add env");
    let r2 = GtkBox::new(Orientation::Horizontal, 6);
    r2.append(&k_entry); r2.append(&v_entry); r2.append(&env_btn);
    root.append(&r2);
    {
        let ke = k_entry.clone(); let ve = v_entry.clone(); let st = state.clone(); let to2 = to.clone();
        env_btn.connect_clicked(move |_| {
            let k = ke.text().to_string();
            let v = ve.text().to_string();
            let lua = format!("hl.env(\"{}\", \"{}\")", k, v.replace('"', "\\\""));
            apply_lua(&st, &to2, &lua, true);
            append_config(&st, &lua);
        });
    }
    root
}

fn page_cursor(state: AppState, to: ToastOverlay) -> GtkBox {
    let root = page_scaffold("Cursor theme");
    let combo = gtk4::ComboBoxText::new();
    for t in list_cursor_themes() { combo.append_text(&t); }
    if combo.active_text().is_none() { combo.set_active(Some(0)); }
    root.append(&combo);
    let size_entry = Entry::new(); size_entry.set_text("24"); size_entry.set_width_chars(6);
    root.append(&size_entry);
    let apply = Button::with_label("Apply cursor");
    root.append(&apply);
    {
        let combo = combo.clone(); let size = size_entry.clone(); let st = state.clone(); let to2 = to.clone();
        apply.connect_clicked(move |_| {
            let theme = combo.active_text().map(|s| s.to_string()).unwrap_or_default();
            let size: i32 = size.text().parse().unwrap_or(24);
            let lua = format!("hl.env(\"HYPRCURSOR_THEME\", \"{}\"); hl.env(\"XCURSOR_THEME\", \"{}\"); hl.env(\"HYPRCURSOR_SIZE\", \"{}\"); hl.env(\"XCURSOR_SIZE\", \"{}\")", theme, theme, size, size);
            apply_lua(&st, &to2, &lua, true);
            append_config(&st, &lua);
        });
    }
    root
}

fn list_cursor_themes() -> Vec<String> {
    let mut v = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/usr/share/icons") {
        for e in entries.flatten() {
            if e.path().join("cursors").is_dir() { v.push(e.file_name().to_string_lossy().to_string()); }
        }
    }
    v.sort();
    v
}

fn page_animations(state: AppState, to: ToastOverlay) -> GtkBox {
    let root = page_scaffold("Animations & beziers");
    let name_entry = Entry::new(); name_entry.set_placeholder_text(Some("bezier name")); name_entry.set_hexpand(true);
    let x1 = Entry::new(); x1.set_text("0.05"); x1.set_width_chars(6);
    let y1 = Entry::new(); y1.set_text("0.9"); y1.set_width_chars(6);
    let x2 = Entry::new(); x2.set_text("0.1"); x2.set_width_chars(6);
    let y2 = Entry::new(); y2.set_text("1.0"); y2.set_width_chars(6);
    let row = GtkBox::new(Orientation::Horizontal, 6);
    row.append(&name_entry); row.append(&x1); row.append(&y1); row.append(&x2); row.append(&y2);
    root.append(&row);

    let area = DrawingArea::new();
    area.set_content_width(320); area.set_content_height(180);
    area.set_draw_func(|_, cr, w, h| {
        cr.set_source_rgb(0.15, 0.15, 0.18);
        cr.rectangle(0.0, 0.0, w as f64, h as f64);
        cr.fill().ok();
        cr.set_source_rgb(0.8, 0.8, 1.0);
        cr.move_to(0.0, h as f64);
        cr.curve_to(w as f64 * 0.05, h as f64 * (1.0 - 0.9), w as f64 * 0.1, h as f64 * (1.0 - 1.0), w as f64, 0.0);
        cr.stroke().ok();
    });
    root.append(&area);

    let gen_btn = Button::with_label("Generate bezier config");
    root.append(&gen_btn);
    {
        let ne = name_entry.clone(); let a = x1.clone(); let b = y1.clone(); let c = x2.clone(); let d = y2.clone();
        let st = state.clone(); let to2 = to.clone();
        gen_btn.connect_clicked(move |_| {
            let name = ne.text().to_string();
            let lua = format!("hl.curve(\"{}\", {{ type = \"bezier\", points = {{ {{{}, {}}}, {{{}, {}}} }} }})", name, a.text(), b.text(), c.text(), d.text());
            apply_lua(&st, &to2, &lua, true);
            append_config(&st, &lua);
        });
    }
    let anim_leaf = Entry::new(); anim_leaf.set_placeholder_text(Some("leaf, e.g. windows")); anim_leaf.set_hexpand(true);
    let anim_enabled = Entry::new(); anim_enabled.set_text("true"); anim_enabled.set_width_chars(8);
    let anim_speed = Entry::new(); anim_speed.set_text("5"); anim_speed.set_width_chars(6);
    let anim_btn = Button::with_label("Set animation");
    let r = GtkBox::new(Orientation::Horizontal, 6);
    r.append(&anim_leaf); r.append(&anim_enabled); r.append(&anim_speed); r.append(&anim_btn);
    root.append(&r);
    {
        let l = anim_leaf.clone(); let e = anim_enabled.clone(); let s = anim_speed.clone();
        let st = state.clone(); let to2 = to.clone();
        anim_btn.connect_clicked(move |_| {
            let leaf = l.text().to_string();
            let enabled = e.text().to_string();
            let speed = s.text().to_string();
            let lua = format!("hl.animation({{ leaf = \"{}\", enabled = {}, speed = {} }})", leaf, enabled, speed);
            apply_lua(&st, &to2, &lua, true);
            append_config(&st, &lua);
        });
    }
    root
}

fn page_config(state: AppState, _to: ToastOverlay) -> GtkBox {
    let root = page_scaffold("Config file (hyprland.lua)");
    let path_row = GtkBox::new(Orientation::Horizontal, 6);
    let path_entry = Entry::new();
    path_entry.set_text(&state.config_path.borrow());
    path_entry.set_hexpand(true);
    let load_btn = Button::with_label("Load");
    let save_btn = Button::with_label("Save");
    let backup_btn = Button::with_label("Backup");
    path_row.append(&path_entry); path_row.append(&load_btn); path_row.append(&save_btn); path_row.append(&backup_btn);
    root.append(&path_row);
    let file_text = TextView::new(); file_text.set_monospace(true);
    let file_buf = file_text.buffer();
    let file_sw = ScrolledWindow::builder().child(&file_text).vexpand(true).hexpand(true).build();
    root.append(&file_sw);
    let append_label = Label::new(Some("Append Lua snippet:"));
    append_label.set_halign(gtk4::Align::Start);
    root.append(&append_label);
    let append_text = TextView::new(); append_text.set_monospace(true);
    append_text.buffer().set_text("hl.config({ general = { gaps_in = 5 } })");
    let append_sw = ScrolledWindow::builder().child(&append_text).height_request(70).hexpand(true).build();
    root.append(&append_sw);
    let append_btn = Button::with_label("Append to file");
    root.append(&append_btn);
    let del_row = GtkBox::new(Orientation::Horizontal, 6);
    let del_entry = Entry::new(); del_entry.set_placeholder_text(Some("substring to delete lines containing")); del_entry.set_hexpand(true);
    let del_btn = Button::with_label("Delete lines");
    del_row.append(&del_entry); del_row.append(&del_btn);
    root.append(&del_row);

    {
        let pe = path_entry.clone(); let fb = file_buf.clone();
        load_btn.connect_clicked(move |_| {
            let p = pe.text().to_string();
            match std::fs::read_to_string(&p) { Ok(s) => fb.set_text(&s), Err(e) => fb.set_text(&format!("error: {}", e)) }
        });
    }
    {
        let pe = path_entry.clone(); let fb = file_buf.clone();
        save_btn.connect_clicked(move |_| {
            let p = pe.text().to_string();
            let (s,e) = (fb.start_iter(), fb.end_iter());
            let text = fb.text(&s,&e,false).to_string();
            let _ = std::fs::write(&p, text);
        });
    }
    {
        let pe = path_entry.clone();
        backup_btn.connect_clicked(move |_| {
            let p = pe.text().to_string();
            let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs().to_string()).unwrap_or_default();
            let backup = format!("{}.bak.{}", p, ts);
            if let Ok(s) = std::fs::read_to_string(&p) { let _ = std::fs::write(&backup, s); }
        });
    }
    {
        let fb = file_buf.clone(); let ab = append_text.clone();
        append_btn.connect_clicked(move |_| {
            let abuf = ab.buffer();
            let (as_,ae) = (abuf.start_iter(), abuf.end_iter());
            let snippet = abuf.text(&as_,&ae,false).to_string();
            let (s,e) = (fb.start_iter(), fb.end_iter());
            let mut cur = fb.text(&s,&e,false).to_string();
            if !cur.ends_with('\n') { cur.push('\n'); }
            cur.push_str(&snippet); cur.push('\n');
            fb.set_text(&cur);
        });
    }
    {
        let fb = file_buf.clone(); let de = del_entry.clone();
        del_btn.connect_clicked(move |_| {
            let needle = de.text().to_string();
            let (s,e) = (fb.start_iter(), fb.end_iter());
            let text = fb.text(&s,&e,false).to_string();
            let filtered: String = text.lines().filter(|l| needle.is_empty() || !l.contains(&needle)).map(|l| format!("{}\n", l)).collect();
            fb.set_text(&filtered);
        });
    }
    root
}

fn page_profiles(state: AppState, to: ToastOverlay) -> GtkBox {
    let root = page_scaffold("Profiles");
    let _ = std::fs::create_dir_all(state.profile_dir.borrow().as_str());
    let name_entry = Entry::new(); name_entry.set_placeholder_text(Some("profile name")); name_entry.set_hexpand(true);
    let save_btn = Button::with_label("Save current as profile");
    let r1 = GtkBox::new(Orientation::Horizontal, 6);
    r1.append(&name_entry); r1.append(&save_btn);
    root.append(&r1);
    let list_box = ListBox::new();
    let sw = ScrolledWindow::builder().child(&list_box).vexpand(true).hexpand(true).build();
    root.append(&sw);
    let refresh_list = Rc::new(RefCell::new(None::<Box<dyn Fn()>>));
    let to_for_rl = to.clone();
    let rl_imp = {
        let list_box = list_box.clone();
        let state = state.clone();
        let to = to_for_rl.clone();
        move || {
            while let Some(child) = list_box.first_child() { list_box.remove(&child); }
            let dir = state.profile_dir.borrow().clone();
            if let Ok(entries) = std::fs::read_dir(&dir) {
                let mut names: Vec<String> = entries.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
                names.sort();
                for n in names {
                    let row_box = GtkBox::new(Orientation::Horizontal, 6);
                    row_box.append(&Label::new(Some(&n)));
                    let apply = Button::with_label("Apply");
                    let del = Button::with_label("Delete");
                    row_box.append(&apply); row_box.append(&del);
                    let n2 = n.clone();
                    let st = state.clone();
                    let to2 = to.clone();
                    apply.connect_clicked(move |_| {
                        let path = format!("{}/{}", st.profile_dir.borrow(), n2);
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            let cfg = st.config_path.borrow().clone();
                            let _ = std::fs::write(&cfg, content);
                            let _ = run_cmd("hyprctl", &["reload"]);
                            toast(&to2, "profile applied");
                        }
                    });
                    let n3 = n.clone();
                    let st2 = state.clone();
                    del.connect_clicked(move |_| {
                        let path = format!("{}/{}", st2.profile_dir.borrow(), n3);
                        let _ = std::fs::remove_file(&path);
                    });
                    list_box.append(&row_box);
                }
            }
        }
    };
    *refresh_list.borrow_mut() = Some(Box::new(rl_imp.clone()));
    rl_imp();
    {
        let rl = rl_imp.clone();
        let ne = name_entry.clone(); let st = state.clone(); let to2 = to.clone();
        save_btn.connect_clicked(move |_| {
            let name = ne.text().to_string();
            if name.is_empty() { return; }
            let src = st.config_path.borrow().clone();
            if let Ok(content) = std::fs::read_to_string(&src) {
                let dir = st.profile_dir.borrow().clone();
                let _ = std::fs::create_dir_all(&dir);
                let path = format!("{}/{}", dir, name);
                let _ = std::fs::write(&path, content);
                toast(&to2, "profile saved");
                rl();
            }
        });
    }
    let rb = Button::with_label("Refresh list");
    root.append(&rb);
    {
        let rl = rl_imp;
        rb.connect_clicked(move |_| rl());
    }
    root
}

fn page_pending(state: AppState, to: ToastOverlay) -> GtkBox {
    let root = page_scaffold("Pending changes & undo");
    let view = TextView::new(); view.set_monospace(true); view.set_editable(false);
    let vbuf = view.buffer();
    let sw = ScrolledWindow::builder().child(&view).vexpand(true).hexpand(true).build();
    root.append(&sw);
    let refresh = Button::with_label("Refresh pending");
    let apply = Button::with_label("Apply all pending");
    let undo = Button::with_label("Undo last snapshot");
    let row = GtkBox::new(Orientation::Horizontal, 6);
    row.append(&refresh); row.append(&apply); row.append(&undo);
    root.append(&row);
    let refresh_fn = {
        let vbuf = vbuf.clone(); let state = state.clone();
        move || {
            let pending = state.pending.borrow();
            let mut text = String::from("Pending (session) changes:\n");
            for p in pending.iter() { text.push_str(&format!("- {} = {}\n  {}\n", p.key, p.value, p.lua)); }
            text.push_str(&format!("\nUndo snapshots: {}\n", state.undo.borrow().len()));
            vbuf.set_text(&text);
        }
    };
    refresh_fn();
    {
        let rf = refresh_fn.clone();
        refresh.connect_clicked(move |_| rf());
    }
    {
        let st = state.clone(); let to2 = to.clone();
        apply.connect_clicked(move |_| {
            for pc in st.pending.borrow().iter() {
                let _ = hyprctl(&["eval", &pc.lua]);
            }
            toast(&to2, "pending applied");
        });
    }
    {
        let st = state.clone(); let to2 = to.clone(); let vbuf = vbuf.clone();
        undo.connect_clicked(move |_| {
            let mut undo_stack = st.undo.borrow_mut();
            if let Some(prev) = undo_stack.pop() {
                let cfg = st.config_path.borrow().clone();
                let _ = std::fs::write(&cfg, prev);
                drop(undo_stack);
                let _ = run_cmd("hyprctl", &["reload"]);
                toast(&to2, "undone");
            } else {
                toast(&to2, "nothing to undo");
            }
            let pending = st.pending.borrow();
            let mut text = String::from("Pending (session) changes:\n");
            for p in pending.iter() { text.push_str(&format!("- {} = {}\n", p.key, p.value)); }
            text.push_str(&format!("\nUndo snapshots: {}\n", st.undo.borrow().len()));
            vbuf.set_text(&text);
        });
    }
    root
}

fn page_search(state: AppState, _to: ToastOverlay) -> GtkBox {
    let root = page_scaffold("Global search");
    let entry = Entry::new(); entry.set_placeholder_text(Some("search options/text")); entry.set_hexpand(true);
    root.append(&entry);
    let view = TextView::new(); view.set_monospace(true); view.set_editable(false);
    let vbuf = view.buffer();
    let sw = ScrolledWindow::builder().child(&view).vexpand(true).hexpand(true).build();
    root.append(&sw);
    let search = {
        let vbuf = vbuf.clone(); let state = state.clone();
        move |q: &str| {
            let q = q.to_lowercase();
            let mut out = String::new();
            let common = ["general:gaps_in", "general:gaps_out", "general:border_size", "general:layout", "decoration:rounding", "decoration:active_opacity", "decoration:inactive_opacity", "input:kb_layout", "input:follow_mouse", "misc:disable_hyprland_logo", "animations:enabled", "cursor:no_hardware_cursors"];
            for c in common { if c.to_lowercase().contains(&q) { out.push_str(&format!("{} \n", c)); } }
            let path = state.config_path.borrow().clone();
            if let Ok(content) = std::fs::read_to_string(&path) {
                for line in content.lines() {
                    if line.to_lowercase().contains(&q) { out.push_str(&format!("file: {}\n", line.trim())); }
                }
            }
            vbuf.set_text(&out);
        }
    };
    {
        let search = search.clone();
        entry.connect_changed(move |entry| { search(&entry.text()); });
    }
    search("");
    root
}

fn page_console(state: AppState, to: ToastOverlay) -> GtkBox {
    let root = page_scaffold("Console / raw hyprctl");
    let cmd_entry = Entry::new(); cmd_entry.set_placeholder_text(Some("hyprctl args, e.g. monitors -j")); cmd_entry.set_hexpand(true);
    let run = Button::with_label("Run");
    let r = GtkBox::new(Orientation::Horizontal, 6);
    r.append(&cmd_entry); r.append(&run);
    root.append(&r);
    let view = TextView::new(); view.set_monospace(true);
    let vbuf = view.buffer();
    let sw = ScrolledWindow::builder().child(&view).vexpand(true).hexpand(true).build();
    root.append(&sw);
    {
        let ce = cmd_entry.clone(); let vbuf = vbuf.clone();
        run.connect_clicked(move |_| {
            let text = ce.text().to_string();
            let args: Vec<&str> = text.split_whitespace().collect();
            vbuf.set_text(&hyprctl(&args));
        });
    }
    let _ = (state, to);
    root
}


fn parse_wiki_options() -> Vec<WikiOption> {
    let mut out = Vec::new();
    collect_wiki_files(&WIKI, &mut |path, content| {
        let mut current_category = String::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i].trim();
            if line.starts_with("Category:") || line.starts_with("Category `") {
                if let Some(start) = line.find('`') {
                    if let Some(end) = line[start + 1..].find('`') {
                        current_category = line[start + 1..start + 1 + end].to_string();
                    }
                }
            }
            if line.contains("| Name | Description | Type | Default | Limits |") {
                i += 1;
                if i < lines.len() && lines[i].trim().starts_with("| ---") { i += 1; }
                while i < lines.len() && lines[i].trim().starts_with('|') && !lines[i].trim().starts_with("| Name") {
                    let row = lines[i].trim();
                    let cells: Vec<String> = row.split('|').map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect();
                    if cells.len() >= 5 {
                        let name = cells[0].clone();
                        if name != "Name" && !name.is_empty() && !name.starts_with("---") {
                            out.push(WikiOption {
                                category: if current_category.is_empty() { path.to_string() } else { current_category.clone() },
                                name,
                                description: cells[1].clone(),
                                ty: cells[2].clone(),
                                default: cells[3].clone(),
                                limits: cells[4].clone(),
                            });
                        }
                    }
                    i += 1;
                }
                continue;
            }
            i += 1;
        }
    });
    out
}

fn collect_wiki_files(dir: &Dir, cb: &mut dyn FnMut(&str, &str)) {
    for entry in dir.entries() {
        match entry {
            include_dir::DirEntry::Dir(d) => collect_wiki_files(d, cb),
            include_dir::DirEntry::File(f) => {
                let p = f.path().to_string_lossy().to_string();
                if p.ends_with(".md") {
                    if let Some(content) = f.contents_utf8() {
                        cb(&p, content);
                    }
                }
            }
        }
    }
}

fn get_option_value_simple(option: &str) -> String {
    let out = hyprctl(&["getoption", option, "-j"]);
    let json = extract_json(&out);
    if let Ok(v) = serde_json::from_str::<Value>(&json) {
        if let Some(b) = v.get("bool") { return b.to_string(); }
        if let Some(i) = v.get("int") { return i.to_string(); }
        if let Some(f) = v.get("float") { return f.to_string(); }
        if let Some(s) = v.get("str") { return s.as_str().unwrap_or("").to_string(); }
        if let Some(s) = v.get("css") { return s.as_str().unwrap_or("").to_string(); }
    }
    String::new()
}

fn recommendation_for(opt: &WikiOption) -> String {
    let ty = opt.ty.to_lowercase();
    let n = opt.name.to_lowercase();
    if ty.contains("bool") { return "Start with the documented default. Enable only when you want this visual or focus behavior.".to_string(); }
    if n.contains("gaps") { return "Beginner: try 8 / 12 / 16 for tiled gaps, and 10 / 20 for outer edges.".to_string(); }
    if n.contains("rounding") { return "Beginner: try 6, 8, or 12 for modern rounded corners.".to_string(); }
    if n.contains("border_size") { return "Beginner: 0 for a flat look, 1 or 2 for visible focus borders.".to_string(); }
    if n.contains("blur") && n.contains("enabled") { return "Disable on lower-end GPUs if animations stutter.".to_string(); }
    if n.contains("animation") && n.contains("enabled") { return "Disable if you prefer instant window changes.".to_string(); }
    if n.contains("vrr") { return "Use 1 (on) for supported monitors, 0 if flickering appears.".to_string(); }
    if n.contains("kb_layout") { return "Set your locale, for example us, tr, de, or multi layouts separated by commas.".to_string(); }
    format!("Safe beginner choice: keep {} ; adjust gradually and test with live apply.", opt.default)
}
fn page_wiki_options(state: AppState, to: ToastOverlay) -> GtkBox {
    let root = page_scaffold("Wiki options explorer");
    let opts = parse_wiki_options();
    let count = Label::new(Some(&format!("Parsed {} options from embedded wiki", opts.len())));
    count.set_halign(gtk4::Align::Start);
    root.append(&count);

    let search_entry = Entry::new();
    search_entry.set_placeholder_text(Some("search option or description"));
    search_entry.set_hexpand(true);
    root.append(&search_entry);

    let split = GtkBox::new(Orientation::Horizontal, 6);
    split.set_vexpand(true);
    let list_box = ListBox::new();
    let list_sw = ScrolledWindow::builder().child(&list_box).width_request(420).vexpand(true).hexpand(true).build();
    let detail = TextView::new();
    detail.set_monospace(true);
    detail.set_editable(false);
    let detail_buf = detail.buffer();
    let detail_sw = ScrolledWindow::builder().child(&detail).vexpand(true).hexpand(true).build();
    split.append(&list_sw);
    split.append(&detail_sw);
    root.append(&split);

    let opts_rc = std::rc::Rc::new(opts);
    let opts_for_build = opts_rc.clone();
    let list_box_for_build = list_box.clone();
    let search_for_build = search_entry.clone();

    let rebuild = {
        let list_box = list_box_for_build.clone();
        let opts = opts_for_build.clone();
        let query_entry = search_for_build.clone();
        move || {
            while let Some(child) = list_box.first_child() { list_box.remove(&child); }
            let q = query_entry.text().to_lowercase();
            for (idx, opt) in opts.iter().enumerate() {
                if !q.is_empty()
                    && !opt.name.to_lowercase().contains(&q)
                    && !opt.category.to_lowercase().contains(&q)
                    && !opt.description.to_lowercase().contains(&q)
                {
                    continue;
                }
                let row = GtkBox::new(Orientation::Horizontal, 6);
                let lbl = Label::new(Some(&format!("{} : {}", opt.category, opt.name)));
                lbl.set_halign(gtk4::Align::Start);
                lbl.set_hexpand(true);
                row.append(&lbl);
                row.set_widget_name(&idx.to_string());
                list_box.append(&row);
            }
        }
    };
    rebuild();
    {
        let rb = rebuild.clone();
        search_entry.connect_changed(move |_| rb());
    }

    let selected_idx: std::rc::Rc<std::cell::RefCell<Option<usize>>> = std::rc::Rc::new(std::cell::RefCell::new(None));
    let value_entries: std::rc::Rc<std::cell::RefCell<Vec<Entry>>> = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let value_switch: std::rc::Rc<std::cell::RefCell<Option<gtk4::Switch>>> = std::rc::Rc::new(std::cell::RefCell::new(None));

    let control_box = GtkBox::new(Orientation::Horizontal, 6);
    root.append(&control_box);

    let apply_row = GtkBox::new(Orientation::Horizontal, 6);
    let val_entry = Entry::new();
    val_entry.set_placeholder_text(Some("value"));
    val_entry.set_hexpand(true);
    let apply_btn = Button::with_label("Apply selected option");
    apply_row.append(&val_entry);
    apply_row.append(&apply_btn);
    root.append(&apply_row);

    {
        let list_box = list_box.clone();
        let opts = opts_rc.clone();
        let detail_buf = detail_buf.clone();
        let selected_idx = selected_idx.clone();
        let value_entries = value_entries.clone();
        let value_switch = value_switch.clone();
        let control_box = control_box.clone();
        let val_entry = val_entry.clone();
        list_box.connect_row_activated(move |_, row| {
            let idx_str = row.child().map(|c| c.widget_name()).unwrap_or_default();
            if let Ok(idx) = idx_str.parse::<usize>() {
                if let Some(opt) = opts.get(idx) {
                    let text = format!(
                        "{} : {}\nType: {}\nDefault: {}\nLimits: {}\n{}\nRecommendation: {}\n",
                        opt.category, opt.name, opt.ty, opt.default, opt.limits, opt.description, recommendation_for(opt)
                    );
                    detail_buf.set_text(&text);
                    *selected_idx.borrow_mut() = Some(idx);

                    while let Some(child) = control_box.first_child() { control_box.remove(&child); }
                    value_entries.borrow_mut().clear();
                    *value_switch.borrow_mut() = None;

                    let ty = opt.ty.to_lowercase();
                    if ty.contains("bool") {
                        let sw = gtk4::Switch::new();
                        let current = get_option_value_simple(&format!("{}.{}", opt.category, opt.name));
                        sw.set_active(current == "true" || current == "1");
                        control_box.append(&sw);
                        *value_switch.borrow_mut() = Some(sw);
                    } else if ty.contains("vec2") {
                        let e1 = Entry::new(); e1.set_width_chars(8); e1.set_placeholder_text(Some("x"));
                        let e2 = Entry::new(); e2.set_width_chars(8); e2.set_placeholder_text(Some("y"));
                        control_box.append(&e1); control_box.append(&e2);
                        value_entries.borrow_mut().push(e1.clone());
                        value_entries.borrow_mut().push(e2.clone());
                    } else {
                        let e = Entry::new();
                        e.set_hexpand(true);
                        e.set_placeholder_text(Some(&ty));
                        let current = get_option_value_simple(&format!("{}.{}", opt.category, opt.name));
                        if !current.is_empty() { e.set_text(&current); } else { e.set_text(&opt.default.replace('`', "")); }
                        control_box.append(&e);
                        value_entries.borrow_mut().push(e.clone());
                    }
                    let complex = format!("{}:{}", opt.category, opt.name);
                    val_entry.set_text(&complex);
                }
            }
        });
    }
    {
        let list_box = list_box.clone();
        let opts = opts_rc.clone();
        let val_entry = val_entry.clone();
        let state = state.clone();
        let to2 = to.clone();
        let value_entries = value_entries.clone();
        let value_switch = value_switch.clone();
        apply_btn.connect_clicked(move |_| {
            let Some(row) = list_box.selected_row() else { return; };
            let idx_str = row.child().map(|c| c.widget_name()).unwrap_or_default();
            let Ok(idx) = idx_str.parse::<usize>() else { return; };
            let Some(opt) = opts.get(idx) else { return; };
            let value = if let Some(sw) = value_switch.borrow().as_ref() {
                if sw.is_active() { "true".to_string() } else { "false".to_string() }
            } else {
                let entries = value_entries.borrow();
                if entries.len() >= 2 {
                    format!("{{{},{}}}", entries[0].text(), entries[1].text())
                } else if !entries.is_empty() {
                    entries[0].text().to_string()
                } else {
                    val_entry.text().to_string()
                }
            };
            let name = format!("{}:{}", opt.category, opt.name);
            let lua = option_to_lua_config(&name, &value);
            apply_lua(&state, &to2, &lua, true);
            append_config(&state, &lua);
        });
    }

    root
}

fn page_wiki_browser() -> GtkBox {
    let root = page_scaffold("Wiki browser");
    let search_entry = Entry::new();
    search_entry.set_placeholder_text(Some("search wiki text"));
    search_entry.set_hexpand(true);
    root.append(&search_entry);

    let split = GtkBox::new(Orientation::Horizontal, 6);
    split.set_vexpand(true);
    let file_list = ListBox::new();
    let fl_sw = ScrolledWindow::builder().child(&file_list).width_request(320).vexpand(true).hexpand(true).build();
    let content = TextView::new();
    content.set_monospace(false);
    content.set_editable(false);
    content.set_wrap_mode(WrapMode::WordChar);
    let cb = content.buffer();
    let c_sw = ScrolledWindow::builder().child(&content).vexpand(true).hexpand(true).build();
    split.append(&fl_sw);
    split.append(&c_sw);
    root.append(&split);

    let mut files: Vec<(String, String)> = Vec::new();
    collect_wiki_files(&WIKI, &mut |path, content| {
        files.push((path.to_string(), content.to_string()));
    });

    let files_rc = std::rc::Rc::new(files);
    let fl_clone = file_list.clone();
    let search_clone = search_entry.clone();
    let cb_clone = cb.clone();
    let files_for_rows = files_rc.clone();

    let rebuild = {
        let file_list = fl_clone.clone();
        let files = files_for_rows.clone();
        let query_entry = search_clone.clone();
        move || {
            while let Some(child) = file_list.first_child() { file_list.remove(&child); }
            let q = query_entry.text().to_lowercase();
            for (i, (path, content)) in files.iter().enumerate() {
                if !q.is_empty() && !path.to_lowercase().contains(&q) && !content.to_lowercase().contains(&q) {
                    continue;
                }
                let row = GtkBox::new(Orientation::Horizontal, 6);
                let lbl = Label::new(Some(path));
                lbl.set_halign(gtk4::Align::Start);
                lbl.set_hexpand(true);
                row.append(&lbl);
                row.set_widget_name(&i.to_string());
                file_list.append(&row);
            }
        }
    };
    rebuild();
    {
        let rb = rebuild.clone();
        search_entry.connect_changed(move |_| rb());
    }

    {
        let files = files_rc.clone();
        let cb = cb_clone.clone();
        file_list.connect_row_activated(move |_, row| {
            let idx_str = row.child().map(|c| c.widget_name()).unwrap_or_default();
            if let Ok(idx) = idx_str.parse::<usize>() {
                if let Some((_, content)) = files.get(idx) {
                    cb.set_text(content);
                }
            }
        });
    }

    root
}


fn page_start(stack: Stack, _state: AppState, _to: ToastOverlay) -> GtkBox {
    let root = page_scaffold("Start here for beginners");
    let intro = Label::new(Some("Choose an area. Buttons jump directly to the right page. Use Wiki options to search and apply any documented option."));
    intro.set_halign(gtk4::Align::Start);
    intro.set_wrap(true);
    root.append(&intro);

    let buttons = GtkBox::new(Orientation::Vertical, 6);
    let areas = [
        ("Live option editor", "live"),
        ("Monitors", "monitors"),
        ("Keybinds", "keybinds"),
        ("Rules", "rules"),
        ("Autostart and environment", "autostart"),
        ("Cursor", "cursor"),
        ("Animations", "animations"),
        ("Config editor", "config"),
        ("Profiles", "profiles"),
        ("Pending and undo", "pending"),
        ("Search", "search"),
        ("Console", "console"),
        ("Wiki options (all documented options)", "wiki_options"),
        ("Wiki browser (full help)", "wikibrowser"),
    ];
    for (name, key) in areas {
        let btn = Button::with_label(name);
        btn.set_halign(gtk4::Align::Start);
        let stack = stack.clone();
        let key = key.to_string();
        btn.connect_clicked(move |_| { stack.set_visible_child_name(&key); });
        buttons.append(&btn);
    }
    root.append(&buttons);
    root
}

fn page_live_wiki() -> GtkBox {
    let root = page_scaffold("Live Hyprland wiki");
    let web_view = WebView::new();
    web_view.set_vexpand(true);
    web_view.set_hexpand(true);
    web_view.load_uri("https://wiki.hypr.land/");
    let sw = ScrolledWindow::builder().child(&web_view).vexpand(true).hexpand(true).build();
    root.append(&sw);
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn option_to_lua_basic() {
        assert_eq!(option_to_lua_config("general:gaps_in", "5"), "hl.config({ general = { gaps_in = 5 } })");
    }
    #[test]
    fn option_to_lua_nested_colon() {
        assert_eq!(option_to_lua_config("decoration:shadow:enabled", "false"), "hl.config({ decoration = { shadow = { enabled = false } } })");
    }
    #[test]
    fn lua_to_options_nested() {
        let lua = "hl.config({\n  general = {\n    gaps_in = 0,\n    gaps_out = 0,\n  },\n  decoration = {\n    rounding = 12,\n  },\n})";
        let out = lua_to_options(lua);
        assert!(out.contains("general.gaps_in = 0"), "{}", out);
        assert!(out.contains("general.gaps_out = 0"), "{}", out);
        assert!(out.contains("decoration.rounding = 12"), "{}", out);
    }
}
