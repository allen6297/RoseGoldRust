use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use regex::Regex;
use rosegold_check::{Checker, FnRc};
use rosegold_syntax::{Expr, ExprKind, FnDecl, Pat, Stmt, StmtKind};

use crate::bytecode::Fn as BcFn;
use crate::json;
use crate::value::*;

#[path = "compile.rs"]
mod compile;
#[path = "llvm.rs"]
mod llvm;
#[path = "ui.rs"]
mod ui;
#[path = "vm.rs"]
mod vm;

pub struct Outcome {
    pub ok: bool,
    pub out: String,
    pub message: String,
    pub exit_code: i32,
}

/// One row a script added with `gui.element`.
#[derive(Clone, Debug, PartialEq)]
pub struct GuiElement {
    pub name: String,
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub color: i32,
    pub font: String,
    pub font_size: i32,
    pub font_color: i32,
}

/// One screen a script built with `gui.screen` during `main`.
#[derive(Clone, Debug, PartialEq)]
pub struct GuiScreen {
    pub name: String,
    pub w: f32,
    pub h: f32,
    pub x: f32,
    pub y: f32,
    /// One lowercase letter. Empty until `gui.press`.
    pub key: String,
    pub elements: Vec<GuiElement>,
}

impl Eq for GuiElement {}
impl Eq for GuiScreen {}

/// One settings row a script built with `settings.option` during `main`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingOption {
    pub category: String,
    /// The category description shown on the settings page.
    pub category_text: String,
    pub name: String,
    pub description: String,
    pub value: String,
    /// Empty means Enter flips `on` and `off`. Otherwise Enter steps through these words.
    pub choices: Vec<String>,
}

#[derive(Clone)]
struct GameHook {
    name: String,
    id: String,
    func: Value,
}

/// A `world.*` call from a hook. The pointer is the callback passed to
/// [`Session::call_hook_hosted`], and it does not borrow this interpreter.
type WorldHost = dyn FnMut(&str, &[Value]) -> Result<Value, String>;

fn run_entry(interp: &mut Interp, path: &str, name: &str) -> Result<(), String> {
    match interp.call_named(name) {
        Ok(Value::Int(code)) if code != 0 => Err(format!("{path}: {name} returned {code}")),
        Ok(_) => Ok(()),
        Err(Halt::Runtime(message)) => Err(message),
        Err(Halt::Throw { value, line, col }) => Err(located(
            "runtime error",
            path,
            line,
            col,
            &format!("uncaught throw: {}", value.to_print()),
        )),
    }
}

/// One loaded mod. It stays alive so the game can call the functions `main` registered.
pub struct Session {
    interp: Interp,
}

impl Session {
    /// Check and run `source`. `sandbox` is the root for `std.io` paths.
    pub fn load(source: &str, path: &str, sandbox: Option<&Path>) -> Result<Self, String> {
        let (program, parse_diags) = rosegold_syntax::parse_source(source, path);
        if let Some(d) = first_error(&parse_diags) {
            let kind = if d.kind.is_empty() {
                "parse error"
            } else {
                d.kind.as_str()
            };
            let file = if d.file.is_empty() {
                path
            } else {
                d.file.as_str()
            };
            return Err(located(kind, file, d.line, d.col, &d.message));
        }
        let has_main = program.fns.iter().any(|f| f.name == "main");
        let has_init = program.fns.iter().any(|f| f.name == "on_init");
        let (checker, diags) = rosegold_check::check_interpreted(program, path);
        if let Some(d) = first_error(&diags) {
            let kind = if d.kind.is_empty() {
                "error"
            } else {
                d.kind.as_str()
            };
            let file = if d.file.is_empty() {
                path
            } else {
                d.file.as_str()
            };
            return Err(located(kind, file, d.line, d.col, &d.message));
        }
        let mut interp = Interp::new(checker, &[path.to_string()]);
        if let Some(root) = sandbox {
            interp.sandbox_root = root.to_path_buf();
        }
        if has_main {
            run_entry(&mut interp, path, "main")?;
        }
        if has_init {
            run_entry(&mut interp, path, "on_init")?;
        }
        Ok(Self { interp })
    }

    /// `(name, id)` for each hook `main` registered, in registration order.
    pub fn hooks(&self) -> Vec<(String, String)> {
        self.interp
            .game_hooks
            .iter()
            .map(|hook| (hook.name.clone(), hook.id.clone()))
            .collect()
    }

    /// `(kind, namespace, path)` for each `block.register` and `item.register` in `main`.
    pub fn registrations(&self) -> Vec<(String, String, String)> {
        self.interp.registrations.clone()
    }

    /// `(kind, id, value)` for `block.texture` / `model` / `hardness` / `light`,
    /// `item.places` / `stack`, and recipe lines.
    pub fn attachments(&self) -> Vec<(String, String, String)> {
        self.interp.attachments.clone()
    }

    /// `(id, x, y, z)` still standing after `entity.spawn` and `entity.despawn`.
    pub fn spawns(&self) -> Vec<(String, i32, i32, i32)> {
        self.interp.spawns.clone()
    }

    /// Screens `GUIAPI.createScreen` recorded during `main`.
    pub fn screens(&self) -> Vec<GuiScreen> {
        self.interp.gui_screens.clone()
    }

    /// Write the typed line onto a field before its press runs.
    pub fn set_row_text(&mut self, hook_index: usize, row: &str, text: &str) {
        let Some(screen_name) = self
            .interp
            .game_hooks
            .get(hook_index)
            .map(|hook| hook.id.clone())
        else {
            return;
        };
        let text: String = text.chars().take(32).collect();
        let Some(screen) = self
            .interp
            .gui_screens
            .iter_mut()
            .find(|screen| screen.name == screen_name)
        else {
            return;
        };
        if let Some(element) = screen
            .elements
            .iter_mut()
            .find(|element| element.name == row && element.font == "field")
        {
            element.text = text;
        }
    }

    /// Settings rows `SettingsAPI.createOption` recorded during `main`.
    pub fn options(&self) -> Vec<SettingOption> {
        self.interp.settings.clone()
    }

    /// Call a hook registered by `main`. The string is whatever the function printed.
    pub fn call_hook(&mut self, index: usize, args: Vec<Value>) -> Result<(Value, String), String> {
        let func = self
            .interp
            .game_hooks
            .get(index)
            .cloned()
            .ok_or_else(|| format!("hook {index} is missing"))?
            .func;
        let start = self.interp.out.len();
        let value = self
            .interp
            .call_value(func, args)
            .map_err(|err| match err {
                Halt::Runtime(message) => message,
                Halt::Throw { value, line, col } => located(
                    "runtime error",
                    &self.interp.file,
                    line,
                    col,
                    &format!("uncaught throw: {}", value.to_print()),
                ),
            })?;
        let printed = self.interp.out[start..].to_string();
        Ok((value, printed))
    }

    /// Like [`Session::call_hook`], and `world.*` calls `host` while the function runs.
    pub fn call_hook_hosted(
        &mut self,
        index: usize,
        args: Vec<Value>,
        host: &mut WorldHost,
    ) -> Result<(Value, String), String> {
        self.interp.host = Some(host);
        let result = self.call_hook(index, args);
        self.interp.host = None;
        result
    }
}

pub type Task = Box<dyn FnOnce(&mut Interp) -> Result<(), Halt>>;

pub enum Halt {
    Runtime(String),
    Throw { value: Value, line: u32, col: u32 },
}

enum Flow {
    Next,
    Return(Value),
    Break,
    Continue,
    Throw(Value),
}

struct DeferredEmit {
    signal: String,
    rec: Option<Rc<RefCell<StructData>>>,
    args: Vec<Value>,
    line: u32,
    col: u32,
}

struct TimerJob {
    deadline_ms: i64,
    fut: Rc<RefCell<FutureData>>,
}

struct FrameJob {
    win_id: i64,
    fut: Rc<RefCell<FutureData>>,
}

struct Rng(u64);

impl Rng {
    fn new() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x1234_5678_9abc_def0);
        Self(seed | 1)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.0
    }

    fn next_int(&mut self, n: i32) -> i32 {
        if n <= 0 {
            return 0;
        }
        // Low bits of an LCG cycle every 2^k steps. Java's Random.nextInt uses the high bits.
        let bits = (self.next_u64() >> 33) as u32;
        (bits % n as u32) as i32
    }

    fn next_f64(&mut self) -> f64 {
        let bits = self.next_u64() >> 11;
        (bits as f64) / ((1u64 << 53) as f64)
    }
}

pub struct Interp {
    file: String,
    checker: Checker,
    argv: Vec<String>,
    fns: HashMap<String, FnRc>,
    listeners: HashMap<String, Vec<Value>>,
    deferred: Vec<DeferredEmit>,
    env: Vec<HashMap<String, Binding>>,
    loop_depth: i32,
    call_depth: i32,
    sync_emit_depth: i32,
    super_type: String,
    rng: Rng,
    microtasks: VecDeque<Task>,
    timers: Vec<TimerJob>,
    frame_jobs: Vec<FrameJob>,
    future_waiters: HashMap<usize, Vec<Task>>,
    ui: ui::HostUi,
    out: String,
    sandbox_root: PathBuf,
    start: Instant,
    game_hooks: Vec<GameHook>,
    /// `block.register` and `item.register` calls from `main`, as `(kind, namespace, path)`.
    registrations: Vec<(String, String, String)>,
    /// `block.texture` / `model` / `hardness` / `light`, `item.places` / `stack`, recipes.
    attachments: Vec<(String, String, String)>,
    /// Bodies placed by `entity.spawn` and not yet removed by `entity.despawn`.
    spawns: Vec<(String, i32, i32, i32)>,
    /// Screens built by `gui.screen` and `gui.element` during `main`.
    gui_screens: Vec<GuiScreen>,
    /// Rows built by `settings.option` during `main`.
    settings: Vec<SettingOption>,
    host: Option<*mut WorldHost>,
    compiled: HashMap<String, Option<Rc<BcFn>>>,
}

pub fn run_source(source: &str, path: &str, argv: &[String]) -> Outcome {
    let (program, parse_diags) = rosegold_syntax::parse_source(source, path);
    if let Some(d) = first_error(&parse_diags) {
        let kind = if d.kind.is_empty() {
            "parse error"
        } else {
            d.kind.as_str()
        };
        let file = if d.file.is_empty() {
            path
        } else {
            d.file.as_str()
        };
        return fail(&located(kind, file, d.line, d.col, &d.message));
    }
    let has_main = program.fns.iter().any(|f| f.name == "main");
    let (checker, diags) = rosegold_check::check_interpreted(program, path);
    if let Some(d) = first_error(&diags) {
        let kind = if d.kind.is_empty() {
            "error"
        } else {
            d.kind.as_str()
        };
        let file = if d.file.is_empty() {
            path
        } else {
            d.file.as_str()
        };
        return fail(&located(kind, file, d.line, d.col, &d.message));
    }
    let mut interp = Interp::new(checker, argv);
    let called = if has_main {
        interp.call_named("main")
    } else {
        Ok(Value::Void)
    };
    match called {
        Ok(v) => {
            let exit_code = match v {
                Value::Int(n) => n as i32,
                _ => 0,
            };
            Outcome {
                ok: true,
                out: std::mem::take(&mut interp.out),
                message: String::new(),
                exit_code,
            }
        }
        Err(Halt::Runtime(message)) => fail(&message),
        Err(Halt::Throw { value, line, col }) => fail(&located(
            "runtime error",
            &interp.file,
            line,
            col,
            &format!("uncaught throw: {}", value.to_print()),
        )),
    }
}

pub fn test_source(source: &str, path: &str) -> Outcome {
    let (program, parse_diags) = rosegold_syntax::parse_source(source, path);
    if let Some(d) = parse_diags.first() {
        let kind = if d.kind.is_empty() {
            "parse error"
        } else {
            d.kind.as_str()
        };
        let file = if d.file.is_empty() {
            path
        } else {
            d.file.as_str()
        };
        return fail_code(&located(kind, file, d.line, d.col, &d.message));
    }
    let tests: Vec<String> = program
        .fns
        .iter()
        .filter(|f| f.is_test)
        .map(|f| f.name.clone())
        .collect();
    let (checker, diags) = rosegold_check::check_interpreted(program, path);
    if let Some(d) = diags.first() {
        let kind = if d.kind.is_empty() {
            "error"
        } else {
            d.kind.as_str()
        };
        let file = if d.file.is_empty() {
            path
        } else {
            d.file.as_str()
        };
        return fail_code(&located(kind, file, d.line, d.col, &d.message));
    }
    if tests.is_empty() {
        return Outcome {
            ok: true,
            out: String::new(),
            message: "no @test functions".to_string(),
            exit_code: 0,
        };
    }
    let mut interp = Interp::new(checker, &[path.to_string()]);
    let mut failed = 0i32;
    for name in &tests {
        match interp.call_named(name) {
            Ok(_) => interp.emit(&format!("ok {name}\n")),
            Err(err) => {
                failed += 1;
                let msg = match err {
                    Halt::Runtime(msg) => msg,
                    Halt::Throw { value, line, col } => located(
                        "runtime error",
                        &interp.file,
                        line,
                        col,
                        &format!("uncaught throw: {}", value.to_print()),
                    ),
                };
                interp.emit(&format!("FAIL {name}: {msg}\n"));
            }
        }
    }
    let total = tests.len() as i32;
    let passed = total - failed;
    let summary = format!("{passed}/{total} tests passed");
    interp.emit(&format!("{summary}\n"));
    Outcome {
        ok: failed == 0,
        out: std::mem::take(&mut interp.out),
        message: summary,
        exit_code: if failed == 0 { 0 } else { 1 },
    }
}

pub fn test_file(path: &Path) -> Outcome {
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(err) => return fail_code(&err.to_string()),
    };
    test_source(&source, &path.display().to_string())
}

pub fn run_file(path: &Path, argv: &[String]) -> Outcome {
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(err) => return fail(&err.to_string()),
    };
    let display = path.display().to_string();
    let owned;
    let args: &[String] = if argv.is_empty() {
        owned = vec![display.clone()];
        &owned
    } else {
        argv
    };
    run_source(&source, &display, args)
}

fn fail(message: &str) -> Outcome {
    Outcome {
        ok: false,
        out: String::new(),
        message: message.to_string(),
        exit_code: 0,
    }
}

fn fail_code(message: &str) -> Outcome {
    Outcome {
        ok: false,
        out: String::new(),
        message: message.to_string(),
        exit_code: 1,
    }
}

fn first_error(diags: &[rosegold_syntax::Diagnostic]) -> Option<&rosegold_syntax::Diagnostic> {
    diags.iter().find(|d| d.severity == "error")
}

fn located(kind: &str, file: &str, line: u32, col: u32, msg: &str) -> String {
    let mut s = kind.to_string();
    if !file.is_empty() {
        s.push_str(" in ");
        s.push_str(file);
    }
    if line > 0 {
        s.push_str(" at ");
        s.push_str(&format!("{line}:{col}"));
    }
    s.push_str(": ");
    s.push_str(msg);
    s
}

fn type_head(ty: &str) -> String {
    match ty.find('[') {
        Some(p) => ty[..p].to_string(),
        None => ty.to_string(),
    }
}

fn int_cell(value: &Value) -> Option<i32> {
    match value {
        Value::Int(n) => i32::try_from(*n).ok(),
        _ => None,
    }
}

/// `hooks.*` names and the Fabric-style `events.*` names share one stored key.
fn canonical_hook(module: &str, name: &str) -> Option<&'static str> {
    match (module, name) {
        ("hooks", "before_break") => Some("before_break"),
        ("events", "break_block") | ("events", "break") => Some("break_block"),
        ("hooks", "before_place") | ("events", "place_block") | ("events", "place") => {
            Some("before_place")
        }
        ("hooks", "after_break") | ("events", "broken") => Some("after_break"),
        ("hooks", "after_place") | ("events", "placed") => Some("after_place"),
        ("hooks", "fill_place") | ("events", "fill_place") => Some("fill_place"),
        ("hooks", "fill_remove") | ("events", "fill_remove") => Some("fill_remove"),
        ("hooks", "on_select") | ("events", "select_slot") => Some("on_select"),
        ("hooks", "on_screen") | ("events", "open_screen") => Some("on_screen"),
        ("hooks", "break_speed") | ("events", "break_speed") => Some("break_speed"),
        ("events", "pickup_item") => Some("pickup_item"),
        ("events", "break_drop") => Some("break_drop"),
        ("events", "loot") => Some("loot"),
        ("events", "entity_spawn") => Some("entity_spawn"),
        ("events", "entity_hurt") => Some("entity_hurt"),
        ("events", "entity_death") => Some("entity_death"),
        ("events", "entity_tick") => Some("entity_tick"),
        ("events", "use_item") => Some("use_item"),
        ("events", "use_block") => Some("use_block"),
        ("events", "use_entity") => Some("use_entity"),
        ("events", "attack") => Some("attack"),
        ("events", "chat") => Some("chat"),
        ("events", "respawn") => Some("respawn"),
        ("events", "craft") => Some("craft"),
        ("events", "smelt") => Some("smelt"),
        ("events", "repair") => Some("repair"),
        ("events", "enchant") => Some("enchant"),
        ("events", "brew") => Some("brew"),
        ("events", "smith") => Some("smith"),
        ("events", "weave") => Some("weave"),
        ("events", "tick") => Some("tick"),
        ("events", "dawn") => Some("dawn"),
        ("events", "dusk") => Some("dusk"),
        ("events", "jump") => Some("jump"),
        ("events", "sprint") => Some("sprint"),
        ("events", "land") => Some("land"),
        ("events", "command") => Some("command"),
        ("events", "structure_discovered") => Some("structure_discovered"),
        _ => None,
    }
}

fn host_arity(module: &str, name: &str) -> Option<usize> {
    match (module, name) {
        ("world", "block" | "get_block") => Some(3),
        ("world", "write" | "set_block") => Some(4),
        ("world", "set_meta" | "set_fill") => Some(4),
        ("world", "get_meta" | "get_fill" | "fill_level" | "is_air" | "is_compound") => Some(3),
        (
            "world",
            "hardness" | "light" | "replaces" | "slot" | "count" | "time_of_day" | "is_day"
            | "season" | "has_time",
        ) => Some(
            if matches!(name, "time_of_day" | "is_day" | "season" | "has_time") {
                0
            } else {
                1
            },
        ),
        ("player", "stack" | "count") => Some(1),
        ("player", "held" | "selected" | "present" | "x" | "y" | "z") => Some(0),
        ("item", "exists" | "hardness" | "light" | "replaces" | "max_count") => Some(1),
        ("modapi", "note" | "registry") => Some(1),
        ("chat", "say") => Some(1),
        ("settings", "value") => Some(1),
        ("entity", "spawn" | "despawn") => Some(4),
        ("block_entity", "has" | "clear") => Some(3),
        ("block_entity", "get" | "get_int") => Some(4),
        ("block_entity", "set" | "set_int") => Some(5),
        ("gui", "open") => Some(1),
        _ => None,
    }
}

fn registry_kind(kind: &str) -> bool {
    let mut chars = kind.chars();
    match chars.next() {
        Some(first) if first.is_ascii_uppercase() => {}
        _ => return false,
    }
    chars.all(|ch| ch.is_ascii_uppercase() || ch == '_')
}

fn placed(n: f64) -> bool {
    n.is_finite() && (-4096.0..=4096.0).contains(&n)
}

fn whole_at(args: &[Value], index: usize) -> Option<i32> {
    let value = args.get(index)?;
    if !value.is_numeric() {
        return None;
    }
    let number = value.as_f64();
    if number.is_finite() && number.fract() == 0.0 && (i32::MIN as f64..=i32::MAX as f64).contains(&number)
    {
        Some(number as i32)
    } else {
        None
    }
}

/// `""` and `bitmap` are the mixed face. `capital` draws the tall capitals.
/// `button` is a raised plate. `heading` is a label with a rule. `icon` is a block face.
/// `field` is a line you can type.
fn known_font(font: &str) -> bool {
    matches!(
        font,
        "" | "bitmap" | "capital" | "button" | "heading" | "icon" | "field"
    )
}

fn choice_word(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 16
        && text.chars().all(|ch| ch.is_ascii_lowercase())
}

fn setting_words(text: &str, max: usize) -> bool {
    if text.is_empty() || text.len() > max || text.starts_with(' ') || text.ends_with(' ') {
        return false;
    }
    let mut spaced = false;
    for ch in text.chars() {
        if ch.is_ascii_lowercase() {
            spaced = false;
        } else if ch == ' ' && !spaced {
            spaced = true;
        } else {
            return false;
        }
    }
    true
}

fn reserved_setting(name: &str) -> bool {
    matches!(
        name,
        "world"
            | "hemisphere"
            | "volume"
            | "view_bob"
            | "trace"
            | "back"
            | "play"
            | "join"
            | "settings"
            | "worlds"
            | "quit"
            | "resume"
            | "title"
            | "new"
    )
}

fn mod_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
}

fn is_host(name: &str) -> bool {
    matches!(
        name,
        "checks"
            | "process"
            | "__math"
            | "__str"
            | "__io"
            | "__uuid"
            | "__time"
            | "__path"
            | "__json"
            | "__regex"
            | "__ui"
            | "hooks"
            | "events"
            | "world"
            | "player"
            | "item"
            | "block"
            | "recipe"
            | "loot"
            | "entity"
            | "block_entity"
            | "modapi"
            | "gui"
            | "chat"
            | "settings"
    )
}

fn skip_capture(name: &str) -> bool {
    matches!(
        name,
        "print" | "len" | "assert" | "argv" | "argv_len" | "super"
    )
}

fn min_arity(func: &FnDecl, method: bool) -> usize {
    let off = if method { 1 } else { 0 };
    for i in off..func.params.len() {
        if func
            .param_defaults
            .get(i)
            .and_then(|d| d.as_ref())
            .is_some()
        {
            return i - off;
        }
    }
    func.params.len().saturating_sub(off)
}

fn max_arity(func: &FnDecl, method: bool) -> usize {
    let off = if method { 1 } else { 0 };
    func.params.len().saturating_sub(off)
}

fn fut_key(fut: &Rc<RefCell<FutureData>>) -> usize {
    Rc::as_ptr(fut) as usize
}

impl Interp {
    fn new(checker: Checker, argv: &[String]) -> Self {
        let file = checker.file.clone();
        let mut argv = argv.to_vec();
        if argv.is_empty() && !file.is_empty() {
            argv.push(file.clone());
        }
        let fns = checker.fns.borrow().clone();
        let compiled = compile::attach(&checker);
        let mut listeners = HashMap::new();
        for sig in checker.signal_arity.borrow().keys() {
            listeners.insert(sig.clone(), Vec::new());
        }
        Self {
            file,
            checker,
            argv,
            fns,
            listeners,
            deferred: Vec::new(),
            env: vec![HashMap::new()],
            loop_depth: 0,
            call_depth: 0,
            sync_emit_depth: 0,
            super_type: String::new(),
            rng: Rng::new(),
            microtasks: VecDeque::new(),
            timers: Vec::new(),
            frame_jobs: Vec::new(),
            future_waiters: HashMap::new(),
            ui: ui::HostUi::new(),
            out: String::new(),
            sandbox_root: sandbox_root(),
            start: Instant::now(),
            game_hooks: Vec::new(),
            registrations: Vec::new(),
            attachments: Vec::new(),
            spawns: Vec::new(),
            gui_screens: Vec::new(),
            settings: Vec::new(),
            host: None,
            compiled,
        }
    }

    fn call_value(&mut self, func: Value, args: Vec<Value>) -> Result<Value, Halt> {
        let ret = match self.call_fn_value(func, args, 1, 1) {
            Ok(v) => v,
            Err(Halt::Throw { value, line, col }) => {
                return Err(self.runtime(
                    &format!("uncaught throw: {}", value.to_print()),
                    line,
                    col,
                ));
            }
            Err(err) => return Err(err),
        };
        let ret = if let Value::Future(fut) = &ret {
            let fut = fut.clone();
            match self.await_future(&fut, 1, 1) {
                Ok(v) => v,
                Err(Halt::Throw { value, line, col }) => {
                    return Err(self.runtime(
                        &format!("uncaught throw: {}", value.to_print()),
                        line,
                        col,
                    ));
                }
                Err(err) => return Err(err),
            }
        } else {
            ret
        };
        self.flush_deferred()?;
        self.drain_event_loop()?;
        Ok(ret)
    }

    fn runtime(&self, msg: &str, line: u32, col: u32) -> Halt {
        Halt::Runtime(located("runtime error", &self.file, line, col, msg))
    }

    fn emit(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.out.push_str(text);
    }

    fn now_ms(&self) -> i64 {
        self.start.elapsed().as_millis() as i64
    }

    fn call_named(&mut self, name: &str) -> Result<Value, Halt> {
        let Some(func) = self.fns.get(name).cloned() else {
            return Err(self.runtime(&format!("unknown function '{name}'"), 0, 0));
        };
        let line = func.span.line;
        let ret = match self.call_user(&func, Vec::new(), line, 1, None, None) {
            Ok(v) => v,
            Err(Halt::Throw { value, line, col }) => {
                return Err(self.runtime(
                    &format!("uncaught throw: {}", value.to_print()),
                    line,
                    col,
                ));
            }
            Err(err) => return Err(err),
        };
        let ret = if let Value::Future(fut) = &ret {
            let fut = fut.clone();
            match self.await_future(&fut, line, 1) {
                Ok(v) => v,
                Err(Halt::Throw { value, line, col }) => {
                    return Err(self.runtime(
                        &format!("uncaught throw: {}", value.to_print()),
                        line,
                        col,
                    ));
                }
                Err(err) => return Err(err),
            }
        } else {
            ret
        };
        self.flush_deferred()?;
        self.drain_event_loop()?;
        Ok(ret)
    }

    fn call_user(
        &mut self,
        func: &FnDecl,
        args: Vec<Value>,
        line: u32,
        col: u32,
        caps: Option<HashMap<String, Binding>>,
        code: Option<Rc<BcFn>>,
    ) -> Result<Value, Halt> {
        if func.is_async {
            let fut = Rc::new(RefCell::new(FutureData::pending()));
            let fut_task = fut.clone();
            let func = Rc::new(func.clone());
            let caps = caps.map(|caps| {
                caps.into_iter()
                    .map(|(k, v)| {
                        (
                            k,
                            Binding {
                                value: v.value.clone(),
                                is_const: v.is_const,
                            },
                        )
                    })
                    .collect::<HashMap<_, _>>()
            });
            self.microtasks.push_back(Box::new(move |ip| {
                match ip.run_user_body(&func, args, line, col, caps, code) {
                    Ok(v) => {
                        ip.settle(&fut_task, v);
                        Ok(())
                    }
                    Err(Halt::Throw { value, line, col }) => {
                        ip.fail_future(&fut_task, value, line, col);
                        Ok(())
                    }
                    Err(err) => Err(err),
                }
            }));
            return Ok(Value::Future(fut));
        }
        self.run_user_body(func, args, line, col, caps, code)
    }

    fn run_user_body(
        &mut self,
        func: &FnDecl,
        mut args: Vec<Value>,
        line: u32,
        col: u32,
        caps: Option<HashMap<String, Binding>>,
        code: Option<Rc<BcFn>>,
    ) -> Result<Value, Halt> {
        if self.call_depth >= MAX_CALL_DEPTH {
            return Err(self.runtime("call stack overflow", line, col));
        }
        if args.len() < func.params.len() {
            for i in args.len()..func.params.len() {
                match func.param_defaults.get(i).and_then(|d| d.as_ref()) {
                    Some(default) => args.push(self.eval(default)?),
                    None => break,
                }
            }
        }
        if args.len() != func.params.len() && !(func.is_vararg && args.len() > func.params.len()) {
            return Err(self.runtime(
                &format!("{} takes {} argument(s)", func.name, func.params.len()),
                line,
                col,
            ));
        }
        if func.is_deprecated {
            self.emit(&format!("warning: '{}' is deprecated\n", func.name));
        }
        self.call_depth += 1;
        let prev_module = self.checker.current_module();
        *self.checker.current_module.borrow_mut() = func.module.clone();
        let vm_caps = caps.clone();
        let mut frame = HashMap::new();
        if let Some(caps) = caps {
            for (k, v) in caps {
                frame.insert(
                    k,
                    Binding {
                        value: v.value,
                        is_const: v.is_const,
                    },
                );
            }
        }
        for (i, pname) in func.params.iter().enumerate() {
            let val = args.get(i).cloned().unwrap_or(Value::Void);
            frame.insert(
                pname.clone(),
                Binding {
                    value: val,
                    is_const: false,
                },
            );
        }
        self.env.push(frame);
        let code = code.or_else(|| {
            self.compiled
                .get(&compile::fn_key(func))
                .and_then(|c| c.clone())
        });
        if let Some(code) = code {
            let result = vm::exec(self, &code, &args, vm_caps.as_ref());
            self.env.pop();
            *self.checker.current_module.borrow_mut() = prev_module;
            self.call_depth -= 1;
            return match result {
                Ok(v) => Ok(v),
                Err(Halt::Throw { value, .. }) => Err(Halt::Throw { value, line, col }),
                Err(err) => Err(err),
            };
        }
        let flow = self.exec_block(&func.body);
        self.env.pop();
        *self.checker.current_module.borrow_mut() = prev_module;
        self.call_depth -= 1;
        match flow {
            Ok(Flow::Throw(value)) => Err(Halt::Throw { value, line, col }),
            Ok(Flow::Return(value)) => Ok(value),
            Ok(Flow::Break) => Err(self.runtime("break outside loop", func.span.line, 1)),
            Ok(Flow::Continue) => Err(self.runtime("continue outside loop", func.span.line, 1)),
            Ok(Flow::Next) => Ok(Value::Void),
            Err(err) => Err(err),
        }
    }

    fn exec_block(&mut self, stmts: &[Stmt]) -> Result<Flow, Halt> {
        for stmt in stmts {
            let flow = self.exec_stmt(stmt)?;
            if !matches!(flow, Flow::Next) {
                return Ok(flow);
            }
        }
        Ok(Flow::Next)
    }

    fn exec_stmt(&mut self, stmt: &Stmt) -> Result<Flow, Halt> {
        let line = stmt.span.line;
        let col = stmt.span.col;
        match stmt.kind {
            StmtKind::Expr => {
                if let Some(expr) = &stmt.expr {
                    self.eval(expr)?;
                }
                Ok(Flow::Next)
            }
            StmtKind::Var => {
                let value = self.eval_opt(stmt.expr.as_ref(), line, col)?;
                self.env.last_mut().unwrap().insert(
                    stmt.name.clone(),
                    Binding {
                        value,
                        is_const: false,
                    },
                );
                Ok(Flow::Next)
            }
            StmtKind::Const => {
                let value = self.eval_opt(stmt.expr.as_ref(), line, col)?;
                self.env.last_mut().unwrap().insert(
                    stmt.name.clone(),
                    Binding {
                        value,
                        is_const: true,
                    },
                );
                Ok(Flow::Next)
            }
            StmtKind::Assign => {
                let rhs = self.eval_opt(stmt.expr.as_ref(), line, col)?;
                self.assign_name(&stmt.name, &stmt.op, rhs, line, col)?;
                Ok(Flow::Next)
            }
            StmtKind::IndexAssign => {
                let target = stmt
                    .target
                    .as_ref()
                    .ok_or_else(|| self.runtime("invalid index assignment", line, col))?;
                if target.kids.len() < 2 {
                    return Err(self.runtime("invalid index assignment", line, col));
                }
                let obj = self.eval(&target.kids[0])?;
                let idx = self.eval(&target.kids[1])?;
                let mut rhs = self.eval_opt(stmt.expr.as_ref(), line, col)?;
                if !stmt.op.is_empty() && stmt.op != "=" {
                    let cur = self.index_get(&obj, &idx, line, col)?;
                    let op = &stmt.op[..stmt.op.len() - 1];
                    rhs = self.apply_binop(op, cur, rhs, line, col)?;
                }
                self.index_set(&obj, &idx, rhs, line, col)?;
                Ok(Flow::Next)
            }
            StmtKind::Return => {
                let value = if stmt.has_expr {
                    self.eval_opt(stmt.expr.as_ref(), line, col)?
                } else {
                    Value::Void
                };
                Ok(Flow::Return(value))
            }
            StmtKind::If => {
                let cond = self.eval_opt(stmt.expr.as_ref(), line, col)?;
                if cond.truthy() {
                    self.exec_block(&stmt.body)
                } else {
                    self.exec_block(&stmt.else_body)
                }
            }
            StmtKind::While => {
                self.loop_depth += 1;
                let mut result = Ok(Flow::Next);
                loop {
                    let cond = match self.eval_opt(stmt.expr.as_ref(), line, col) {
                        Ok(v) => v,
                        Err(err) => {
                            result = Err(err);
                            break;
                        }
                    };
                    if !cond.truthy() {
                        break;
                    }
                    match self.exec_block(&stmt.body) {
                        Ok(Flow::Break) => break,
                        Ok(Flow::Continue) | Ok(Flow::Next) => {}
                        other => {
                            result = other;
                            break;
                        }
                    }
                }
                self.loop_depth -= 1;
                result
            }
            StmtKind::For => {
                let iter = self.eval_opt(stmt.expr.as_ref(), line, col)?;
                let items = self.iter_items(iter, line, col)?;
                let existed = self.env.last().unwrap().contains_key(&stmt.name);
                let saved = if existed {
                    self.env.last().unwrap().get(&stmt.name).cloned()
                } else {
                    None
                };
                self.loop_depth += 1;
                let mut result = Ok(Flow::Next);
                for item in items {
                    self.env.last_mut().unwrap().insert(
                        stmt.name.clone(),
                        Binding {
                            value: item,
                            is_const: false,
                        },
                    );
                    match self.exec_block(&stmt.body) {
                        Ok(Flow::Break) => break,
                        Ok(Flow::Continue) | Ok(Flow::Next) => {}
                        other => {
                            result = other;
                            break;
                        }
                    }
                }
                self.loop_depth -= 1;
                if let Some(saved) = saved {
                    self.env
                        .last_mut()
                        .unwrap()
                        .insert(stmt.name.clone(), saved);
                } else {
                    self.env.last_mut().unwrap().remove(&stmt.name);
                }
                result
            }
            StmtKind::Match => self.exec_match(stmt),
            StmtKind::Pass | StmtKind::Comment => Ok(Flow::Next),
            StmtKind::Break => {
                if self.loop_depth == 0 {
                    return Err(self.runtime("break outside loop", line, col));
                }
                Ok(Flow::Break)
            }
            StmtKind::Continue => {
                if self.loop_depth == 0 {
                    return Err(self.runtime("continue outside loop", line, col));
                }
                Ok(Flow::Continue)
            }
            StmtKind::Throw => Ok(Flow::Throw(self.eval_opt(stmt.expr.as_ref(), line, col)?)),
            StmtKind::Do => self.exec_do(stmt),
            StmtKind::FieldAssign => {
                let target = stmt
                    .target
                    .as_ref()
                    .ok_or_else(|| self.runtime("cannot assign field on ", line, col))?;
                let obj = if target.kind == ExprKind::Var && target.text == "super" {
                    self.self_object(line, col)?
                } else {
                    self.eval(target)?
                };
                let Value::Struct(rec) = &obj else {
                    return Err(self.runtime(
                        &format!("cannot assign field on {}", obj.to_print()),
                        line,
                        col,
                    ));
                };
                let (has, ty, cur) = {
                    let data = rec.borrow();
                    (
                        data.fields.contains_key(&stmt.name),
                        data.name.clone(),
                        data.fields.get(&stmt.name).cloned(),
                    )
                };
                if !has {
                    return Err(self.runtime(
                        &format!("struct {ty} has no field '{}'", stmt.name),
                        line,
                        col,
                    ));
                }
                if self.checker.is_data_type(&ty) {
                    return Err(self.runtime(
                        &format!("cannot assign to data field '{}'", stmt.name),
                        line,
                        col,
                    ));
                }
                let rhs = self.eval_opt(stmt.expr.as_ref(), line, col)?;
                let newv =
                    self.apply_assign_op(&stmt.op, cur.unwrap_or(Value::Void), rhs, line, col)?;
                rec.borrow_mut().fields.insert(stmt.name.clone(), newv);
                Ok(Flow::Next)
            }
        }
    }

    fn exec_do(&mut self, stmt: &Stmt) -> Result<Flow, Halt> {
        match self.exec_block(&stmt.body) {
            Ok(Flow::Throw(value)) => {
                if stmt.else_body.is_empty() {
                    return Ok(Flow::Throw(value));
                }
                self.bind_catch(&stmt.name, value);
                self.exec_block(&stmt.else_body)
            }
            Err(Halt::Throw { value, line, col }) => {
                if stmt.else_body.is_empty() {
                    return Err(Halt::Throw { value, line, col });
                }
                self.bind_catch(&stmt.name, value);
                self.exec_block(&stmt.else_body)
            }
            Err(Halt::Runtime(msg)) => {
                if stmt.else_body.is_empty() {
                    return Err(Halt::Runtime(msg));
                }
                self.bind_catch(&stmt.name, Value::String(msg));
                self.exec_block(&stmt.else_body)
            }
            other => other,
        }
    }

    fn bind_catch(&mut self, name: &str, value: Value) {
        if let Some(frame) = self.env.last_mut() {
            frame.insert(
                name.to_string(),
                Binding {
                    value,
                    is_const: false,
                },
            );
        }
    }

    fn exec_match(&mut self, stmt: &Stmt) -> Result<Flow, Halt> {
        let value = self.eval_opt(stmt.expr.as_ref(), stmt.span.line, stmt.span.col)?;
        for arm in &stmt.arms {
            let hit = match arm.pat {
                Pat::Wildcard => true,
                Pat::Int => self.match_lit(&value, &Value::Int(arm.number)),
                Pat::Float => self.match_lit(&value, &Value::Float(arm.real)),
                Pat::String => self.match_lit(&value, &Value::String(arm.text.clone())),
                Pat::Bool => self.match_lit(&value, &Value::Bool(arm.boolean_value)),
                Pat::Variant => {
                    self.match_variant(&value, &arm.name, stmt.span.line, stmt.span.col)?
                }
            };
            if !hit {
                continue;
            }
            let mut saved = HashMap::new();
            let mut added = Vec::new();
            if arm.pat == Pat::Variant {
                self.bind_variant(
                    &value,
                    &arm.binds,
                    &arm.field_names,
                    &mut saved,
                    &mut added,
                    stmt.span.line,
                    stmt.span.col,
                )?;
            }
            let result = self.exec_block(&arm.body);
            for (k, v) in saved {
                self.env.last_mut().unwrap().insert(k, v);
            }
            for name in added {
                self.env.last_mut().unwrap().remove(&name);
            }
            return result;
        }
        Ok(Flow::Next)
    }

    fn match_lit(&self, value: &Value, pat: &Value) -> bool {
        match pat {
            Value::Int(n) => matches!(value, Value::Int(v) if v == n),
            Value::Float(n) => value.is_numeric() && (value.as_f64() - n).abs() < 1e-9,
            Value::String(s) => matches!(value, Value::String(v) if v == s),
            Value::Bool(b) => matches!(value, Value::Bool(v) if v == b),
            _ => false,
        }
    }

    fn match_variant(&self, value: &Value, name: &str, line: u32, col: u32) -> Result<bool, Halt> {
        match value {
            Value::Enum { variant, .. } => Ok(variant == name),
            _ => Err(self.runtime(
                &format!(
                    "match pattern '{name}' does not match value of type {}",
                    value.to_print()
                ),
                line,
                col,
            )),
        }
    }

    fn match_bind(
        &self,
        value: &Value,
        index: i64,
        field: &str,
        line: u32,
        col: u32,
    ) -> Result<Option<Value>, Halt> {
        let Value::Enum {
            ty,
            variant,
            payload,
        } = value
        else {
            return Ok(None);
        };
        if !field.is_empty() {
            let Some(en) = self.checker.find_enum(ty) else {
                return Err(self.runtime(&format!("undefined enum '{ty}'"), line, col));
            };
            let Some(var) = en.variants.iter().find(|v| &v.name == variant) else {
                return Err(self.runtime(&format!("undefined enum '{ty}'"), line, col));
            };
            let Some(idx) = var.field_names.iter().position(|n| n == field) else {
                return Err(self.runtime(
                    &format!("{ty}.{variant} has no field '{field}'"),
                    line,
                    col,
                ));
            };
            if idx >= payload.len() {
                return Err(self.runtime(
                    &format!("{ty}.{variant} field '{field}' is missing"),
                    line,
                    col,
                ));
            }
            return Ok(Some(payload[idx].clone()));
        }
        if index < 0 {
            if payload.len() == 1 {
                return Ok(Some(payload[0].clone()));
            }
            return Ok(Some(Value::array(payload.clone())));
        }
        let index = index as usize;
        if index >= payload.len() {
            return Ok(None);
        }
        Ok(Some(payload[index].clone()))
    }

    fn bind_variant(
        &mut self,
        value: &Value,
        binds: &[String],
        field_names: &[String],
        saved: &mut HashMap<String, Binding>,
        added: &mut Vec<String>,
        line: u32,
        col: u32,
    ) -> Result<(), Halt> {
        let named = field_names.iter().any(|f| !f.is_empty());
        if named {
            for (i, name) in binds.iter().enumerate() {
                let field = field_names.get(i).map(String::as_str).unwrap_or("");
                if field.is_empty() {
                    continue;
                }
                let bound = self
                    .match_bind(value, 0, field, line, col)?
                    .unwrap_or(Value::Void);
                self.bind_match_name(name, bound, saved, added);
            }
        } else if binds.len() == 1 {
            let bound = self
                .match_bind(value, -1, "", line, col)?
                .unwrap_or(Value::Void);
            self.bind_match_name(&binds[0], bound, saved, added);
        } else {
            for (i, name) in binds.iter().enumerate() {
                if let Some(bound) = self.match_bind(value, i as i64, "", line, col)? {
                    self.bind_match_name(name, bound, saved, added);
                }
            }
        }
        Ok(())
    }

    fn bind_match_name(
        &mut self,
        name: &str,
        value: Value,
        saved: &mut HashMap<String, Binding>,
        added: &mut Vec<String>,
    ) {
        if name.is_empty() || name == "_" {
            return;
        }
        let frame = self.env.last_mut().unwrap();
        if let Some(prev) = frame.get(name).cloned() {
            saved.insert(name.to_string(), prev);
        } else {
            added.push(name.to_string());
        }
        frame.insert(
            name.to_string(),
            Binding {
                value,
                is_const: false,
            },
        );
    }

    fn eval_opt(&mut self, expr: Option<&Expr>, line: u32, col: u32) -> Result<Value, Halt> {
        match expr {
            Some(expr) => self.eval(expr),
            None => Err(self.runtime("missing expression", line, col)),
        }
    }

    fn eval(&mut self, e: &Expr) -> Result<Value, Halt> {
        let line = e.span.line;
        let col = e.span.col;
        match e.kind {
            ExprKind::Int => Ok(Value::Int(e.number)),
            ExprKind::Float => Ok(Value::Float(e.real)),
            ExprKind::String => Ok(Value::String(e.text.clone())),
            ExprKind::Bool => Ok(Value::Bool(e.boolean_value)),
            ExprKind::Var => self.resolve_value(&e.text, line, col),
            ExprKind::Unary => {
                let v = self.eval(
                    e.kids
                        .first()
                        .ok_or_else(|| self.runtime("invalid unary", line, col))?,
                )?;
                self.apply_unary(&e.text, v, line, col)
            }
            ExprKind::Binary => {
                if e.kids.len() < 2 {
                    return Err(self.runtime("invalid binary", line, col));
                }
                if e.text == "&&" {
                    let a = self.eval(&e.kids[0])?;
                    let b = if a.truthy() {
                        self.eval(&e.kids[1])?.truthy()
                    } else {
                        false
                    };
                    return Ok(Value::Bool(a.truthy() && b));
                }
                if e.text == "||" {
                    let a = self.eval(&e.kids[0])?;
                    let b = if a.truthy() {
                        true
                    } else {
                        self.eval(&e.kids[1])?.truthy()
                    };
                    return Ok(Value::Bool(a.truthy() || b));
                }
                let a = self.eval(&e.kids[0])?;
                let b = self.eval(&e.kids[1])?;
                self.apply_binop(&e.text, a, b, line, col)
            }
            ExprKind::Call => self.eval_call(e),
            ExprKind::Array => {
                let mut elems = Vec::new();
                for kid in &e.kids {
                    elems.push(self.eval(kid)?);
                }
                Ok(Value::array(elems))
            }
            ExprKind::Range => {
                if e.kids.len() < 2 {
                    return Err(self.runtime("invalid range", line, col));
                }
                let a = self.eval(&e.kids[0])?;
                let b = self.eval(&e.kids[1])?;
                let Value::Int(start) = a else {
                    return Err(self.runtime(
                        "range start must be Int",
                        e.kids[0].span.line,
                        e.kids[0].span.col,
                    ));
                };
                let Value::Int(end) = b else {
                    return Err(self.runtime(
                        "range end must be Int",
                        e.kids[1].span.line,
                        e.kids[1].span.col,
                    ));
                };
                Ok(Value::Range {
                    start,
                    end,
                    inclusive: e.boolean_value,
                })
            }
            ExprKind::Index => {
                if e.kids.len() < 2 {
                    return Err(self.runtime("invalid index", line, col));
                }
                let obj = self.eval(&e.kids[0])?;
                let idx = self.eval(&e.kids[1])?;
                self.index_get(&obj, &idx, line, col)
            }
            ExprKind::Try => self.eval(
                e.kids
                    .first()
                    .ok_or_else(|| self.runtime("invalid try", line, col))?,
            ),
            ExprKind::Cast => {
                if e.kids.is_empty() {
                    Ok(Value::Void)
                } else {
                    self.eval(&e.kids[0])
                }
            }
            ExprKind::Is => {
                Err(self.runtime("'is' runs when the script plays in Godot", line, col))
            }
            ExprKind::If => {
                if e.kids.len() < 3 {
                    return Err(self.runtime("invalid if", line, col));
                }
                if self.eval(&e.kids[0])?.truthy() {
                    self.eval(&e.kids[1])
                } else {
                    self.eval(&e.kids[2])
                }
            }
            ExprKind::Format => {
                let mut acc = Value::String(String::new());
                for kid in &e.kids {
                    let piece = self.eval(kid)?;
                    acc = self.apply_binop("+", acc, piece, line, col)?;
                }
                Ok(acc)
            }
            ExprKind::NodePath => {
                Err(self.runtime("node path runs when the script plays in Godot", line, col))
            }
            ExprKind::New => {
                let head = type_head(&e.text);
                if self.checker.find_struct(&head).is_some()
                    && self.checker.native_base(&head).is_empty()
                {
                    return self.struct_lit(&head, &[], &[], line, col);
                }
                Err(self.runtime("new is only available in a Godot script", line, col))
            }
            ExprKind::Null => Ok(Value::Void),
            ExprKind::Member => self.eval_member(e),
            ExprKind::MethodCall => self.eval_method_call(e),
            ExprKind::StructLit => self.eval_struct_lit(e),
            ExprKind::Map => self.eval_map_lit(e),
            ExprKind::Lambda => self.eval_lambda(e),
            ExprKind::Await => {
                if e.kids.is_empty() {
                    return Err(self.runtime("invalid await", line, col));
                }
                if e.boolean_value {
                    return Err(self.runtime(
                        "await signal runs when the script plays in Godot",
                        line,
                        col,
                    ));
                }
                let f = self.eval(&e.kids[0])?;
                let Value::Future(fut) = f else {
                    return Err(self.runtime("can only await a Future", line, col));
                };
                self.await_future(&fut, line, col)
            }
        }
    }
}
impl Interp {
    fn eval_member(&mut self, e: &Expr) -> Result<Value, Halt> {
        let line = e.span.line;
        let col = e.span.col;
        if let Some(recv) = e.kids.first() {
            if recv.kind == ExprKind::Var
                && self.find_local(&recv.text).is_none()
                && self.find_global(&recv.text).is_none()
                && self.checker.find_struct(&recv.text).is_some()
            {
                if let Some(n) = self.checker.lookup_const(&recv.text, &e.text) {
                    return Ok(Value::Int(n));
                }
            }
        }
        let obj = if e
            .kids
            .first()
            .is_some_and(|k| k.kind == ExprKind::Var && k.text == "super")
        {
            self.self_object(line, col)?
        } else {
            let recv = e
                .kids
                .first()
                .ok_or_else(|| self.runtime("cannot read field", line, col))?;
            self.eval(recv)?
        };
        self.read_member(obj, &e.text, line, col)
    }

    fn read_member(&mut self, obj: Value, name: &str, line: u32, col: u32) -> Result<Value, Halt> {
        if let Value::EnumType(ty) = &obj {
            let Some(en) = self.checker.find_enum(ty) else {
                return Err(self.runtime(&format!("undefined enum '{ty}'"), line, col));
            };
            return self.construct_enum(&en, name, Vec::new(), line, col);
        }
        if matches!(obj, Value::Array(_) | Value::String(_) | Value::Map(_)) {
            return self.read_value_member(&obj, name, line, col);
        }
        let Value::Struct(rec) = &obj else {
            return Err(self.runtime(
                &format!("cannot read field on {}", obj.to_print()),
                line,
                col,
            ));
        };
        let (field, ty) = {
            let data = rec.borrow();
            (data.fields.get(name).cloned(), data.name.clone())
        };
        if let Some(field) = field {
            return Ok(field);
        }
        if self.checker.lookup_type_signal(&ty, name).is_some() {
            return Ok(Value::SignalRef {
                name: name.to_string(),
                owner: Some(rec.clone()),
            });
        }
        Err(self.runtime(&format!("struct {ty} has no field '{name}'"), line, col))
    }

    fn eval_method_call(&mut self, e: &Expr) -> Result<Value, Halt> {
        let line = e.span.line;
        let col = e.span.col;
        let mut args = Vec::new();
        for kid in e.kids.iter().skip(1) {
            args.push(self.eval(kid)?);
        }
        let Some(recv) = e.kids.first() else {
            return Err(self.runtime("cannot call method", line, col));
        };
        if recv.kind == ExprKind::Var && recv.text == "super" {
            return self.call_super(&e.text, args, line, col);
        }
        if recv.kind == ExprKind::Var
            && self.find_local(&recv.text).is_none()
            && self.find_global(&recv.text).is_none()
        {
            let (defined, func) = self.checker.lookup_static(&recv.text, &e.text);
            let _ = defined;
            if func.is_some() && self.checker.find_struct(&recv.text).is_some() {
                let func = func.unwrap();
                let file = func.file.replace('\\', "/");
                if file.ends_with("/godot/lib.rg") {
                    return Err(self.runtime(
                        &format!(
                            "'{}.{}' is available when the script runs in Godot",
                            recv.text, e.text
                        ),
                        line,
                        col,
                    ));
                }
                return self.call_user(&func, args, line, col, None, None);
            }
        }
        if recv.kind == ExprKind::Var {
            if let Some(selfb) = self.find_local("self") {
                if let Value::Struct(rec) = &selfb.value {
                    let ty = rec.borrow().name.clone();
                    if self.checker.lookup_type_signal(&ty, &recv.text).is_some() {
                        return self.dispatch_signal(
                            Value::SignalRef {
                                name: recv.text.clone(),
                                owner: Some(rec.clone()),
                            },
                            &e.text,
                            args,
                            line,
                            col,
                        );
                    }
                }
            }
            if self.checker.signal_arity.borrow().contains_key(&recv.text) {
                return self.call_signal(&recv.text, &e.text, args, line, col);
            }
            if let Some(bound) = self.checker.find_module_bind(&recv.text) {
                return self.call_module(&bound, &e.text, args, line, col);
            }
            let resolvable = self.find_local(&recv.text).is_some()
                || self.field_on_self(&recv.text).is_some()
                || self.find_global(&recv.text).is_some()
                || self.checker.find_enum(&recv.text).is_some();
            if resolvable {
                let obj = self.resolve_value(&recv.text, recv.span.line, recv.span.col)?;
                return self.call_method(obj, &e.text, args, line, col);
            }
            return self.call_builtin(&recv.text, &e.text, args, line, col);
        }
        match self.module_path(recv)? {
            Some(nested) => self.call_module(&nested, &e.text, args, line, col),
            None => {
                let obj = self.eval(recv)?;
                self.call_method(obj, &e.text, args, line, col)
            }
        }
    }

    fn call_method(
        &mut self,
        obj: Value,
        name: &str,
        args: Vec<Value>,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        if matches!(obj, Value::SignalRef { .. }) {
            return self.dispatch_signal(obj, name, args, line, col);
        }
        if let Value::EnumType(ty) = &obj {
            let key = ty.clone();
            let Some(en) = self.checker.find_enum(&key) else {
                return Err(self.runtime(&format!("undefined enum '{key}'"), line, col));
            };
            return self.construct_enum(&en, name, args, line, col);
        }
        if let Value::Struct(rec) = &obj {
            let ty = rec.borrow().name.clone();
            return self.invoke_type_method(obj, &ty, name, args, line, col);
        }
        if matches!(
            obj,
            Value::Array(_) | Value::String(_) | Value::Map(_) | Value::Future(_)
        ) {
            return self.call_value_method(obj, name, args, line, col);
        }
        let ty = obj.type_name();
        if let Some(ufcs) = self.checker.find_ufcs(name, &ty) {
            return self.call_ufcs(&ufcs, obj, args, line, col);
        }
        Err(self.runtime(
            &format!("cannot call method '{name}' on {}", obj.to_print()),
            line,
            col,
        ))
    }

    fn eval_struct_lit(&mut self, e: &Expr) -> Result<Value, Halt> {
        let mut provided = Vec::new();
        for kid in &e.kids {
            provided.push(self.eval(kid)?);
        }
        self.struct_lit(&e.text, &e.names, &provided, e.span.line, e.span.col)
    }

    fn struct_lit(
        &mut self,
        type_name: &str,
        names: &[String],
        provided: &[Value],
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        let head = type_head(type_name);
        if self.checker.class_abstract.borrow().get(&head).copied() == Some(true) {
            return Err(self.runtime(
                &format!("cannot construct abstract class '{head}'"),
                line,
                col,
            ));
        }
        let Some(decl) = self.checker.find_struct(&head) else {
            return Err(self.runtime(&format!("undefined struct '{head}'"), line, col));
        };
        let (name, fields, field_types, field_optional) = {
            let d = decl.borrow();
            (
                d.name.clone(),
                d.fields.clone(),
                d.field_types.clone(),
                d.field_optional.clone(),
            )
        };
        let mut data = StructData {
            name: name.clone(),
            order: fields.clone(),
            fields: HashMap::new(),
            listeners: HashMap::new(),
        };
        for (i, field) in names.iter().enumerate() {
            if !fields.iter().any(|f| f == field) {
                return Err(self.runtime(&format!("unknown field '{field}' on {name}"), line, col));
            }
            if data.fields.contains_key(field) {
                return Err(self.runtime(
                    &format!("duplicate field '{field}' on {name}"),
                    line,
                    col,
                ));
            }
            let value = provided.get(i).cloned().unwrap_or(Value::Void);
            data.fields.insert(field.clone(), value);
        }
        let defaults = self.checker.field_defaults.borrow().get(&name).cloned();
        for field in &fields {
            if data.fields.contains_key(field) {
                continue;
            }
            if let Some(expr) = defaults.as_ref().and_then(|m| m.get(field)) {
                let value = self.eval(expr)?;
                data.fields.insert(field.clone(), value);
                continue;
            }
            let optional = fields
                .iter()
                .position(|f| f == field)
                .and_then(|i| field_optional.get(i).copied())
                .unwrap_or(false);
            if optional {
                let ty = fields
                    .iter()
                    .position(|f| f == field)
                    .and_then(|i| field_types.get(i))
                    .map(String::as_str)
                    .unwrap_or("");
                data.fields.insert(field.clone(), zero_of_type(ty));
                continue;
            }
            return Err(self.runtime(&format!("missing field '{field}' on {name}"), line, col));
        }
        self.init_instance_signals(&mut data);
        Ok(Value::struc(data))
    }

    fn eval_map_lit(&mut self, e: &Expr) -> Result<Value, Halt> {
        if e.kids.len() % 2 != 0 {
            return Err(self.runtime("invalid map literal", e.span.line, e.span.col));
        }
        let mut data = MapData {
            order: Vec::new(),
            fields: HashMap::new(),
        };
        let mut i = 0;
        while i < e.kids.len() {
            let key = self.eval(&e.kids[i])?;
            let Value::String(s) = key else {
                return Err(self.runtime(
                    "map key must be String",
                    e.kids[i].span.line,
                    e.kids[i].span.col,
                ));
            };
            let val = self.eval(&e.kids[i + 1])?;
            if !data.fields.contains_key(&s) {
                data.order.push(s.clone());
            }
            data.fields.insert(s, val);
            i += 2;
        }
        Ok(Value::map(data))
    }

    fn eval_lambda(&mut self, e: &Expr) -> Result<Value, Halt> {
        let Some(func) = e.lambda.as_ref() else {
            return Err(self.runtime("invalid closure", e.span.line, e.span.col));
        };
        let mut bound: HashSet<String> = func.params.iter().cloned().collect();
        let mut free = HashSet::new();
        for st in &func.body {
            collect_free_stmt(st, &mut bound, &mut free);
        }
        let mut caps = HashMap::new();
        let mut captured_self = false;
        for name in free {
            if self.skip_free_name(&name) {
                continue;
            }
            if let Some(b) = self.find_local(&name) {
                if name == "self" {
                    captured_self = true;
                }
                caps.insert(name, b);
                continue;
            }
            if self.field_on_self(&name).is_some() || self.is_signal_on_self(&name) {
                if !captured_self {
                    if let Some(selfb) = self.find_local("self") {
                        caps.insert("self".to_string(), selfb);
                        captured_self = true;
                    }
                }
            }
        }
        Ok(Value::closure(ClosureData {
            func: Rc::new((**func).clone()),
            caps,
            code: None,
        }))
    }

    fn skip_free_name(&self, name: &str) -> bool {
        if skip_capture(name) || is_host(name) || self.fns.contains_key(name) {
            return true;
        }
        if self.checker.find_local_fn(name).is_some() {
            return true;
        }
        if self.checker.structs.borrow().contains_key(name) {
            return true;
        }
        if self.checker.all_types.borrow().contains_key(name) {
            return true;
        }
        if self.checker.enums.borrow().contains_key(name) {
            return true;
        }
        if self.checker.traits.borrow().contains_key(name) {
            return true;
        }
        if self.checker.signal_arity.borrow().contains_key(name) {
            return true;
        }
        self.checker.module_binds.borrow().contains_key(name)
    }

    fn eval_call(&mut self, e: &Expr) -> Result<Value, Halt> {
        if e.text.is_empty() {
            if e.kids.is_empty() {
                return Err(self.runtime("can only call a function", e.span.line, e.span.col));
            }
            let func = self.eval(&e.kids[0])?;
            let mut args = Vec::new();
            for kid in e.kids.iter().skip(1) {
                args.push(self.eval(kid)?);
            }
            return self.call_fn_value(func, args, e.span.line, e.span.col);
        }
        let mut args = Vec::new();
        for kid in &e.kids {
            args.push(self.eval(kid)?);
        }
        self.call(&e.module, &e.text, args, e.span.line, e.span.col)
    }

    fn call(
        &mut self,
        module: &str,
        name: &str,
        args: Vec<Value>,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        if module.is_empty() {
            if let Some(func) = self.checker.find_local_fn(name) {
                return self.call_user(&func, args, line, col, None, None);
            }
            if let Some(local) = self.find_var(name) {
                if matches!(local.value, Value::FnRef { .. }) {
                    return self.call_fn_value(local.value, args, line, col);
                }
            }
            if let Some(selfb) = self.find_local("self") {
                if let Value::Struct(rec) = &selfb.value {
                    let ty = rec.borrow().name.clone();
                    if self.checker.lookup_method(&ty, name).1.is_some() {
                        return self.invoke_type_method(selfb.value, &ty, name, args, line, col);
                    }
                }
            }
        } else if self.checker.signal_arity.borrow().contains_key(module) {
            return self.call_signal(module, name, args, line, col);
        }
        self.call_builtin(module, name, args, line, col)
    }

    fn module_path(&self, e: &Expr) -> Result<Option<String>, Halt> {
        match e.kind {
            ExprKind::Var => Ok(self.checker.find_module_bind(&e.text)),
            ExprKind::Member => {
                let Some(parent_expr) = e.kids.first() else {
                    return Ok(None);
                };
                let Some(parent) = self.module_path(parent_expr)? else {
                    return Ok(None);
                };
                let Some(lit) = self.checker.loaded.borrow().get(&parent).cloned() else {
                    return Ok(None);
                };
                let internal = self.checker.current_module() == parent;
                let hit = {
                    let lit = lit.borrow();
                    let map = if internal {
                        &lit.modules
                    } else {
                        &lit.export_mods
                    };
                    map.get(&e.text).cloned()
                };
                match hit {
                    Some(hit) => Ok(Some(hit)),
                    None => Err(self.runtime(
                        &format!("module '{parent}' has no export '{}'", e.text),
                        e.span.line,
                        e.span.col,
                    )),
                }
            }
            _ => Ok(None),
        }
    }

    fn call_module(
        &mut self,
        mod_name: &str,
        name: &str,
        args: Vec<Value>,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        let Some(module) = self.checker.loaded.borrow().get(mod_name).cloned() else {
            return Err(self.runtime(&format!("unknown function {mod_name}.{name}"), line, col));
        };
        let internal = self.checker.current_module() == mod_name;
        let func = {
            let module = module.borrow();
            if internal {
                module
                    .fns
                    .get(name)
                    .cloned()
                    .or_else(|| module.from_fns.get(name).cloned())
            } else {
                module.exports.get(name).cloned()
            }
        };
        let Some(func) = func else {
            return Err(self.runtime(
                &format!("module '{mod_name}' has no export '{name}'"),
                line,
                col,
            ));
        };
        self.call_user(&func, args, line, col, None, None)
    }

    fn call_fn_value(
        &mut self,
        func: Value,
        args: Vec<Value>,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        let Value::FnRef { name, clo } = func else {
            return Err(self.runtime("can only call a function", line, col));
        };
        if let Some(clo) = clo {
            return self.call_user(
                &clo.func,
                args,
                line,
                col,
                Some(clo.caps.clone()),
                clo.code.clone(),
            );
        }
        let Some(decl) = self.fns.get(&name).cloned() else {
            return Err(self.runtime(&format!("unknown function '{name}'"), line, col));
        };
        self.call_user(&decl, args, line, col, None, None)
    }

    fn call_builtin(
        &mut self,
        module: &str,
        name: &str,
        args: Vec<Value>,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        if module.is_empty() && name == "print" {
            let mut parts = String::new();
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    parts.push(' ');
                }
                parts.push_str(&arg.to_print());
            }
            parts.push('\n');
            self.emit(&parts);
            return Ok(Value::Void);
        }
        if module.is_empty() && name == "assert" {
            if args.len() != 1 {
                return Err(self.runtime("assert takes 1 argument", line, col));
            }
            if !args[0].truthy() {
                return Err(self.runtime("assertion failed", line, col));
            }
            return Ok(Value::Void);
        }
        if module.is_empty() && name == "len" {
            if args.len() != 1 {
                return Err(self.runtime("len takes 1 argument", line, col));
            }
            return match &args[0] {
                Value::Array(items) => Ok(Value::Int(items.borrow().len() as i64)),
                Value::String(s) => Ok(Value::Int(utf16_len(s))),
                Value::Map(dict) => Ok(Value::Int(dict.borrow().fields.len() as i64)),
                _ => Err(self.runtime("len expects Array, String, or Map", line, col)),
            };
        }
        if (module.is_empty() && name == "argv") || (module == "process" && name == "argv") {
            if args.len() != 1 {
                return Err(self.runtime("argv takes 1 argument", line, col));
            }
            let Value::Int(i) = args[0] else {
                return Err(self.runtime("argv index must be Int", line, col));
            };
            if i < 0 || i as usize >= self.argv.len() {
                return Err(self.runtime("argv index out of range", line, col));
            }
            return Ok(Value::String(self.argv[i as usize].clone()));
        }
        if (module.is_empty() && name == "argv_len") || (module == "process" && name == "argc") {
            if !args.is_empty() {
                let msg = if module.is_empty() {
                    "argv_len takes 0 arguments"
                } else {
                    "process.argc takes 0 arguments"
                };
                return Err(self.runtime(msg, line, col));
            }
            return Ok(Value::Int(self.argv.len() as i64));
        }
        if module == "checks" {
            return self.call_checks(name, &args, line, col);
        }
        if module == "__math" {
            return self.call_math(name, &args, line, col);
        }
        if module == "__str" {
            return self.call_str(name, &args, line, col);
        }
        if module == "__io" {
            return self.call_io(name, &args, line, col);
        }
        if module == "__uuid" {
            return self.call_uuid(name, &args, line, col);
        }
        if module == "__time" {
            return self.call_time(name, &args, line, col);
        }
        if module == "__path" {
            return self.call_path(name, &args, line, col);
        }
        if module == "__json" {
            return self.call_json(name, &args, line, col);
        }
        if module == "__regex" {
            return self.call_regex(name, &args, line, col);
        }
        if module == "__ui" {
            return self.call_ui(name, &args, line, col);
        }
        if module == "hooks" || module == "events" {
            return self.call_game_hook(module, name, args, line, col);
        }
        if module == "block"
            || module == "recipe"
            || module == "loot"
            || module == "entity"
            || (module == "item" && (name == "register" || name == "places" || name == "stack"))
        {
            return self.call_registry(module, name, args, line, col);
        }
        if module == "block_entity" && name == "bind" {
            return self.bind_block_entity(args, line, col);
        }
        if module == "modapi" {
            return self.call_modapi(name, args, line, col);
        }
        if module == "gui" && name == "open" {
            return self.call_host(module, name, args, line, col);
        }
        if module == "gui" {
            return self.call_gui(name, args, line, col);
        }
        if module == "settings" {
            return self.call_settings(name, args, line, col);
        }
        if module == "world"
            || module == "player"
            || module == "item"
            || module == "chat"
            || module == "block_entity"
        {
            return self.call_host(module, name, args, line, col);
        }
        if module == "Future" {
            return self.call_future(name, args, line, col);
        }
        let msg = if module.is_empty() {
            format!("unknown function '{name}'")
        } else {
            format!("unknown function {module}.{name}")
        };
        Err(self.runtime(&msg, line, col))
    }
}
impl Interp {
    fn call_future(
        &mut self,
        name: &str,
        args: Vec<Value>,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        if name != "all" && name != "race" {
            return Err(self.runtime(&format!("unknown function Future.{name}"), line, col));
        }
        if args.len() != 1 {
            return Err(self.runtime(&format!("Future.{name} takes 1 argument"), line, col));
        }
        let Value::Array(items) = &args[0] else {
            return Err(self.runtime(&format!("Future.{name} expects Array of Future"), line, col));
        };
        let items = items.borrow().clone();
        for item in &items {
            if !matches!(item, Value::Future(_)) {
                return Err(self.runtime(
                    &format!("Future.{name} expects Array of Future"),
                    line,
                    col,
                ));
            }
        }
        let out = Rc::new(RefCell::new(FutureData::pending()));
        if items.is_empty() {
            if name == "all" {
                self.settle(&out, Value::array(Vec::new()));
            } else {
                self.fail_future(
                    &out,
                    Value::String("Future.race on empty Array".into()),
                    line,
                    col,
                );
            }
            return Ok(Value::Future(out));
        }
        let mut futs = Vec::new();
        for item in &items {
            if let Value::Future(fut) = item {
                futs.push(fut.clone());
            }
        }
        if name == "all" {
            let state = Rc::new(AllState {
                out: out.clone(),
                results: RefCell::new(vec![Value::Void; futs.len()]),
                remaining: std::cell::Cell::new(futs.len()),
                done: std::cell::Cell::new(false),
            });
            for (index, fut) in futs.iter().cloned().enumerate() {
                let state = state.clone();
                let watched = fut.clone();
                self.watch(
                    &fut,
                    Box::new(move |ip| {
                        if state.done.get() {
                            return Ok(());
                        }
                        let (st, err, el, ec, result) = {
                            let f = watched.borrow();
                            (
                                f.state,
                                f.error.clone(),
                                f.err_line,
                                f.err_col,
                                f.result.clone(),
                            )
                        };
                        if st == FutState::Failed {
                            state.done.set(true);
                            ip.fail_future(&state.out, err, el, ec);
                            return Ok(());
                        }
                        if st != FutState::Ready {
                            return Ok(());
                        }
                        state.results.borrow_mut()[index] = result;
                        let left = state.remaining.get() - 1;
                        state.remaining.set(left);
                        if left == 0 {
                            state.done.set(true);
                            let arr = Value::array(state.results.borrow().clone());
                            ip.settle(&state.out, arr);
                        }
                        Ok(())
                    }),
                );
            }
        } else {
            let state = Rc::new(RaceState {
                out: out.clone(),
                done: std::cell::Cell::new(false),
            });
            for fut in &futs {
                let state = state.clone();
                let watched = fut.clone();
                self.watch(
                    fut,
                    Box::new(move |ip| {
                        if state.done.get() {
                            return Ok(());
                        }
                        let (st, err, el, ec, result) = {
                            let f = watched.borrow();
                            (
                                f.state,
                                f.error.clone(),
                                f.err_line,
                                f.err_col,
                                f.result.clone(),
                            )
                        };
                        if st == FutState::Failed {
                            state.done.set(true);
                            ip.fail_future(&state.out, err, el, ec);
                            return Ok(());
                        }
                        if st != FutState::Ready {
                            return Ok(());
                        }
                        state.done.set(true);
                        ip.settle(&state.out, result);
                        Ok(())
                    }),
                );
            }
        }
        Ok(Value::Future(out))
    }

    fn settle(&mut self, fut: &Rc<RefCell<FutureData>>, value: Value) {
        {
            let mut f = fut.borrow_mut();
            if f.state != FutState::Pending {
                return;
            }
            f.state = FutState::Ready;
            f.result = value;
        }
        if let Some(waiters) = self.future_waiters.remove(&fut_key(fut)) {
            for waiter in waiters {
                self.microtasks.push_back(waiter);
            }
        }
    }

    fn fail_future(&mut self, fut: &Rc<RefCell<FutureData>>, error: Value, line: u32, col: u32) {
        {
            let mut f = fut.borrow_mut();
            if f.state != FutState::Pending {
                return;
            }
            f.state = FutState::Failed;
            f.error = error;
            f.err_line = line;
            f.err_col = col;
        }
        if let Some(waiters) = self.future_waiters.remove(&fut_key(fut)) {
            for waiter in waiters {
                self.microtasks.push_back(waiter);
            }
        }
    }

    fn watch(&mut self, fut: &Rc<RefCell<FutureData>>, task: Task) {
        if fut.borrow().state != FutState::Pending {
            self.microtasks.push_back(task);
            return;
        }
        self.future_waiters
            .entry(fut_key(fut))
            .or_default()
            .push(task);
    }

    fn cancel_future(&mut self, fut: &Rc<RefCell<FutureData>>, line: u32, col: u32) {
        if fut.borrow().state != FutState::Pending {
            return;
        }
        self.timers.retain(|job| !Rc::ptr_eq(&job.fut, fut));
        self.frame_jobs.retain(|job| !Rc::ptr_eq(&job.fut, fut));
        self.fail_future(fut, Value::String("cancelled".into()), line, col);
    }

    fn pump_once(&mut self, may_wait: bool) -> Result<bool, Halt> {
        self.flush_deferred()?;
        let mut progress = false;
        while let Some(task) = self.microtasks.pop_front() {
            progress = true;
            task(self)?;
            self.flush_deferred()?;
        }
        let now = self.now_ms();
        let mut due = Vec::new();
        self.timers.retain(|job| {
            if job.deadline_ms <= now {
                due.push(job.fut.clone());
                false
            } else {
                true
            }
        });
        for fut in due {
            progress = true;
            self.settle(&fut, Value::Void);
        }
        if self.pump_frame_jobs() {
            progress = true;
        }
        if progress {
            return Ok(true);
        }
        if !may_wait || self.timers.is_empty() {
            return Ok(false);
        }
        let mut next = self.timers[0].deadline_ms;
        for job in &self.timers {
            if job.deadline_ms < next {
                next = job.deadline_ms;
            }
        }
        while next > self.now_ms() {
            let wait = (next - self.now_ms()) as u64;
            std::thread::sleep(Duration::from_millis(wait.max(1)));
        }
        let now = self.now_ms();
        let mut due = Vec::new();
        self.timers.retain(|job| {
            if job.deadline_ms <= now {
                due.push(job.fut.clone());
                false
            } else {
                true
            }
        });
        for fut in due {
            self.settle(&fut, Value::Void);
            progress = true;
        }
        Ok(progress || !self.microtasks.is_empty())
    }

    fn pump_frame_jobs(&mut self) -> bool {
        if self.frame_jobs.is_empty() {
            return false;
        }
        let jobs = std::mem::take(&mut self.frame_jobs);
        for job in jobs {
            let alive = self.ui.is_alive(job.win_id);
            self.settle(&job.fut, Value::Bool(alive));
        }
        true
    }

    fn drain_event_loop(&mut self) -> Result<(), Halt> {
        for _ in 0..MAX_EVENT_LOOP {
            if !self.pump_once(false)? {
                if self.microtasks.is_empty()
                    && self.timers.is_empty()
                    && self.frame_jobs.is_empty()
                {
                    return Ok(());
                }
                if !self.pump_once(true)? {
                    return Ok(());
                }
            }
        }
        Err(self.runtime("event loop exceeded iteration limit", 1, 1))
    }

    fn await_future(
        &mut self,
        fut: &Rc<RefCell<FutureData>>,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        let mut spins = 0i32;
        loop {
            if fut.borrow().state != FutState::Pending {
                break;
            }
            if spins >= MAX_EVENT_LOOP {
                return Err(self.runtime("event loop exceeded iteration limit", line, col));
            }
            spins += 1;
            if !self.pump_once(true)? {
                return Err(self.runtime("await deadlock: Future never settles", line, col));
            }
        }
        let (state, result, error, el, ec) = {
            let f = fut.borrow();
            (
                f.state,
                f.result.clone(),
                f.error.clone(),
                f.err_line,
                f.err_col,
            )
        };
        if state == FutState::Failed {
            return Err(Halt::Throw {
                value: error,
                line: el,
                col: ec,
            });
        }
        Ok(result)
    }

    fn call_settings(
        &mut self,
        name: &str,
        args: Vec<Value>,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        if name == "value" {
            return self.call_host("settings", name, args, line, col);
        }
        if name == "watch" {
            if args.len() != 2 {
                return Err(self.runtime("settings.watch takes 2 arguments", line, col));
            }
            let Some(Value::String(option_name)) = args.first() else {
                return Err(self.runtime("settings.watch expects String, fn", line, col));
            };
            if !matches!(args.get(1), Some(Value::FnRef { .. })) {
                return Err(self.runtime("settings.watch expects a function", line, col));
            }
            if !self.settings.iter().any(|option| option.name == *option_name) {
                return Err(self.runtime(&format!("unknown setting {option_name}"), line, col));
            }
            if self
                .game_hooks
                .iter()
                .any(|hook| hook.name == "setting_change" && hook.id == *option_name)
            {
                return Err(self.runtime(
                    &format!("duplicate watch on {option_name}"),
                    line,
                    col,
                ));
            }
            self.game_hooks.push(GameHook {
                name: "setting_change".to_string(),
                id: option_name.clone(),
                func: args[1].clone(),
            });
            return Ok(Value::Void);
        }
        if name == "choices" {
            if args.len() != 2 {
                return Err(self.runtime("settings.choices takes 2 arguments", line, col));
            }
            let Some(Value::String(option_name)) = args.first() else {
                return Err(self.runtime("settings.choices expects String, String", line, col));
            };
            let Some(Value::String(list)) = args.get(1) else {
                return Err(self.runtime("settings.choices expects String, String", line, col));
            };
            let Some(option) = self
                .settings
                .iter_mut()
                .find(|option| option.name == *option_name)
            else {
                return Err(self.runtime(&format!("unknown setting {option_name}"), line, col));
            };
            if !option.choices.is_empty() {
                return Err(self.runtime(
                    &format!("duplicate choices on {option_name}"),
                    line,
                    col,
                ));
            }
            let words: Vec<String> = list.split(' ').map(str::to_string).collect();
            if !(2..=4).contains(&words.len())
                || words.iter().any(|word| !choice_word(word))
                || words.iter().enumerate().any(|(index, word)| {
                    words[..index].iter().any(|earlier| earlier == word)
                })
            {
                return Err(self.runtime(&format!("bad choices {option_name}"), line, col));
            }
            if !words.iter().any(|word| word == &option.value) {
                return Err(self.runtime(
                    &format!("{option_name} is not in the choice list"),
                    line,
                    col,
                ));
            }
            option.choices = words;
            return Ok(Value::Void);
        }
        if name != "option" {
            return Err(self.runtime(&format!("unknown function settings.{name}"), line, col));
        }
        if args.len() != 5 {
            return Err(self.runtime("settings.option takes 5 arguments", line, col));
        }
        let Some(Value::String(category)) = args.first() else {
            return Err(self.runtime(
                "settings.option expects String, String, String, String, String",
                line,
                col,
            ));
        };
        let Some(Value::String(category_text)) = args.get(1) else {
            return Err(self.runtime(
                "settings.option expects String, String, String, String, String",
                line,
                col,
            ));
        };
        let Some(Value::String(option_name)) = args.get(2) else {
            return Err(self.runtime(
                "settings.option expects String, String, String, String, String",
                line,
                col,
            ));
        };
        let Some(Value::String(description)) = args.get(3) else {
            return Err(self.runtime(
                "settings.option expects String, String, String, String, String",
                line,
                col,
            ));
        };
        let Some(Value::String(value)) = args.get(4) else {
            return Err(self.runtime(
                "settings.option expects String, String, String, String, String",
                line,
                col,
            ));
        };
        if !mod_name(category) || category.len() > 16 || category == option_name {
            return Err(self.runtime(&format!("bad setting category {category}"), line, col));
        }
        if reserved_setting(category) {
            return Err(self.runtime(&format!("reserved category {category}"), line, col));
        }
        if !setting_words(category_text, 48) {
            return Err(self.runtime(&format!("bad setting category {category}"), line, col));
        }
        if !mod_name(option_name) || option_name.len() > 16 {
            return Err(self.runtime(&format!("bad setting {option_name}"), line, col));
        }
        if reserved_setting(option_name) {
            return Err(self.runtime(
                &format!("reserved setting {option_name}"),
                line,
                col,
            ));
        }
        if !setting_words(description, 48) || !setting_words(value, 16) {
            return Err(self.runtime(&format!("bad setting {option_name}"), line, col));
        }
        if self.settings.len() >= 4 {
            return Err(self.runtime("too many settings", line, col));
        }
        if self.settings.iter().any(|option| option.name == *option_name) {
            return Err(self.runtime(
                &format!("duplicate setting {option_name}"),
                line,
                col,
            ));
        }
        self.settings.push(SettingOption {
            category: category.clone(),
            category_text: category_text.clone(),
            name: option_name.clone(),
            description: description.clone(),
            value: value.clone(),
            choices: Vec::new(),
        });
        Ok(Value::Bool(true))
    }

    fn call_gui(
        &mut self,
        name: &str,
        args: Vec<Value>,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        let text_at = |args: &[Value], index: usize| match args.get(index) {
            Some(Value::String(text)) => Some(text.clone()),
            _ => None,
        };
        let number_at = |args: &[Value], index: usize| {
            let value = args.get(index)?;
            if !value.is_numeric() {
                return None;
            }
            let number = value.as_f64();
            if number.is_finite() {
                Some(number)
            } else {
                None
            }
        };
        match name {
            "screen" => {
                if args.len() != 5 {
                    return Err(self.runtime("gui.screen takes 5 arguments", line, col));
                }
                let Some(screen_name) = text_at(&args, 0) else {
                    return Err(self.runtime(
                        "gui.screen expects String, Float, Float, Float, Float",
                        line,
                        col,
                    ));
                };
                let (Some(w), Some(h), Some(x), Some(y)) = (
                    number_at(&args, 1),
                    number_at(&args, 2),
                    number_at(&args, 3),
                    number_at(&args, 4),
                ) else {
                    return Err(self.runtime(
                        "gui.screen expects String, Float, Float, Float, Float",
                        line,
                        col,
                    ));
                };
                if !mod_name(&screen_name) {
                    return Err(self.runtime(&format!("bad screen id {screen_name}"), line, col));
                }
                if !(w > 0.0 && h > 0.0 && w <= 4096.0 && h <= 4096.0) {
                    return Err(self.runtime("screen size is out of range", line, col));
                }
                if !placed(x) || !placed(y) {
                    return Err(self.runtime("screen position is out of range", line, col));
                }
                if self.gui_screens.iter().any(|screen| screen.name == screen_name) {
                    return Err(self.runtime(&format!("duplicate screen {screen_name}"), line, col));
                }
                self.gui_screens.push(GuiScreen {
                    name: screen_name,
                    w: w as f32,
                    h: h as f32,
                    x: x as f32,
                    y: y as f32,
                    key: String::new(),
                    elements: Vec::new(),
                });
                Ok(Value::Bool(true))
            }
            "element" => {
                if args.len() != 11 {
                    return Err(self.runtime("gui.element takes 11 arguments", line, col));
                }
                let (Some(screen_name), Some(row_name), Some(text), Some(font)) = (
                    text_at(&args, 0),
                    text_at(&args, 1),
                    text_at(&args, 2),
                    text_at(&args, 8),
                ) else {
                    return Err(self.runtime("gui.element expects the screen row as strings and numbers", line, col));
                };
                let (Some(x), Some(y), Some(w), Some(h)) = (
                    number_at(&args, 3),
                    number_at(&args, 4),
                    number_at(&args, 5),
                    number_at(&args, 6),
                ) else {
                    return Err(self.runtime("gui.element expects the screen row as strings and numbers", line, col));
                };
                let (Some(color), Some(font_size), Some(font_color)) = (
                    whole_at(&args, 7),
                    whole_at(&args, 9),
                    whole_at(&args, 10),
                ) else {
                    return Err(self.runtime("gui.element expects the screen row as strings and numbers", line, col));
                };
                if !mod_name(&row_name) || text.len() > 32 || font.len() > 32 {
                    return Err(self.runtime(&format!("bad element {row_name}"), line, col));
                }
                if text.is_empty() && font != "field" {
                    return Err(self.runtime(&format!("bad element {row_name}"), line, col));
                }
                if !known_font(&font) {
                    return Err(self.runtime(&format!("unknown font {font}"), line, col));
                }
                if !(w > 0.0 && h > 0.0 && w <= 4096.0 && h <= 4096.0) || !placed(x) || !placed(y) {
                    return Err(self.runtime("element box is out of range", line, col));
                }
                let Some(screen) = self
                    .gui_screens
                    .iter_mut()
                    .find(|screen| screen.name == screen_name)
                else {
                    return Err(self.runtime(&format!("unknown screen {screen_name}"), line, col));
                };
                if screen.elements.iter().any(|row| row.name == row_name) {
                    return Err(self.runtime(&format!("duplicate element {row_name}"), line, col));
                }
                if screen.elements.len() >= 12 {
                    return Err(self.runtime("a screen holds at most 12 elements", line, col));
                }
                screen.elements.push(GuiElement {
                    name: row_name,
                    text,
                    x: x as f32,
                    y: y as f32,
                    w: w as f32,
                    h: h as f32,
                    color,
                    font,
                    font_size,
                    font_color,
                });
                Ok(Value::Bool(true))
            }
            "remove" => {
                if args.len() != 2 {
                    return Err(self.runtime("gui.remove takes 2 arguments", line, col));
                }
                let (Some(screen_name), Some(row_name)) = (text_at(&args, 0), text_at(&args, 1))
                else {
                    return Err(self.runtime("gui.remove expects String, String", line, col));
                };
                let Some(screen) = self
                    .gui_screens
                    .iter_mut()
                    .find(|screen| screen.name == screen_name)
                else {
                    return Ok(Value::Bool(false));
                };
                let before = screen.elements.len();
                screen.elements.retain(|row| row.name != row_name);
                Ok(Value::Bool(screen.elements.len() != before))
            }
            "read" => {
                if args.len() != 2 {
                    return Err(self.runtime("gui.read takes 2 arguments", line, col));
                }
                let (Some(screen_name), Some(row_name)) = (text_at(&args, 0), text_at(&args, 1))
                else {
                    return Err(self.runtime("gui.read expects String, String", line, col));
                };
                let text = self
                    .gui_screens
                    .iter()
                    .find(|screen| screen.name == screen_name)
                    .and_then(|screen| screen.elements.iter().find(|row| row.name == row_name))
                    .map(|row| row.text.clone())
                    .unwrap_or_default();
                Ok(Value::String(text))
            }
            "press" => {
                if args.len() != 3 {
                    return Err(self.runtime("gui.press takes 3 arguments", line, col));
                }
                let (Some(screen_name), Some(key)) = (text_at(&args, 0), text_at(&args, 1)) else {
                    return Err(self.runtime("gui.press expects String, String, fn", line, col));
                };
                if !matches!(args.get(2), Some(Value::FnRef { .. })) {
                    return Err(self.runtime("gui.press expects a function", line, col));
                }
                if key.len() != 1 || !key.chars().next().is_some_and(|ch| ch.is_ascii_lowercase())
                {
                    return Err(self.runtime("screen key must be one lowercase letter", line, col));
                }
                if key == "e" {
                    return Err(self.runtime("e opens the items list", line, col));
                }
                if key == "i" {
                    return Err(self.runtime("i opens the inventory", line, col));
                }
                if self
                    .gui_screens
                    .iter()
                    .any(|screen| screen.name != screen_name && screen.key == key)
                {
                    return Err(self.runtime(
                        &format!("{key} already opens a screen"),
                        line,
                        col,
                    ));
                }
                if !self.gui_screens.iter().any(|screen| screen.name == screen_name) {
                    return Err(self.runtime(&format!("unknown screen {screen_name}"), line, col));
                }
                if self
                    .game_hooks
                    .iter()
                    .any(|hook| hook.name == "screen_press" && hook.id == screen_name)
                {
                    return Err(self.runtime(
                        &format!("duplicate press on {screen_name}"),
                        line,
                        col,
                    ));
                }
                if let Some(screen) = self
                    .gui_screens
                    .iter_mut()
                    .find(|screen| screen.name == screen_name)
                {
                    screen.key = key;
                }
                self.game_hooks.push(GameHook {
                    name: "screen_press".to_string(),
                    id: screen_name,
                    func: args[2].clone(),
                });
                Ok(Value::Void)
            }
            _ => Err(self.runtime(&format!("unknown function gui.{name}"), line, col)),
        }
    }

    fn call_modapi(
        &mut self,
        name: &str,
        args: Vec<Value>,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        let string_at = |args: &[Value], index: usize| match args.get(index) {
            Some(Value::String(text)) => Some(text.clone()),
            _ => None,
        };
        match name {
            "note" | "registry" => {
                if args.len() != 1 {
                    return Err(self.runtime(
                        &format!("modapi.{name} takes 1 argument"),
                        line,
                        col,
                    ));
                }
                let Some(text) = string_at(&args, 0) else {
                    return Err(self.runtime(
                        &format!("modapi.{name} expects a string"),
                        line,
                        col,
                    ));
                };
                self.attachments
                    .push((name.to_string(), text, String::new()));
                Ok(Value::Void)
            }
            "content" => {
                if args.len() != 3 {
                    return Err(self.runtime("modapi.content takes 3 arguments", line, col));
                }
                let (Some(kind), Some(namespace), Some(path)) = (
                    string_at(&args, 0),
                    string_at(&args, 1),
                    string_at(&args, 2),
                ) else {
                    return Err(self.runtime(
                        "modapi.content expects String, String, String",
                        line,
                        col,
                    ));
                };
                if !registry_kind(&kind) || !mod_name(&namespace) || !mod_name(&path) {
                    return Err(self.runtime(
                        &format!("bad identifier {kind} {namespace}:{path}"),
                        line,
                        col,
                    ));
                }
                let id = format!("{namespace}:{path}");
                self.attachments
                    .push(("entry".to_string(), kind, id.clone()));
                Ok(Value::String(id))
            }
            _ => Err(self.runtime(&format!("unknown function modapi.{name}"), line, col)),
        }
    }

    fn call_registry(
        &mut self,
        module: &str,
        name: &str,
        args: Vec<Value>,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        if module == "entity" && (name == "spawn" || name == "despawn") {
            return self.entity_body(name, args, line, col);
        }
        if module == "entity" && name == "hostile" {
            return self.attach_flag(module, name, &args, line, col);
        }
        if module == "entity" && (name == "hearts" || name == "hurtbox" || name == "speed") {
            return self.attach_number(module, name, &args, line, col);
        }
        if module == "block" && name == "hardness" {
            return self.attach_number(module, name, &args, line, col);
        }
        if module == "block" && name == "light" {
            return self.attach_number(module, name, &args, line, col);
        }
        if module == "item" && name == "stack" {
            return self.attach_number(module, name, &args, line, col);
        }
        if args.len() != 2 {
            return Err(self.runtime(&format!("{module}.{name} takes 2 arguments"), line, col));
        }
        let Value::String(left) = &args[0] else {
            return Err(self.runtime(
                &format!("{module}.{name} expects String, String"),
                line,
                col,
            ));
        };
        let Value::String(right) = &args[1] else {
            return Err(self.runtime(
                &format!("{module}.{name} expects String, String"),
                line,
                col,
            ));
        };
        if name == "register" && module != "recipe" {
            self.registrations
                .push((module.to_string(), left.clone(), right.clone()));
            return Ok(Value::String(format!("{left}:{right}")));
        }
        if (module == "block" && (name == "texture" || name == "model"))
            || (module == "item" && name == "places")
            || (module == "entity" && (name == "model" || name == "spawn_egg"))
            || (module == "recipe"
                && (name == "register"
                    || name == "shapeless"
                    || name == "shaped"
                    || name == "smelt"
                    || name == "smoke"
                    || name == "blast"))
            || (module == "loot" && (name == "block" || name == "chest" || name == "entity"))
        {
            let kind = if module == "recipe" && name == "register" {
                "recipe"
            } else if module == "entity" && name == "model" {
                "entity_model"
            } else if module == "entity" && name == "spawn_egg" {
                "spawn_egg"
            } else if module == "loot" && name == "block" {
                "block_loot"
            } else if module == "loot" && name == "chest" {
                "chest_loot"
            } else if module == "loot" && name == "entity" {
                "entity_loot"
            } else {
                name
            };
            self.attachments
                .push((kind.to_string(), left.clone(), right.clone()));
            return Ok(Value::Bool(true));
        }
        Err(self.runtime(&format!("unknown function {module}.{name}"), line, col))
    }

    /// `block_entity.bind(id)` marks a block id as a script tile during `main`.
    fn bind_block_entity(
        &mut self,
        args: Vec<Value>,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        if args.len() != 1 {
            return Err(self.runtime("block_entity.bind takes 1 argument", line, col));
        }
        let Value::String(id) = &args[0] else {
            return Err(self.runtime("block_entity.bind expects String", line, col));
        };
        if id.is_empty() || !id.contains(':') {
            return Err(self.runtime(&format!("bad block entity {id}"), line, col));
        }
        self.attachments
            .push(("block_entity".to_string(), id.clone(), String::new()));
        Ok(Value::Bool(true))
    }

    /// `entity.hostile(id, flag)` stores `"1"` or `"0"`.
    fn attach_flag(
        &mut self,
        module: &str,
        name: &str,
        args: &[Value],
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        if args.len() != 2 {
            return Err(self.runtime(&format!("{module}.{name} takes 2 arguments"), line, col));
        }
        let Value::String(id) = &args[0] else {
            return Err(self.runtime(
                &format!("{module}.{name} expects String, Bool"),
                line,
                col,
            ));
        };
        let Value::Bool(flag) = &args[1] else {
            return Err(self.runtime(
                &format!("{module}.{name} expects String, Bool"),
                line,
                col,
            ));
        };
        self.attachments.push((
            name.to_string(),
            id.clone(),
            if *flag { "1".into() } else { "0".into() },
        ));
        Ok(Value::Bool(true))
    }

    /// `block.hardness`, `block.light`, and `item.stack` store a number as text.
    fn attach_number(
        &mut self,
        module: &str,
        name: &str,
        args: &[Value],
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        if args.len() != 2 {
            return Err(self.runtime(&format!("{module}.{name} takes 2 arguments"), line, col));
        }
        let Value::String(id) = &args[0] else {
            return Err(self.runtime(
                &format!("{module}.{name} expects String, number"),
                line,
                col,
            ));
        };
        if !args[1].is_numeric() {
            return Err(self.runtime(
                &format!("{module}.{name} expects String, number"),
                line,
                col,
            ));
        }
        let value = args[1].as_f64();
        if !value.is_finite() {
            return Err(self.runtime(
                &format!("{module}.{name} expects a finite number"),
                line,
                col,
            ));
        }
        let encoded = if name == "hardness" || name == "hurtbox" || name == "speed" {
            if name == "hardness" && value <= 0.0 {
                return Err(self.runtime(
                    "block.hardness expects a positive number of seconds",
                    line,
                    col,
                ));
            }
            if name == "hurtbox" && !(0.2..=4.0).contains(&value) {
                return Err(self.runtime(
                    "entity.hurtbox expects a radius from 0.2 to 4.0",
                    line,
                    col,
                ));
            }
            if name == "speed" && !(0.25..=4.0).contains(&value) {
                return Err(self.runtime(
                    "entity.speed expects a scale from 0.25 to 4.0",
                    line,
                    col,
                ));
            }
            format!("{value}")
        } else {
            let n = value.round() as i64;
            if name == "light" && !(1..=15).contains(&n) {
                return Err(self.runtime(
                    "block.light expects a level from 1 to 15",
                    line,
                    col,
                ));
            }
            if name == "stack" && !(1..=64).contains(&n) {
                return Err(self.runtime(
                    "item.stack expects a count from 1 to 64",
                    line,
                    col,
                ));
            }
            if name == "hearts" && !(1..=40).contains(&n) {
                return Err(self.runtime(
                    "entity.hearts expects a count from 1 to 40",
                    line,
                    col,
                ));
            }
            n.to_string()
        };
        self.attachments
            .push((name.to_string(), id.clone(), encoded));
        Ok(Value::Bool(true))
    }

    /// `entity.spawn` and `entity.despawn` are `fn(id, x, y, z): Bool`.
    /// False means this script never registered `id`, the cell is already taken,
    /// or no spawned body stood there. Inside a hook the host places or removes
    /// the live body; during `main` / `on_init` the session records it for load.
    fn entity_body(
        &mut self,
        name: &str,
        args: Vec<Value>,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        if args.len() != 4 {
            return Err(self.runtime(&format!("entity.{name} takes 4 arguments"), line, col));
        }
        let Value::String(id) = &args[0] else {
            return Err(self.runtime(
                &format!("entity.{name} expects String, Int, Int, Int"),
                line,
                col,
            ));
        };
        let Some(x) = int_cell(&args[1]) else {
            return Err(self.runtime(
                &format!("entity.{name} expects String, Int, Int, Int"),
                line,
                col,
            ));
        };
        let Some(y) = int_cell(&args[2]) else {
            return Err(self.runtime(
                &format!("entity.{name} expects String, Int, Int, Int"),
                line,
                col,
            ));
        };
        let Some(z) = int_cell(&args[3]) else {
            return Err(self.runtime(
                &format!("entity.{name} expects String, Int, Int, Int"),
                line,
                col,
            ));
        };
        if self.host.is_some() {
            return self.call_host("entity", name, args, line, col);
        }
        let known = self.registrations.iter().any(|(kind, namespace, path)| {
            kind == "entity" && format!("{namespace}:{path}") == *id
        });
        if !known {
            return Ok(Value::Bool(false));
        }
        let at = self
            .spawns
            .iter()
            .position(|spawn| spawn.0 == *id && spawn.1 == x && spawn.2 == y && spawn.3 == z);
        if name == "despawn" {
            if let Some(index) = at {
                self.spawns.remove(index);
                return Ok(Value::Bool(true));
            }
            return Ok(Value::Bool(false));
        }
        if at.is_some() {
            return Ok(Value::Bool(false));
        }
        self.spawns.push((id.clone(), x, y, z));
        Ok(Value::Bool(true))
    }

    fn call_game_hook(
        &mut self,
        module: &str,
        name: &str,
        args: Vec<Value>,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        let Some(stored) = canonical_hook(module, name) else {
            return Err(self.runtime(&format!("unknown function {module}.{name}"), line, col));
        };
        if args.len() != 2 {
            return Err(self.runtime(&format!("{module}.{name} takes 2 arguments"), line, col));
        }
        let Value::String(id) = &args[0] else {
            return Err(self.runtime(&format!("{module}.{name} expects String, fn"), line, col));
        };
        if id.is_empty() {
            return Err(self.runtime("hook id is empty", line, col));
        }
        if !matches!(args[1], Value::FnRef { .. }) {
            return Err(self.runtime(&format!("{module}.{name} expects a function"), line, col));
        }
        self.game_hooks.push(GameHook {
            name: stored.to_string(),
            id: id.clone(),
            func: args[1].clone(),
        });
        Ok(Value::Void)
    }

    fn call_host(
        &mut self,
        module: &str,
        name: &str,
        args: Vec<Value>,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        let Some(expected) = host_arity(module, name) else {
            return Err(self.runtime(&format!("unknown function {module}.{name}"), line, col));
        };
        if args.len() != expected {
            return Err(self.runtime(
                &format!("{module}.{name} takes {expected} arguments"),
                line,
                col,
            ));
        }
        let Some(host) = self.host else {
            return Err(self.runtime(
                &format!("{module}.{name} is only available inside a hook"),
                line,
                col,
            ));
        };
        let qualified = format!("{module}.{name}");
        // The host callback lives on the caller's stack for this hook call.
        let result = unsafe { (*host)(&qualified, &args) };
        match result {
            Ok(value) => Ok(value),
            Err(err) => Err(self.runtime(&err, line, col)),
        }
    }

    fn call_checks(&self, name: &str, args: &[Value], line: u32, col: u32) -> Result<Value, Halt> {
        if name == "eq" || name == "neq" {
            if args.len() != 2 {
                return Err(self.runtime(&format!("checks.{name} takes 2 argument(s)"), line, col));
            }
            if !args[0].is_numeric() || !args[1].is_numeric() {
                return Err(self.runtime(&format!("checks.{name} expects numbers"), line, col));
            }
            let same = args[0].equals_value(&args[1]);
            if if name == "eq" { !same } else { same } {
                return Err(self.runtime("assertion failed", line, col));
            }
            return Ok(Value::Void);
        }
        if name == "eq_string" {
            if args.len() != 2 {
                return Err(self.runtime("checks.eq_string takes 2 argument(s)", line, col));
            }
            match (&args[0], &args[1]) {
                (Value::String(a), Value::String(b)) if a == b => return Ok(Value::Void),
                (Value::String(_), Value::String(_)) => {
                    return Err(self.runtime("assertion failed", line, col))
                }
                _ => {
                    return Err(self.runtime("checks.eq_string expects String, String", line, col))
                }
            }
        }
        if name == "that" || name == "truthy" {
            if args.len() != 1 {
                return Err(self.runtime(&format!("checks.{name} takes 1 argument(s)"), line, col));
            }
            if !args[0].truthy() {
                return Err(self.runtime("assertion failed", line, col));
            }
            return Ok(Value::Void);
        }
        Err(self.runtime(&format!("unknown function checks.{name}"), line, col))
    }

    fn call_math(
        &mut self,
        name: &str,
        args: &[Value],
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        if name == "pow" {
            if args.len() != 2 {
                return Err(self.runtime("__math.pow takes 2 arguments", line, col));
            }
            let (Value::Int(mut base), Value::Int(exp)) = (&args[0], &args[1]) else {
                return Err(self.runtime("__math.pow expects Int, Int", line, col));
            };
            if *exp < 0 {
                return Ok(Value::Int(0));
            }
            let mut r: i64 = 1;
            let mut exp = *exp as u64;
            while exp != 0 {
                if exp & 1 != 0 {
                    r = r.wrapping_mul(base);
                }
                exp >>= 1;
                if exp != 0 {
                    base = base.wrapping_mul(base);
                }
            }
            return Ok(Value::Int(r));
        }
        if name == "rand_int" {
            if args.len() != 1 {
                return Err(self.runtime("__math.rand_int takes 1 argument", line, col));
            }
            let Value::Int(n) = args[0] else {
                return Err(self.runtime("__math.rand_int expects Int", line, col));
            };
            if n <= 0 {
                return Err(self.runtime("__math.rand_int expects n > 0", line, col));
            }
            let n = n.min(i32::MAX as i64) as i32;
            return Ok(Value::Int(self.rng.next_int(n) as i64));
        }
        if name == "random" {
            if !args.is_empty() {
                return Err(self.runtime("__math.random takes 0 arguments", line, col));
            }
            return Ok(Value::Float(self.rng.next_f64()));
        }
        if matches!(
            name,
            "sin" | "cos" | "sqrt" | "floor" | "ceil" | "to_int" | "to_float"
        ) {
            if args.len() != 1 {
                return Err(self.runtime(&format!("__math.{name} takes 1 argument"), line, col));
            }
            if name == "to_float" {
                let Value::Int(n) = args[0] else {
                    return Err(self.runtime("__math.to_float expects Int", line, col));
                };
                return Ok(Value::Float(n as f64));
            }
            if !args[0].is_numeric() {
                return Err(self.runtime(&format!("__math.{name} expects Float"), line, col));
            }
            let n = args[0].as_f64();
            return match name {
                "sin" => Ok(Value::Float(n.sin())),
                "cos" => Ok(Value::Float(n.cos())),
                "sqrt" => {
                    if n < 0.0 {
                        return Err(self.runtime("__math.sqrt expects n >= 0", line, col));
                    }
                    Ok(Value::Float(n.sqrt()))
                }
                "floor" => Ok(Value::Float(n.floor())),
                "ceil" => Ok(Value::Float(n.ceil())),
                _ => Ok(Value::Int(java_trunc_i64(n))),
            };
        }
        if name == "atan2" || name == "powf" {
            if args.len() != 2 {
                return Err(self.runtime(&format!("__math.{name} takes 2 arguments"), line, col));
            }
            if !args[0].is_numeric() || !args[1].is_numeric() {
                return Err(self.runtime(&format!("__math.{name} expects Float"), line, col));
            }
            let a = args[0].as_f64();
            let b = args[1].as_f64();
            if name == "atan2" {
                return Ok(Value::Float(a.atan2(b)));
            }
            return Ok(Value::Float(a.powf(b)));
        }
        Err(self.runtime(&format!("unknown function __math.{name}"), line, col))
    }

    fn call_str(&self, name: &str, args: &[Value], line: u32, col: u32) -> Result<Value, Halt> {
        let need_str = |i: usize| -> Result<String, Halt> {
            match args.get(i) {
                Some(Value::String(s)) => Ok(s.clone()),
                _ => Err(self.runtime(&format!("__str.{name} expects String"), line, col)),
            }
        };
        let need_int = |i: usize| -> Result<i64, Halt> {
            match args.get(i) {
                Some(Value::Int(n)) => Ok(*n),
                _ => Err(self.runtime(&format!("__str.{name} expects Int"), line, col)),
            }
        };
        match name {
            "contains" => {
                if args.len() != 2 {
                    return Err(self.runtime("str.contains takes 2 arguments", line, col));
                }
                Ok(Value::Bool(need_str(0)?.contains(&need_str(1)?)))
            }
            "starts_with" => {
                if args.len() != 2 {
                    return Err(self.runtime("str.starts_with takes 2 arguments", line, col));
                }
                Ok(Value::Bool(need_str(0)?.starts_with(&need_str(1)?)))
            }
            "ends_with" => {
                if args.len() != 2 {
                    return Err(self.runtime("str.ends_with takes 2 arguments", line, col));
                }
                Ok(Value::Bool(need_str(0)?.ends_with(&need_str(1)?)))
            }
            "length" => {
                if args.len() != 1 {
                    return Err(self.runtime("str.length takes 1 argument", line, col));
                }
                Ok(Value::Int(utf16_len(&need_str(0)?)))
            }
            "is_empty" => {
                if args.len() != 1 {
                    return Err(self.runtime("str.is_empty takes 1 argument", line, col));
                }
                Ok(Value::Bool(need_str(0)?.is_empty()))
            }
            "repeat" => {
                if args.len() != 2 {
                    return Err(self.runtime("str.repeat takes 2 arguments", line, col));
                }
                let s = need_str(0)?;
                let mut n = need_int(1)?;
                if n < 0 {
                    n = 0;
                }
                if n > 0 && !s.is_empty() {
                    match utf16_len(&s).checked_mul(n) {
                        Some(total) if total <= MAX_REPEAT_UNITS => {}
                        _ => return Err(self.runtime("str.repeat result too large", line, col)),
                    }
                }
                let count = n.min(i32::MAX as i64) as usize;
                Ok(Value::String(s.repeat(count)))
            }
            "upper" | "lower" => {
                if args.len() != 1 {
                    return Err(self.runtime(&format!("str.{name} takes 1 argument"), line, col));
                }
                let s = need_str(0)?;
                Ok(Value::String(if name == "upper" {
                    java_upper(&s)
                } else {
                    java_lower(&s)
                }))
            }
            "trim" => {
                if args.len() != 1 {
                    return Err(self.runtime("str.trim takes 1 argument", line, col));
                }
                Ok(Value::String(java_trim(&need_str(0)?)))
            }
            "slice" => {
                if args.len() != 3 {
                    return Err(self.runtime("str.slice takes 3 arguments", line, col));
                }
                Ok(Value::String(utf16_slice(
                    &need_str(0)?,
                    need_int(1)?,
                    need_int(2)?,
                )))
            }
            "split" => {
                if args.len() != 2 {
                    return Err(self.runtime("str.split takes 2 arguments", line, col));
                }
                let s = need_str(0)?;
                let sep = need_str(1)?;
                let mut parts = Vec::new();
                if sep.is_empty() {
                    for unit in utf16_units(&s) {
                        parts.push(Value::String(from_units(&[unit])));
                    }
                } else {
                    let mut start = 0usize;
                    let bytes = s.as_bytes();
                    let sep_b = sep.as_bytes();
                    while start <= bytes.len() {
                        if let Some(rel) = s[start..].find(&sep) {
                            let pos = start + rel;
                            parts.push(Value::String(s[start..pos].to_string()));
                            start = pos + sep_b.len();
                        } else {
                            parts.push(Value::String(s[start..].to_string()));
                            break;
                        }
                    }
                }
                Ok(Value::array(parts))
            }
            "replace" => {
                if args.len() != 3 {
                    return Err(self.runtime("str.replace takes 3 arguments", line, col));
                }
                let s = need_str(0)?;
                let from = need_str(1)?;
                let to = need_str(2)?;
                if from.is_empty() {
                    return Ok(Value::String(s));
                }
                let mut out = String::new();
                let mut pos = 0usize;
                while let Some(rel) = s[pos..].find(&from) {
                    let hit = pos + rel;
                    out.push_str(&s[pos..hit]);
                    out.push_str(&to);
                    pos = hit + from.len();
                }
                out.push_str(&s[pos..]);
                Ok(Value::String(out))
            }
            "find" => {
                if args.len() != 2 {
                    return Err(self.runtime("str.find takes 2 arguments", line, col));
                }
                Ok(Value::Int(utf16_find(&need_str(0)?, &need_str(1)?)))
            }
            _ => Err(self.runtime(&format!("unknown function __str.{name}"), line, col)),
        }
    }

    fn call_io(&self, name: &str, args: &[Value], line: u32, col: u32) -> Result<Value, Halt> {
        match name {
            "exists" => {
                if args.len() != 1 {
                    return Err(self.runtime("io.exists takes 1 argument", line, col));
                }
                let path = self.sandbox_path(&need_string(args, 0, "__io.exists")?, line, col)?;
                Ok(Value::Bool(path.exists()))
            }
            "remove" => {
                if args.len() != 1 {
                    return Err(self.runtime("io.remove takes 1 argument", line, col));
                }
                let path = self.sandbox_path(&need_string(args, 0, "__io.remove")?, line, col)?;
                Ok(Value::Bool(remove_path(&path)))
            }
            "read_text" => {
                if args.len() != 1 {
                    return Err(self.runtime("io.read_text takes 1 argument", line, col));
                }
                let path =
                    self.sandbox_path(&need_string(args, 0, "__io.read_text")?, line, col)?;
                match fs::read_to_string(&path) {
                    Ok(text) => Ok(Value::String(text)),
                    Err(_) => Err(self.io_throw(
                        &format!("cannot read '{}'", generic_path(&path)),
                        line,
                        col,
                    )),
                }
            }
            "read_lines" => {
                if args.len() != 1 {
                    return Err(self.runtime("io.read_lines takes 1 argument", line, col));
                }
                let path =
                    self.sandbox_path(&need_string(args, 0, "__io.read_lines")?, line, col)?;
                let text = match fs::read_to_string(&path) {
                    Ok(text) => text,
                    Err(_) => {
                        return Err(self.io_throw(
                            &format!("cannot read '{}'", generic_path(&path)),
                            line,
                            col,
                        ))
                    }
                };
                Ok(Value::array(split_lines(&text)))
            }
            "write_text" => {
                if args.len() != 2 {
                    return Err(self.runtime("io.write_text takes 2 arguments", line, col));
                }
                let Value::String(text) = &args[1] else {
                    return Err(self.runtime("io.write_text expects String", line, col));
                };
                let path =
                    self.sandbox_path(&need_string(args, 0, "__io.write_text")?, line, col)?;
                if let Some(parent) = path.parent() {
                    if !parent.as_os_str().is_empty() && fs::create_dir_all(parent).is_err() {
                        return Err(self.io_throw(
                            &format!("cannot write '{}'", generic_path(&path)),
                            line,
                            col,
                        ));
                    }
                }
                match fs::write(&path, text) {
                    Ok(()) => Ok(Value::Void),
                    Err(_) => Err(self.io_throw(
                        &format!("cannot write '{}'", generic_path(&path)),
                        line,
                        col,
                    )),
                }
            }
            _ => Err(self.runtime(&format!("unknown function __io.{name}"), line, col)),
        }
    }

    fn call_uuid(
        &mut self,
        name: &str,
        args: &[Value],
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        if name == "v4" {
            if !args.is_empty() {
                return Err(self.runtime("__uuid.v4 takes 0 arguments", line, col));
            }
            const DIGITS: &[u8] = b"0123456789abcdef";
            let mut hex = [0u8; 32];
            for (i, slot) in hex.iter_mut().enumerate() {
                let mut n = self.rng.next_int(16) as u8;
                if i == 12 {
                    n = 4;
                } else if i == 16 {
                    n = (n & 0x3) | 0x8;
                }
                *slot = DIGITS[n as usize];
            }
            let hex = String::from_utf8(hex.to_vec()).unwrap_or_default();
            return Ok(Value::String(dashed_uuid(&hex)));
        }
        if name == "parse" || name == "valid" {
            if args.len() != 1 {
                return Err(self.runtime(&format!("__uuid.{name} takes 1 argument"), line, col));
            }
            let Value::String(s) = &args[0] else {
                return Err(self.runtime(&format!("__uuid.{name} expects String"), line, col));
            };
            let Some(hex) = uuid_hex32(s) else {
                if name == "valid" {
                    return Ok(Value::Bool(false));
                }
                return Err(Halt::Throw {
                    value: Value::String("invalid UUID".into()),
                    line,
                    col,
                });
            };
            if name == "valid" {
                return Ok(Value::Bool(true));
            }
            return Ok(Value::String(dashed_uuid(&hex)));
        }
        Err(self.runtime(&format!("unknown function __uuid.{name}"), line, col))
    }

    fn call_time(
        &mut self,
        name: &str,
        args: &[Value],
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        if name == "now" {
            if !args.is_empty() {
                return Err(self.runtime("__time.now takes 0 arguments", line, col));
            }
            let ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            return Ok(Value::Int(ms));
        }
        if name == "sleep" || name == "delay" {
            if args.len() != 1 {
                return Err(self.runtime(&format!("__time.{name} takes 1 argument"), line, col));
            }
            let Value::Int(ms) = args[0] else {
                return Err(self.runtime(&format!("__time.{name} expects Int"), line, col));
            };
            let ms = ms.max(0);
            if ms > MAX_SLEEP_MS {
                return Err(self.runtime(&format!("time.{name} duration too large"), line, col));
            }
            if name == "delay" {
                let fut = Rc::new(RefCell::new(FutureData::pending()));
                self.timers.push(TimerJob {
                    deadline_ms: self.now_ms() + ms,
                    fut: fut.clone(),
                });
                return Ok(Value::Future(fut));
            }
            std::thread::sleep(Duration::from_millis(ms as u64));
            return Ok(Value::Void);
        }
        Err(self.runtime(&format!("unknown function __time.{name}"), line, col))
    }

    fn call_path(&self, name: &str, args: &[Value], line: u32, col: u32) -> Result<Value, Halt> {
        if name == "join" {
            if args.len() != 2 {
                return Err(self.runtime("__path.join takes 2 arguments", line, col));
            }
            let a = need_string(args, 0, "__path.join")?;
            let b = need_string(args, 1, "__path.join")?;
            let joined = Path::new(&a).join(b);
            return Ok(Value::String(generic_path(&joined)));
        }
        if name == "parent" {
            if args.len() != 1 {
                return Err(self.runtime("__path.parent takes 1 argument", line, col));
            }
            let raw = need_string(args, 0, "__path.parent")?;
            let parent = Path::new(&raw).parent();
            let text = match parent {
                Some(p) if !p.as_os_str().is_empty() => generic_path(p),
                _ => String::new(),
            };
            return Ok(Value::String(text));
        }
        if name == "stem" {
            if args.len() != 1 {
                return Err(self.runtime("__path.stem takes 1 argument", line, col));
            }
            let raw = need_string(args, 0, "__path.stem")?;
            let leaf = Path::new(&raw)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let stem = match leaf.rfind('.') {
                Some(dot) if dot > 0 => leaf[..dot].to_string(),
                _ => leaf,
            };
            return Ok(Value::String(stem));
        }
        Err(self.runtime(&format!("unknown function __path.{name}"), line, col))
    }

    fn call_json(&self, name: &str, args: &[Value], line: u32, col: u32) -> Result<Value, Halt> {
        if name == "parse" {
            if args.len() != 1 {
                return Err(self.runtime("__json.parse takes 1 argument", line, col));
            }
            let Value::String(s) = &args[0] else {
                return Err(self.runtime("__json.parse expects String", line, col));
            };
            return match json::parse(s) {
                Ok(v) => Ok(v),
                Err(msg) => Err(Halt::Throw {
                    value: Value::String(msg),
                    line,
                    col,
                }),
            };
        }
        if name == "valid" {
            if args.len() != 1 {
                return Err(self.runtime("__json.valid takes 1 argument", line, col));
            }
            let Value::String(s) = &args[0] else {
                return Err(self.runtime("__json.valid expects String", line, col));
            };
            return Ok(Value::Bool(json::parse(s).is_ok()));
        }
        if name == "stringify" {
            if args.len() != 1 {
                return Err(self.runtime("__json.stringify takes 1 argument", line, col));
            }
            return match json::stringify(&args[0]) {
                Ok(s) => Ok(Value::String(s)),
                Err(err) => Err(self.runtime(&err, line, col)),
            };
        }
        Err(self.runtime(&format!("unknown function __json.{name}"), line, col))
    }

    fn call_regex(&self, name: &str, args: &[Value], line: u32, col: u32) -> Result<Value, Halt> {
        let need = |i: usize| -> Result<String, Halt> {
            match args.get(i) {
                Some(Value::String(s)) => Ok(s.clone()),
                _ => Err(self.runtime(&format!("__regex.{name} expects String"), line, col)),
            }
        };
        match name {
            "valid" => {
                if args.len() != 1 {
                    return Err(self.runtime("regex.valid takes 1 argument", line, col));
                }
                Ok(Value::Bool(Regex::new(&need(0)?).is_ok()))
            }
            "is_match" => {
                if args.len() != 2 {
                    return Err(self.runtime("regex.is_match takes 2 arguments", line, col));
                }
                let re = self.compile_regex(&need(0)?, line, col)?;
                Ok(Value::Bool(re.is_match(&need(1)?)))
            }
            "find" => {
                if args.len() != 2 {
                    return Err(self.runtime("regex.find takes 2 arguments", line, col));
                }
                let text = need(1)?;
                let re = self.compile_regex(&need(0)?, line, col)?;
                match re.find(&text) {
                    Some(m) => Ok(Value::Int(byte_to_utf16(&text, m.start()))),
                    None => Ok(Value::Int(-1)),
                }
            }
            "find_match" => {
                if args.len() != 2 {
                    return Err(self.runtime("regex.find_match takes 2 arguments", line, col));
                }
                let text = need(1)?;
                let re = self.compile_regex(&need(0)?, line, col)?;
                match re.find(&text) {
                    Some(m) => Ok(Value::String(m.as_str().to_string())),
                    None => Ok(Value::String(String::new())),
                }
            }
            "captures" => {
                if args.len() != 2 {
                    return Err(self.runtime("regex.captures takes 2 arguments", line, col));
                }
                let text = need(1)?;
                let re = self.compile_regex(&need(0)?, line, col)?;
                let Some(caps) = re.captures(&text) else {
                    return Ok(Value::array(Vec::new()));
                };
                let mut parts = Vec::new();
                for n in 0..caps.len() {
                    parts.push(Value::String(
                        caps.get(n)
                            .map(|g| g.as_str().to_string())
                            .unwrap_or_default(),
                    ));
                }
                Ok(Value::array(parts))
            }
            "findall" => {
                if args.len() != 2 {
                    return Err(self.runtime("regex.findall takes 2 arguments", line, col));
                }
                let text = need(1)?;
                let re = self.compile_regex(&need(0)?, line, col)?;
                let parts = re
                    .find_iter(&text)
                    .map(|m| Value::String(m.as_str().to_string()))
                    .collect();
                Ok(Value::array(parts))
            }
            "replace" => {
                if args.len() != 3 {
                    return Err(self.runtime("regex.replace takes 3 arguments", line, col));
                }
                let re = self.compile_regex(&need(0)?, line, col)?;
                let text = need(1)?;
                let with = need(2)?;
                Ok(Value::String(
                    re.replace_all(&text, as_regex_repl(&with)).into_owned(),
                ))
            }
            "split" => {
                if args.len() != 2 {
                    return Err(self.runtime("regex.split takes 2 arguments", line, col));
                }
                let text = need(1)?;
                let re = self.compile_regex(&need(0)?, line, col)?;
                let mut parts = Vec::new();
                let mut last = 0usize;
                for m in re.find_iter(&text) {
                    parts.push(Value::String(text[last..m.start()].to_string()));
                    last = m.end();
                }
                parts.push(Value::String(text[last..].to_string()));
                if parts.is_empty() {
                    parts.push(Value::String(text));
                }
                Ok(Value::array(parts))
            }
            _ => Err(self.runtime(&format!("unknown function __regex.{name}"), line, col)),
        }
    }

    fn compile_regex(&self, pattern: &str, line: u32, col: u32) -> Result<Regex, Halt> {
        Regex::new(pattern).map_err(|err| self.runtime(&format!("invalid regex: {err}"), line, col))
    }

    fn sandbox_path(&self, raw: &str, line: u32, col: u32) -> Result<PathBuf, Halt> {
        if raw.is_empty() {
            return Err(self.runtime(&format!("invalid path '{raw}'"), line, col));
        }
        let root = normalize_abs(&self.sandbox_root);
        let req = Path::new(raw);
        let joined = if req.is_absolute() {
            req.to_path_buf()
        } else {
            root.join(req)
        };
        let canon = normalize_abs(&joined);
        if !within(&root, &canon) {
            return Err(self.runtime(&format!("path outside sandbox '{raw}'"), line, col));
        }
        Ok(canon)
    }

    fn io_throw(&self, msg: &str, line: u32, col: u32) -> Halt {
        Halt::Throw {
            value: Value::String(msg.to_string()),
            line,
            col,
        }
    }

    fn apply_unary(&self, op: &str, v: Value, line: u32, col: u32) -> Result<Value, Halt> {
        if op == "-" {
            if let Value::Float(n) = v {
                return Ok(Value::Float(-n));
            }
            let Value::Int(n) = v else {
                return Err(self.runtime("unary '-' expects a number", line, col));
            };
            if n == i64::MIN {
                return Err(self.runtime("integer overflow", line, col));
            }
            return Ok(Value::Int(-n));
        }
        if op == "~" {
            let Value::Int(n) = v else {
                return Err(self.runtime("bitwise not needs Int", line, col));
            };
            return Ok(Value::Int(!n));
        }
        Ok(Value::Bool(!v.truthy()))
    }

    fn apply_binop(
        &self,
        op: &str,
        a: Value,
        b: Value,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        if op == "+" {
            if let (Value::String(sa), Value::String(sb)) = (&a, &b) {
                return Ok(Value::String(format!("{sa}{sb}")));
            }
            if let Value::String(sa) = &a {
                if b.is_numeric() {
                    return Ok(Value::String(format!("{sa}{}", b.to_print())));
                }
            }
            if let Value::String(sb) = &b {
                if a.is_numeric() {
                    return Ok(Value::String(format!("{}{sb}", a.to_print())));
                }
            }
        }
        if op == "==" {
            return Ok(Value::Bool(a.equals_value(&b)));
        }
        if op == "!=" {
            return Ok(Value::Bool(!a.equals_value(&b)));
        }
        if !a.is_numeric() || !b.is_numeric() {
            return Err(self.runtime("operands must be numbers", line, col));
        }
        let both_int = matches!((&a, &b), (Value::Int(_), Value::Int(_)));
        if both_int {
            let (Value::Int(x), Value::Int(y)) = (&a, &b) else {
                unreachable!()
            };
            return match op {
                "+" => x
                    .checked_add(*y)
                    .map(Value::Int)
                    .ok_or_else(|| self.runtime("integer overflow", line, col)),
                "-" => x
                    .checked_sub(*y)
                    .map(Value::Int)
                    .ok_or_else(|| self.runtime("integer overflow", line, col)),
                "*" => x
                    .checked_mul(*y)
                    .map(Value::Int)
                    .ok_or_else(|| self.runtime("integer overflow", line, col)),
                "/" => {
                    if *y == 0 {
                        return Err(self.runtime("division by zero", line, col));
                    }
                    if *x == i64::MIN && *y == -1 {
                        return Err(self.runtime("integer overflow", line, col));
                    }
                    Ok(Value::Int(x / y))
                }
                "%" => {
                    if *y == 0 {
                        return Err(self.runtime("modulo by zero", line, col));
                    }
                    if *x == i64::MIN && *y == -1 {
                        return Err(self.runtime("integer overflow", line, col));
                    }
                    Ok(Value::Int(x % y))
                }
                "&" => Ok(Value::Int(x & y)),
                "|" => Ok(Value::Int(x | y)),
                "^" => Ok(Value::Int(x ^ y)),
                "<<" | ">>" => {
                    if *y < 0 || *y >= 64 {
                        return Err(self.runtime("shift out of range", line, col));
                    }
                    let sh = *y as u32;
                    Ok(Value::Int(if op == "<<" { x << sh } else { x >> sh }))
                }
                "<" => Ok(Value::Bool(x < y)),
                ">" => Ok(Value::Bool(x > y)),
                "<=" => Ok(Value::Bool(x <= y)),
                ">=" => Ok(Value::Bool(x >= y)),
                _ => Err(self.runtime("unknown operator", line, col)),
            };
        }
        let x = a.as_f64();
        let y = b.as_f64();
        match op {
            "+" => Ok(Value::Float(x + y)),
            "-" => Ok(Value::Float(x - y)),
            "*" => Ok(Value::Float(x * y)),
            "/" => {
                if y == 0.0 {
                    return Err(self.runtime("division by zero", line, col));
                }
                Ok(Value::Float(x / y))
            }
            "%" => {
                if y == 0.0 {
                    return Err(self.runtime("modulo by zero", line, col));
                }
                Ok(Value::Float(x % y))
            }
            "&" | "|" | "^" | "<<" | ">>" => {
                Err(self.runtime("bitwise operators need Int", line, col))
            }
            "<" => Ok(Value::Bool(x < y)),
            ">" => Ok(Value::Bool(x > y)),
            "<=" => Ok(Value::Bool(x <= y)),
            ">=" => Ok(Value::Bool(x >= y)),
            _ => Err(self.runtime("unknown operator", line, col)),
        }
    }

    fn resolve_value(&self, name: &str, line: u32, col: u32) -> Result<Value, Halt> {
        if let Some(local) = self.find_local(name) {
            return Ok(local.value);
        }
        if let Some(field) = self.field_on_self(name) {
            return Ok(field);
        }
        if let Some(selfb) = self.find_local("self") {
            if let Value::Struct(rec) = &selfb.value {
                let ty = rec.borrow().name.clone();
                if self.checker.lookup_type_signal(&ty, name).is_some() {
                    return Ok(Value::SignalRef {
                        name: name.to_string(),
                        owner: Some(rec.clone()),
                    });
                }
            }
        }
        if let Some(global) = self.find_global(name) {
            return Ok(global.value);
        }
        if self.fns.contains_key(name) {
            return Ok(Value::fn_ref(name));
        }
        if let Some(en) = self.checker.find_enum(name) {
            return Ok(Value::EnumType(en.name.clone()));
        }
        if self.checker.singletons.borrow().contains(name) {
            return Err(self.runtime(
                &format!("'{name}' is available when the script runs in Godot"),
                line,
                col,
            ));
        }
        if let Some(n) = self.checker.lookup_const("", name) {
            return Ok(Value::Int(n));
        }
        Err(self.runtime(&format!("undefined variable '{name}'"), line, col))
    }

    fn find_local(&self, name: &str) -> Option<Binding> {
        self.env.last().and_then(|frame| frame.get(name).cloned())
    }

    fn find_global(&self, name: &str) -> Option<Binding> {
        self.env.first().and_then(|frame| frame.get(name).cloned())
    }

    fn find_var(&self, name: &str) -> Option<Binding> {
        self.find_local(name).or_else(|| self.find_global(name))
    }

    fn self_rec(&self) -> Option<Rc<RefCell<StructData>>> {
        match self.find_local("self")?.value {
            Value::Struct(rec) => Some(rec),
            _ => None,
        }
    }

    fn field_on_self(&self, name: &str) -> Option<Value> {
        self.self_rec()?.borrow().fields.get(name).cloned()
    }

    fn is_signal_on_self(&self, name: &str) -> bool {
        let Some(selfb) = self.find_local("self") else {
            return false;
        };
        let Value::Struct(rec) = selfb.value else {
            return false;
        };
        let ty = rec.borrow().name.clone();
        self.checker.lookup_type_signal(&ty, name).is_some()
    }

    fn init_instance_signals(&self, data: &mut StructData) {
        let mut current = data.name.clone();
        let mut seen = HashSet::new();
        while !current.is_empty() {
            if !seen.insert(current.clone()) {
                break;
            }
            for sig in self.checker.type_signal_names(&current) {
                data.listeners.entry(sig).or_default();
            }
            let parent = self.checker.class_parents.borrow().get(&current).cloned();
            current = match parent {
                Some(p) if !p.is_empty() => type_head(&p),
                _ => break,
            };
        }
    }

    fn self_object(&self, line: u32, col: u32) -> Result<Value, Halt> {
        match self.find_var("self") {
            Some(b) if matches!(b.value, Value::Struct(_)) => Ok(b.value),
            _ => Err(self.runtime("super requires self", line, col)),
        }
    }

    fn call_super(
        &mut self,
        name: &str,
        args: Vec<Value>,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        let obj = self.self_object(line, col)?;
        if self.super_type.is_empty() {
            return Err(self.runtime(
                "super is only valid in a method of a class that extends another",
                line,
                col,
            ));
        }
        let start = self.super_type.clone();
        self.invoke_type_method(obj, &start, name, args, line, col)
    }

    fn call_ufcs(
        &mut self,
        func: &FnDecl,
        obj: Value,
        args: Vec<Value>,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        let mut all = Vec::with_capacity(args.len() + 1);
        all.push(obj);
        all.extend(args);
        self.call_user(func, all, line, col, None, None)
    }

    fn construct_enum(
        &self,
        en: &rosegold_syntax::EnumDecl,
        variant: &str,
        args: Vec<Value>,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        let Some(found) = en.variants.iter().find(|v| v.name == variant) else {
            return Err(self.runtime(
                &format!("enum {} has no variant '{variant}'", en.name),
                line,
                col,
            ));
        };
        if args.len() as i32 != found.arity {
            return Err(self.runtime(
                &format!("{}.{} takes {} argument(s)", en.name, variant, found.arity),
                line,
                col,
            ));
        }
        Ok(Value::Enum {
            ty: en.name.clone(),
            variant: variant.to_string(),
            payload: args,
        })
    }

    fn invoke_type_method(
        &mut self,
        obj: Value,
        start: &str,
        name: &str,
        args: Vec<Value>,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        let (defined_on, func) = self.checker.lookup_method(start, name);
        let Some(func) = func else {
            if let Some(ufcs) = self.checker.find_ufcs(name, start) {
                return self.call_ufcs(&ufcs, obj, args, line, col);
            }
            return Err(self.runtime(&format!("struct {start} has no method '{name}'"), line, col));
        };
        if func.is_abstract {
            return Err(self.runtime(&format!("cannot call abstract method '{name}'"), line, col));
        }
        let min = min_arity(&func, true);
        let max = max_arity(&func, true);
        if args.len() < min || (args.len() > max && !func.is_vararg) {
            let span = if func.is_vararg {
                format!("at least {min}")
            } else if min == max {
                max.to_string()
            } else {
                format!("{min} to {max}")
            };
            return Err(self.runtime(
                &format!("{start}.{name} takes {span} argument(s)"),
                line,
                col,
            ));
        }
        let parent = self
            .checker
            .class_parents
            .borrow()
            .get(&defined_on)
            .cloned()
            .unwrap_or_default();
        let next_super = if parent.is_empty() {
            String::new()
        } else {
            type_head(&parent)
        };
        let prev = std::mem::replace(&mut self.super_type, next_super);
        let mut all = Vec::with_capacity(args.len() + 1);
        all.push(obj);
        all.extend(args);
        let result = self.call_user(&func, all, line, col, None, None);
        self.super_type = prev;
        result
    }

    fn call_value_method(
        &mut self,
        obj: Value,
        name: &str,
        args: Vec<Value>,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        if let Value::Array(items) = &obj {
            if name == "len" {
                if !args.is_empty() {
                    return Err(self.runtime("Array.len takes 0 arguments", line, col));
                }
                return Ok(Value::Int(items.borrow().len() as i64));
            }
            if name == "push" {
                if args.len() != 1 {
                    return Err(self.runtime("Array.push takes 1 argument", line, col));
                }
                items.borrow_mut().push(args[0].clone());
                return Ok(Value::Void);
            }
            if name == "pop" {
                if !args.is_empty() {
                    return Err(self.runtime("Array.pop takes 0 arguments", line, col));
                }
                let mut items = items.borrow_mut();
                if items.is_empty() {
                    return Err(self.runtime("pop from empty array", line, col));
                }
                return Ok(items.pop().unwrap());
            }
            if let Some(ufcs) = self.checker.find_ufcs(name, "Array") {
                return self.call_ufcs(&ufcs, obj.clone(), args, line, col);
            }
            return Err(self.runtime(&format!("Array has no method '{name}'"), line, col));
        }
        if let Value::Map(dict) = &obj {
            if name == "len" {
                if !args.is_empty() {
                    return Err(self.runtime("Map.len takes 0 arguments", line, col));
                }
                return Ok(Value::Int(dict.borrow().fields.len() as i64));
            }
            if name == "has" {
                if args.len() != 1 {
                    return Err(self.runtime("Map.has takes 1 argument", line, col));
                }
                let key = map_key(&args[0], line, col).map_err(|m| self.runtime(&m, line, col))?;
                return Ok(Value::Bool(dict.borrow().fields.contains_key(&key)));
            }
            if name == "keys" {
                if !args.is_empty() {
                    return Err(self.runtime("Map.keys takes 0 arguments", line, col));
                }
                let keys = dict
                    .borrow()
                    .order
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect();
                return Ok(Value::array(keys));
            }
            if name == "remove" {
                if args.len() != 1 {
                    return Err(self.runtime("Map.remove takes 1 argument", line, col));
                }
                let key = map_key(&args[0], line, col).map_err(|m| self.runtime(&m, line, col))?;
                let removed = dict.borrow_mut().fields.remove(&key);
                let Some(removed) = removed else {
                    return Err(self.runtime(&format!("key '{key}' not found"), line, col));
                };
                dict.borrow_mut().order.retain(|k| k != &key);
                return Ok(removed);
            }
            if name == "insert" {
                if args.len() != 2 {
                    return Err(self.runtime("Map.insert takes 2 arguments", line, col));
                }
                let key = map_key(&args[0], line, col).map_err(|m| self.runtime(&m, line, col))?;
                let mut dict = dict.borrow_mut();
                if !dict.fields.contains_key(&key) {
                    dict.order.push(key.clone());
                }
                dict.fields.insert(key, args[1].clone());
                return Ok(Value::Void);
            }
            if let Some(ufcs) = self.checker.find_ufcs(name, "Map") {
                return self.call_ufcs(&ufcs, obj.clone(), args, line, col);
            }
            return Err(self.runtime(&format!("Map has no method '{name}'"), line, col));
        }
        if let Value::String(s) = &obj {
            if name == "len" {
                if !args.is_empty() {
                    return Err(self.runtime("String.len takes 0 arguments", line, col));
                }
                return Ok(Value::Int(utf16_len(s)));
            }
            if let Some(ufcs) = self.checker.find_ufcs(name, "String") {
                return self.call_ufcs(&ufcs, obj.clone(), args, line, col);
            }
            return Err(self.runtime(&format!("String has no method '{name}'"), line, col));
        }
        if let Value::Future(fut) = &obj {
            if name == "cancel" {
                if !args.is_empty() {
                    return Err(self.runtime("Future.cancel takes 0 arguments", line, col));
                }
                let fut = fut.clone();
                self.cancel_future(&fut, line, col);
                return Ok(Value::Void);
            }
            return Err(self.runtime(&format!("Future has no method '{name}'"), line, col));
        }
        let ty = obj.type_name();
        if let Some(ufcs) = self.checker.find_ufcs(name, &ty) {
            return self.call_ufcs(&ufcs, obj, args, line, col);
        }
        Err(self.runtime(
            &format!("cannot call method '{name}' on {}", obj.to_print()),
            line,
            col,
        ))
    }

    fn read_value_member(
        &self,
        obj: &Value,
        name: &str,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        match obj {
            Value::Array(items) if name == "len" => Ok(Value::Int(items.borrow().len() as i64)),
            Value::Array(_) => {
                Err(self.runtime(&format!("Array has no member '{name}'"), line, col))
            }
            Value::Map(dict) if name == "len" => Ok(Value::Int(dict.borrow().fields.len() as i64)),
            Value::Map(_) => Err(self.runtime(&format!("Map has no member '{name}'"), line, col)),
            Value::String(s) if name == "len" => Ok(Value::Int(utf16_len(s))),
            Value::String(_) => {
                Err(self.runtime(&format!("String has no member '{name}'"), line, col))
            }
            _ => Err(self.runtime(
                &format!("cannot read field on {}", obj.to_print()),
                line,
                col,
            )),
        }
    }

    fn apply_assign_op(
        &self,
        op: &str,
        lhs: Value,
        rhs: Value,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        if !op.is_empty() && op != "=" {
            let bin = &op[..op.len() - 1];
            return self.apply_binop(bin, lhs, rhs, line, col);
        }
        Ok(rhs)
    }

    fn assign_name(
        &mut self,
        name: &str,
        op: &str,
        rhs: Value,
        line: u32,
        col: u32,
    ) -> Result<(), Halt> {
        if let Some((is_const, cur)) = self.find_local(name).map(|b| (b.is_const, b.value)) {
            if is_const {
                return Err(self.runtime(&format!("cannot assign to const '{name}'"), line, col));
            }
            let newv = self.apply_assign_op(op, cur, rhs, line, col)?;
            self.env.last_mut().unwrap().get_mut(name).unwrap().value = newv;
            return Ok(());
        }
        if let Some(rec) = self.self_rec() {
            let (has, ty, cur) = {
                let data = rec.borrow();
                (
                    data.fields.contains_key(name),
                    data.name.clone(),
                    data.fields.get(name).cloned(),
                )
            };
            if has {
                if self.checker.is_data_type(&ty) {
                    return Err(self.runtime(
                        &format!("cannot assign to data field '{name}'"),
                        line,
                        col,
                    ));
                }
                let newv = self.apply_assign_op(op, cur.unwrap_or(Value::Void), rhs, line, col)?;
                rec.borrow_mut().fields.insert(name.to_string(), newv);
                return Ok(());
            }
        }
        if let Some((is_const, cur)) = self.find_global(name).map(|b| (b.is_const, b.value)) {
            if is_const {
                return Err(self.runtime(&format!("cannot assign to const '{name}'"), line, col));
            }
            let newv = self.apply_assign_op(op, cur, rhs, line, col)?;
            self.env.first_mut().unwrap().get_mut(name).unwrap().value = newv;
            return Ok(());
        }
        if self.checker.lookup_const("", name).is_some() {
            return Err(self.runtime(&format!("cannot assign to const '{name}'"), line, col));
        }
        Err(self.runtime(&format!("undefined variable '{name}'"), line, col))
    }

    fn call_signal(
        &mut self,
        signal: &str,
        name: &str,
        args: Vec<Value>,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        let Some(arity) = self.checker.signal_arity.borrow().get(signal).copied() else {
            return Err(self.runtime(&format!("undefined signal '{signal}'"), line, col));
        };
        self.listeners.entry(signal.to_string()).or_default();
        self.call_signal_list(signal, arity, None, name, args, line, col)
    }

    fn dispatch_signal(
        &mut self,
        sig: Value,
        name: &str,
        args: Vec<Value>,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        let Value::SignalRef {
            name: signal,
            owner,
        } = sig
        else {
            return Err(self.runtime("can only call signal methods on a signal", line, col));
        };
        if owner.is_none() {
            if !self.checker.signal_arity.borrow().contains_key(&signal) {
                return Err(self.runtime(&format!("undefined signal '{signal}'"), line, col));
            }
            let arity = self
                .checker
                .signal_arity
                .borrow()
                .get(&signal)
                .copied()
                .unwrap_or(0);
            self.listeners.entry(signal.clone()).or_default();
            return self.call_signal_list(&signal, arity, None, name, args, line, col);
        }
        let rec = owner.unwrap();
        let ty = rec.borrow().name.clone();
        let Some(arity) = self.checker.lookup_type_signal(&ty, &signal) else {
            return Err(self.runtime(&format!("struct {ty} has no signal '{signal}'"), line, col));
        };
        rec.borrow_mut()
            .listeners
            .entry(signal.clone())
            .or_default();
        self.call_signal_list(&signal, arity, Some(rec), name, args, line, col)
    }

    fn call_signal_list(
        &mut self,
        signal: &str,
        arity: i32,
        rec: Option<Rc<RefCell<StructData>>>,
        name: &str,
        args: Vec<Value>,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        let list = |ip: &Interp, rec: &Option<Rc<RefCell<StructData>>>| -> Vec<Value> {
            if let Some(rec) = rec {
                rec.borrow()
                    .listeners
                    .get(signal)
                    .cloned()
                    .unwrap_or_default()
            } else {
                ip.listeners.get(signal).cloned().unwrap_or_default()
            }
        };
        if name == "connect" {
            if args.len() != 1 {
                return Err(self.runtime(
                    &format!("signal '{signal}' connect takes 1 argument"),
                    line,
                    col,
                ));
            }
            if !matches!(args[0], Value::FnRef { .. }) {
                return Err(self.runtime(
                    &format!("signal '{signal}' connect expects a function"),
                    line,
                    col,
                ));
            }
            let got = self.signal_fn_arity(&args[0], signal, line, col)?;
            if got != arity as usize {
                return Err(self.runtime(
                    &format!("signal '{signal}' connect expected {arity} parameter(s)"),
                    line,
                    col,
                ));
            }
            let existing = list(self, &rec);
            if existing.iter().any(|fnv| same_fn(fnv, &args[0])) {
                return Ok(Value::Void);
            }
            if let Some(rec) = rec {
                rec.borrow_mut()
                    .listeners
                    .entry(signal.to_string())
                    .or_default()
                    .push(args[0].clone());
            } else {
                self.listeners
                    .entry(signal.to_string())
                    .or_default()
                    .push(args[0].clone());
            }
            return Ok(Value::Void);
        }
        if name == "disconnect" {
            if args.len() != 1 {
                return Err(self.runtime(
                    &format!("signal '{signal}' disconnect takes 1 argument"),
                    line,
                    col,
                ));
            }
            if !matches!(args[0], Value::FnRef { .. }) {
                return Err(self.runtime(
                    &format!("signal '{signal}' disconnect expects a function"),
                    line,
                    col,
                ));
            }
            if let Value::FnRef {
                name: fname,
                clo: None,
            } = &args[0]
            {
                if !self.fns.contains_key(fname) && self.checker.find_local_fn(fname).is_none() {
                    return Err(self.runtime(&format!("undefined function '{fname}'"), line, col));
                }
            }
            if let Some(rec) = rec {
                if let Some(slot) = rec.borrow_mut().listeners.get_mut(signal) {
                    slot.retain(|fnv| !same_fn(fnv, &args[0]));
                }
            } else if let Some(slot) = self.listeners.get_mut(signal) {
                slot.retain(|fnv| !same_fn(fnv, &args[0]));
            }
            return Ok(Value::Void);
        }
        if name == "emit" || name == "emit_deferred" {
            if args.len() as i32 != arity {
                return Err(self.runtime(
                    &format!(
                        "signal '{signal}' expected {arity} argument(s), got {}",
                        args.len()
                    ),
                    line,
                    col,
                ));
            }
            if name == "emit_deferred" {
                self.deferred.push(DeferredEmit {
                    signal: signal.to_string(),
                    rec,
                    args,
                    line,
                    col,
                });
                return Ok(Value::Void);
            }
            if self.sync_emit_depth >= MAX_SYNC_EMIT_DEPTH {
                return Err(self.runtime("signal emit nested too deeply", line, col));
            }
            let listeners = list(self, &rec);
            self.sync_emit_depth += 1;
            let mut result = Ok(Value::Void);
            for fnv in listeners {
                if let Err(err) = self.call_fn_value(fnv, args.clone(), line, col) {
                    result = Err(err);
                    break;
                }
            }
            self.sync_emit_depth -= 1;
            return result;
        }
        Err(self.runtime(
            &format!("signal '{signal}' has no method '{name}'"),
            line,
            col,
        ))
    }

    fn flush_deferred(&mut self) -> Result<(), Halt> {
        let mut waves = 0;
        while !self.deferred.is_empty() {
            waves += 1;
            if waves > 64 {
                let first = &self.deferred[0];
                return Err(self.runtime("deferred emit nested too deeply", first.line, first.col));
            }
            let batch = std::mem::take(&mut self.deferred);
            for item in batch {
                let list = if let Some(rec) = &item.rec {
                    let ty = rec.borrow().name.clone();
                    if self.checker.lookup_type_signal(&ty, &item.signal).is_none() {
                        return Err(self.runtime(
                            &format!("struct {ty} has no signal '{}'", item.signal),
                            item.line,
                            item.col,
                        ));
                    }
                    rec.borrow()
                        .listeners
                        .get(&item.signal)
                        .cloned()
                        .unwrap_or_default()
                } else {
                    self.listeners
                        .get(&item.signal)
                        .cloned()
                        .unwrap_or_default()
                };
                for fnv in list {
                    self.call_fn_value(fnv, item.args.clone(), item.line, item.col)?;
                }
            }
        }
        Ok(())
    }

    fn signal_fn_arity(
        &self,
        func: &Value,
        signal: &str,
        line: u32,
        col: u32,
    ) -> Result<usize, Halt> {
        match func {
            Value::FnRef { clo: Some(clo), .. } => Ok(clo.func.params.len()),
            Value::FnRef { name, clo: None } => {
                if let Some(decl) = self.fns.get(name) {
                    return Ok(decl.params.len());
                }
                if let Some(decl) = self.checker.find_local_fn(name) {
                    return Ok(decl.params.len());
                }
                Err(self.runtime(&format!("undefined function '{name}'"), line, col))
            }
            _ => Err(self.runtime(
                &format!("signal '{signal}' connect expects a function"),
                line,
                col,
            )),
        }
    }

    fn index_get(&self, obj: &Value, idx: &Value, line: u32, col: u32) -> Result<Value, Halt> {
        if let Value::Map(dict) = obj {
            let Value::String(key) = idx else {
                return Err(self.runtime("map key must be String", line, col));
            };
            return dict
                .borrow()
                .fields
                .get(key)
                .cloned()
                .ok_or_else(|| self.runtime(&format!("key '{key}' not found"), line, col));
        }
        let Value::Int(i) = idx else {
            return Err(self.runtime("index must be Int", line, col));
        };
        if *i < 0 {
            return Err(self.runtime(&format!("index {i} out of bounds"), line, col));
        }
        if let Value::Array(items) = obj {
            let items = items.borrow();
            if *i as usize >= items.len() {
                return Err(self.runtime(&format!("index {i} out of bounds"), line, col));
            }
            return Ok(items[*i as usize].clone());
        }
        if let Value::String(s) = obj {
            return utf16_index_char(s, *i as usize)
                .map(Value::String)
                .ok_or_else(|| self.runtime(&format!("index {i} out of bounds"), line, col));
        }
        Err(self.runtime(&format!("cannot index {}", obj.to_print()), line, col))
    }

    fn index_set(
        &self,
        obj: &Value,
        idx: &Value,
        value: Value,
        line: u32,
        col: u32,
    ) -> Result<(), Halt> {
        if let Value::Map(dict) = obj {
            let Value::String(key) = idx else {
                return Err(self.runtime("map key must be String", line, col));
            };
            let mut dict = dict.borrow_mut();
            if !dict.fields.contains_key(key) {
                dict.order.push(key.clone());
            }
            dict.fields.insert(key.clone(), value);
            return Ok(());
        }
        let Value::Array(items) = obj else {
            return Err(self.runtime(&format!("cannot assign to {}", obj.to_print()), line, col));
        };
        let Value::Int(i) = idx else {
            return Err(self.runtime(
                &format!("index {} out of bounds", idx.to_print()),
                line,
                col,
            ));
        };
        let mut items = items.borrow_mut();
        if *i < 0 || *i as usize >= items.len() {
            return Err(self.runtime(&format!("index {i} out of bounds"), line, col));
        }
        items[*i as usize] = value;
        Ok(())
    }

    fn iter_items(&self, iter: Value, line: u32, col: u32) -> Result<Vec<Value>, Halt> {
        match iter {
            Value::Array(items) => Ok(items.borrow().clone()),
            Value::Map(dict) => Ok(dict
                .borrow()
                .order
                .iter()
                .cloned()
                .map(Value::String)
                .collect()),
            Value::String(s) => Ok(utf16_units(&s)
                .into_iter()
                .map(|u| Value::String(from_units(&[u])))
                .collect()),
            Value::Int(n) => {
                if n <= 0 {
                    return Ok(Vec::new());
                }
                if n > MAX_RANGE {
                    return Err(self.runtime("range too large", line, col));
                }
                Ok((0..n).map(Value::Int).collect())
            }
            Value::Range {
                start,
                end,
                inclusive,
            } => {
                let mut nums = Vec::new();
                if inclusive {
                    let mut n = start;
                    loop {
                        if n > end {
                            break;
                        }
                        if nums.len() as i64 >= MAX_RANGE {
                            return Err(self.runtime("range too large", line, col));
                        }
                        nums.push(Value::Int(n));
                        if n == i64::MAX {
                            break;
                        }
                        n += 1;
                    }
                } else {
                    let mut n = start;
                    while n < end {
                        if nums.len() as i64 >= MAX_RANGE {
                            return Err(self.runtime("range too large", line, col));
                        }
                        nums.push(Value::Int(n));
                        n += 1;
                    }
                }
                Ok(nums)
            }
            other => Err(self.runtime(
                &format!("cannot iterate over {}", other.to_print()),
                line,
                col,
            )),
        }
    }
}

struct AllState {
    out: Rc<RefCell<FutureData>>,
    results: RefCell<Vec<Value>>,
    remaining: std::cell::Cell<usize>,
    done: std::cell::Cell<bool>,
}

struct RaceState {
    out: Rc<RefCell<FutureData>>,
    done: std::cell::Cell<bool>,
}

fn collect_free_stmt(stmt: &Stmt, bound: &mut HashSet<String>, free: &mut HashSet<String>) {
    match stmt.kind {
        StmtKind::Var | StmtKind::Const => {
            if let Some(expr) = &stmt.expr {
                collect_free_expr(expr, bound, free);
            }
            bound.insert(stmt.name.clone());
        }
        StmtKind::Assign => {
            if let Some(expr) = &stmt.expr {
                collect_free_expr(expr, bound, free);
            }
            if !bound.contains(&stmt.name) && !skip_capture(&stmt.name) {
                free.insert(stmt.name.clone());
            }
        }
        StmtKind::FieldAssign | StmtKind::IndexAssign => {
            if let Some(target) = &stmt.target {
                collect_free_expr(target, bound, free);
            }
            if let Some(expr) = &stmt.expr {
                collect_free_expr(expr, bound, free);
            }
        }
        StmtKind::Return | StmtKind::Expr | StmtKind::Throw => {
            if let Some(expr) = &stmt.expr {
                collect_free_expr(expr, bound, free);
            }
        }
        StmtKind::If => {
            if let Some(expr) = &stmt.expr {
                collect_free_expr(expr, bound, free);
            }
            for st in &stmt.body {
                collect_free_stmt(st, bound, free);
            }
            for st in &stmt.else_body {
                collect_free_stmt(st, bound, free);
            }
        }
        StmtKind::While => {
            if let Some(expr) = &stmt.expr {
                collect_free_expr(expr, bound, free);
            }
            for st in &stmt.body {
                collect_free_stmt(st, bound, free);
            }
        }
        StmtKind::For => {
            if let Some(expr) = &stmt.expr {
                collect_free_expr(expr, bound, free);
            }
            let mut inner = bound.clone();
            inner.insert(stmt.name.clone());
            for st in &stmt.body {
                collect_free_stmt(st, &mut inner, free);
            }
        }
        StmtKind::Match => {
            if let Some(expr) = &stmt.expr {
                collect_free_expr(expr, bound, free);
            }
            for arm in &stmt.arms {
                let mut inner = bound.clone();
                inner.extend(arm.binds.iter().cloned());
                for st in &arm.body {
                    collect_free_stmt(st, &mut inner, free);
                }
            }
        }
        StmtKind::Do => {
            for st in &stmt.body {
                collect_free_stmt(st, bound, free);
            }
            let mut inner = bound.clone();
            if !stmt.name.is_empty() {
                inner.insert(stmt.name.clone());
            }
            for st in &stmt.else_body {
                collect_free_stmt(st, &mut inner, free);
            }
        }
        _ => {}
    }
}

fn collect_free_expr(e: &Expr, bound: &HashSet<String>, free: &mut HashSet<String>) {
    if e.kind == ExprKind::Lambda {
        if let Some(func) = &e.lambda {
            let mut inner = bound.clone();
            inner.extend(func.params.iter().cloned());
            for st in &func.body {
                collect_free_stmt(st, &mut inner, free);
            }
        }
        return;
    }
    if e.kind == ExprKind::NodePath {
        if !bound.contains("self") {
            free.insert("self".to_string());
        }
        return;
    }
    if e.kind == ExprKind::Var {
        if !bound.contains(&e.text) && !skip_capture(&e.text) {
            free.insert(e.text.clone());
        }
        return;
    }
    if e.kind == ExprKind::Call
        && e.module.is_empty()
        && !e.text.is_empty()
        && !bound.contains(&e.text)
        && !skip_capture(&e.text)
    {
        free.insert(e.text.clone());
    }
    for kid in &e.kids {
        collect_free_expr(kid, bound, free);
    }
}

fn map_key(v: &Value, _line: u32, _col: u32) -> Result<String, String> {
    match v {
        Value::String(s) => Ok(s.clone()),
        Value::Int(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        _ => Err("map key must be String".to_string()),
    }
}

fn java_trunc_i64(n: f64) -> i64 {
    if n.is_nan() {
        return 0;
    }
    if n >= 9.223372036854776e18 {
        return i64::MAX;
    }
    if n <= -9.223372036854776e18 {
        return i64::MIN;
    }
    n as i64
}

fn need_string(args: &[Value], i: usize, what: &str) -> Result<String, Halt> {
    match args.get(i) {
        Some(Value::String(s)) => Ok(s.clone()),
        _ => Err(Halt::Runtime(format!("{what} expects String"))),
    }
}

fn uuid_hex32(s: &str) -> Option<String> {
    let mut hex = String::new();
    for c in s.chars() {
        if c == '-' {
            continue;
        }
        if !c.is_ascii_hexdigit() {
            return None;
        }
        hex.push(c.to_ascii_lowercase());
    }
    if hex.len() == 32 {
        Some(hex)
    } else {
        None
    }
}

fn dashed_uuid(hex: &str) -> String {
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

fn byte_to_utf16(s: &str, byte: usize) -> i64 {
    s[..byte].encode_utf16().count() as i64
}

fn as_regex_repl(with: &str) -> String {
    with.to_string()
}

fn split_lines(text: &str) -> Vec<Value> {
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0usize;
    for i in 0..=bytes.len() {
        if i != bytes.len() && bytes[i] != b'\n' {
            continue;
        }
        if i == bytes.len() && start == i {
            break;
        }
        let mut row = &text[start..i];
        if let Some(stripped) = row.strip_suffix('\r') {
            row = stripped;
        }
        lines.push(Value::String(row.to_string()));
        start = i + 1;
    }
    lines
}

fn remove_path(path: &Path) -> bool {
    match fs::remove_file(path) {
        Ok(()) => true,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => fs::remove_dir(path).is_ok(),
    }
}

fn generic_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn sandbox_root() -> PathBuf {
    normalize_abs(&std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

fn normalize_abs(path: &Path) -> PathBuf {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let mut out = PathBuf::new();
    for c in abs.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn within(root: &Path, canon: &Path) -> bool {
    let r = path_key(root);
    let c = path_key(canon);
    if c == r {
        return true;
    }
    let with_sep = if r.ends_with('\\') {
        r.clone()
    } else {
        format!("{r}\\")
    };
    c.starts_with(&with_sep)
}

impl Interp {
    fn call_qualified(
        &mut self,
        module: &str,
        name: &str,
        args: Vec<Value>,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        if module.is_empty() {
            return self.call("", name, args, line, col);
        }
        if self.checker.loaded.borrow().contains_key(module) {
            return self.call_module(module, name, args, line, col);
        }
        self.call(module, name, args, line, col)
    }

    fn global_signal(&self, name: &str, line: u32, col: u32) -> Result<Value, Halt> {
        if !self.checker.signal_arity.borrow().contains_key(name) {
            return Err(self.runtime(&format!("undefined signal '{name}'"), line, col));
        }
        Ok(Value::SignalRef {
            name: name.to_string(),
            owner: None,
        })
    }

    fn struct_lit_filled(
        &mut self,
        type_name: &str,
        fields: &[Value],
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        if self.checker.class_abstract.borrow().get(type_name).copied() == Some(true) {
            return Err(self.runtime(
                &format!("cannot construct abstract class '{type_name}'"),
                line,
                col,
            ));
        }
        let Some(decl) = self.checker.find_struct(type_name) else {
            return Err(self.runtime(&format!("undefined struct '{type_name}'"), line, col));
        };
        let (name, names) = {
            let decl = decl.borrow();
            (decl.name.clone(), decl.fields.clone())
        };
        if fields.len() != names.len() {
            return Err(self.runtime(&format!("missing field on {name}"), line, col));
        }
        let mut data = StructData {
            name,
            order: names.clone(),
            fields: HashMap::new(),
            listeners: HashMap::new(),
        };
        for (i, field) in names.iter().enumerate() {
            data.fields.insert(field.clone(), fields[i].clone());
        }
        self.init_instance_signals(&mut data);
        Ok(Value::struc(data))
    }

    fn set_field(
        &self,
        obj: &Value,
        name: &str,
        rhs: Value,
        line: u32,
        col: u32,
    ) -> Result<(), Halt> {
        let Value::Struct(rec) = obj else {
            return Err(self.runtime(
                &format!("cannot assign field on {}", obj.to_print()),
                line,
                col,
            ));
        };
        let (has, ty) = {
            let data = rec.borrow();
            (data.fields.contains_key(name), data.name.clone())
        };
        if !has {
            return Err(self.runtime(&format!("struct {ty} has no field '{name}'"), line, col));
        }
        if self.checker.is_data_type(&ty) {
            return Err(self.runtime(&format!("cannot assign to data field '{name}'"), line, col));
        }
        rec.borrow_mut().fields.insert(name.to_string(), rhs);
        Ok(())
    }

    fn map_values(&self, kids: &[Value], line: u32, col: u32) -> Result<Value, Halt> {
        if kids.len() % 2 != 0 {
            return Err(self.runtime("invalid map literal", line, col));
        }
        let mut data = MapData {
            order: Vec::new(),
            fields: HashMap::new(),
        };
        let mut i = 0;
        while i < kids.len() {
            let Value::String(key) = &kids[i] else {
                return Err(self.runtime("map key must be String", line, col));
            };
            if !data.fields.contains_key(key) {
                data.order.push(key.clone());
            }
            data.fields.insert(key.clone(), kids[i + 1].clone());
            i += 2;
        }
        Ok(Value::map(data))
    }

    fn call_super_from(
        &mut self,
        obj: Value,
        start: &str,
        name: &str,
        args: Vec<Value>,
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        if start.is_empty() {
            return Err(self.runtime(
                "super is only valid in a method of a class that extends another",
                line,
                col,
            ));
        }
        self.invoke_type_method(obj, start, name, args, line, col)
    }

    fn make_bc_closure(
        &self,
        proto: &FnDecl,
        code: Option<Rc<BcFn>>,
        names: &[String],
        vals: &[Value],
        flags: &[bool],
    ) -> Value {
        let mut caps = HashMap::new();
        for (i, name) in names.iter().enumerate() {
            if name.is_empty() {
                continue;
            }
            caps.insert(
                name.clone(),
                Binding {
                    value: vals.get(i).cloned().unwrap_or(Value::Void),
                    is_const: flags.get(i).copied().unwrap_or(false),
                },
            );
        }
        Value::closure(ClosureData {
            func: Rc::new(proto.clone()),
            caps,
            code,
        })
    }
}

pub use llvm::{
    emit as emit_llvm, exec_native_file, link_and_run as link_native, Native, NativeFile,
};

pub fn dump_ir(source: &str, path: &str) -> Result<String, String> {
    let (program, parse_diags) = rosegold_syntax::parse_source(source, path);
    if let Some(d) = first_error(&parse_diags) {
        let kind = if d.kind.is_empty() {
            "parse error"
        } else {
            d.kind.as_str()
        };
        let file = if d.file.is_empty() {
            path
        } else {
            d.file.as_str()
        };
        return Err(located(kind, file, d.line, d.col, &d.message));
    }
    let (checker, diags) = rosegold_check::check_interpreted(program, path);
    if let Some(d) = first_error(&diags) {
        let kind = if d.kind.is_empty() {
            "error"
        } else {
            d.kind.as_str()
        };
        let file = if d.file.is_empty() {
            path
        } else {
            d.file.as_str()
        };
        return Err(located(kind, file, d.line, d.col, &d.message));
    }
    Ok(compile::dump_entry(&checker))
}

fn path_key(path: &Path) -> String {
    let s = path.to_string_lossy().replace('/', "\\");
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s
    }
}
