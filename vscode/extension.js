const vscode = require("vscode");
const cp = require("child_process");
const fs = require("fs");
const path = require("path");
const terralite = require("./terralite");

/** @type {vscode.ExtensionContext | undefined} */
let context;
/** @type {vscode.DiagnosticCollection | undefined} */
let diagnostics;
/** @type {vscode.OutputChannel | undefined} */
let output;
/** @type {vscode.StatusBarItem | undefined} */
let status;
/** @type {cp.ChildProcessWithoutNullStreams | null} */
let proc = null;
/** @type {Promise<void> | null} */
let starting = null;
let buffer = "";
let seq = 1;
let lastError = null;
let failedAt = 0;
/** @type {Map<number, {resolve: Function, reject: Function}>} */
const pending = new Map();
const timers = new Map();

function activate(ctx) {
  context = ctx;
  diagnostics = vscode.languages.createDiagnosticCollection("rosegold");
  output = vscode.window.createOutputChannel("RoseGold");
  status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
  status.name = "RoseGold";
  status.text = "RoseGold";
  status.show();

  const selector = [
    { language: "rosegold", scheme: "file" },
    { language: "rosegold", scheme: "untitled" },
  ];
  ctx.subscriptions.push(
    diagnostics,
    output,
    status,
    vscode.languages.registerHoverProvider(selector, { provideHover }),
    vscode.languages.registerCompletionItemProvider(selector, { provideCompletionItems }, ".", "@"),
    vscode.languages.registerDefinitionProvider(selector, { provideDefinition }),
    vscode.languages.registerReferenceProvider(selector, { provideReferences }),
    vscode.languages.registerRenameProvider(selector, { provideRenameEdits }),
    vscode.languages.registerSignatureHelpProvider(selector, { provideSignatureHelp }, "(", ","),
    vscode.languages.registerDocumentFormattingEditProvider(selector, { provideDocumentFormattingEdits }),
    vscode.workspace.onDidOpenTextDocument(refresh),
    vscode.workspace.onDidChangeTextDocument((e) => schedule(e.document)),
    vscode.workspace.onDidSaveTextDocument(refresh),
    vscode.workspace.onDidCloseTextDocument((doc) => diagnostics.delete(doc.uri)),
    vscode.commands.registerCommand("rosegold.restart", restart),
    vscode.commands.registerCommand("rosegold.runFile", () => runCli("run")),
    vscode.commands.registerCommand("rosegold.testFile", () => runCli("test"))
  );

  for (const doc of vscode.workspace.textDocuments) {
    refresh(doc);
  }
}

function deactivate() {
  stop();
}

function cfg() {
  return vscode.workspace.getConfiguration("rosegold");
}

function rgBin() {
  const custom = String(cfg().get("rg") || "").trim();
  if (custom) {
    return custom;
  }
  const folders = vscode.workspace.workspaceFolders || [];
  const names = process.platform === "win32" ? ["rg.exe"] : ["rg"];
  for (const folder of folders) {
    for (const rel of ["target/debug", "target/release"]) {
      for (const name of names) {
        const candidate = path.join(folder.uri.fsPath, rel, name);
        if (fs.existsSync(candidate)) {
          return candidate;
        }
      }
    }
  }
  return "rg";
}

function log(line) {
  try {
    output.appendLine(line);
  } catch (err) {
    // The output channel is already closed when the extension host shuts down.
  }
}

function setStatus(text, tip) {
  status.text = text;
  status.tooltip = tip || text;
}

async function ensureServer() {
  if (proc && proc.exitCode == null && !proc.killed) {
    return;
  }
  if (lastError && Date.now() - failedAt < 5000) {
    throw lastError;
  }
  if (starting) {
    return starting;
  }
  starting = launch().finally(() => {
    starting = null;
  });
  return starting;
}

function launch() {
  const bin = rgBin();
  const args = ["ide"];
  log([bin, ...args].join(" "));
  return new Promise((resolve, reject) => {
    let child;
    try {
      child = cp.spawn(bin, args, { cwd: context.extensionPath, windowsHide: true });
    } catch (err) {
      setStatus("RoseGold (failed)", String(err));
      reject(err);
      return;
    }
    proc = child;
    buffer = "";
    let settled = false;
    const fail = (err) => {
      lastError = err;
      failedAt = Date.now();
      if (child.exitCode == null) {
        child.kill();
      }
      if (settled) {
        return;
      }
      settled = true;
      setStatus("RoseGold (failed)", String(err.message || err));
      reject(err);
    };
    child.on("error", fail);
    child.on("exit", (code) => {
      const err = new Error("RoseGold language service exited (" + code + ")");
      log(err.message);
      for (const [, waiter] of pending) {
        waiter.reject(err);
      }
      pending.clear();
      if (proc === child) {
        proc = null;
        lastError = err;
        failedAt = Date.now();
        setStatus("RoseGold (stopped)", err.message);
      }
      if (!settled) {
        fail(err);
      }
    });
    child.stderr.on("data", (chunk) => log(String(chunk).trimEnd()));
    child.stdout.on("data", (chunk) => {
      buffer += chunk.toString("utf8");
      let nl;
      while ((nl = buffer.indexOf("\n")) >= 0) {
        const line = buffer.slice(0, nl).replace(/\r$/, "");
        buffer = buffer.slice(nl + 1);
        if (!line.trim()) {
          continue;
        }
        let msg;
        try {
          msg = JSON.parse(line);
        } catch (err) {
          log("bad response: " + line);
          continue;
        }
        const waiter = pending.get(msg.id);
        if (waiter) {
          pending.delete(msg.id);
          waiter.resolve(msg);
        }
      }
    });
    const ping = requestRaw({ method: "ping" });
    const timer = setTimeout(() => fail(new Error("RoseGold language service did not start")), 15000);
    ping.then(
      () => {
        clearTimeout(timer);
        if (!settled) {
          settled = true;
          lastError = null;
          setStatus("RoseGold", "Language service is running");
          resolve();
        }
      },
      (err) => {
        clearTimeout(timer);
        fail(err);
      }
    );
  });
}

function stop() {
  const child = proc;
  proc = null;
  if (child && child.exitCode == null) {
    child.kill();
  }
  for (const [, waiter] of pending) {
    waiter.reject(new Error("RoseGold language service stopped"));
  }
  pending.clear();
}

async function restart() {
  lastError = null;
  failedAt = 0;
  stop();
  try {
    await ensureServer();
    for (const doc of vscode.workspace.textDocuments) {
      refresh(doc);
    }
    vscode.window.showInformationMessage("RoseGold language service restarted");
  } catch (err) {
    vscode.window.showErrorMessage(String(err.message || err));
  }
}

function requestRaw(body) {
  if (!proc || proc.exitCode != null) {
    return Promise.reject(new Error("RoseGold language service is not running"));
  }
  const id = seq++;
  body.id = id;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error("RoseGold language service timed out"));
    }, 20000);
    pending.set(id, {
      resolve: (msg) => {
        clearTimeout(timer);
        resolve(msg);
      },
      reject: (err) => {
        clearTimeout(timer);
        reject(err);
      },
    });
    proc.stdin.write(JSON.stringify(body) + "\n", (err) => {
      if (err) {
        clearTimeout(timer);
        pending.delete(id);
        reject(err);
      }
    });
  });
}

async function request(body) {
  await ensureServer();
  const msg = await requestRaw(body);
  if (!msg.ok) {
    throw new Error(msg.error || "RoseGold request failed");
  }
  return msg;
}

function isRose(doc) {
  return doc && doc.languageId === "rosegold";
}

function filePath(doc) {
  if (doc.uri.scheme === "file") {
    return doc.uri.fsPath;
  }
  return doc.uri.toString();
}

function payload(doc, position) {
  const body = { path: filePath(doc), source: doc.getText() };
  if (position) {
    body.offset = doc.offsetAt(position);
  }
  return body;
}

function schedule(doc) {
  if (!isRose(doc)) {
    return;
  }
  const key = doc.uri.toString();
  const prev = timers.get(key);
  if (prev) {
    clearTimeout(prev);
  }
  timers.set(
    key,
    setTimeout(() => {
      timers.delete(key);
      refresh(doc);
    }, 400)
  );
}

async function refresh(doc) {
  if (!isRose(doc)) {
    return;
  }
  try {
    const msg = await request({ method: "check", ...payload(doc) });
    if (doc.isClosed) {
      return;
    }
    const items = (msg.diagnostics || []).map((d) => toDiagnostic(doc, d));
    diagnostics.set(doc.uri, items);
    setStatus("RoseGold", "Language service is running");
  } catch (err) {
    log(String(err.message || err));
    const item = new vscode.Diagnostic(
      new vscode.Range(0, 0, 0, 1),
      String(err.message || err),
      vscode.DiagnosticSeverity.Warning
    );
    item.source = "rosegold";
    diagnostics.set(doc.uri, [item]);
  }
}

function toDiagnostic(doc, d) {
  const line = Math.max(0, (d.line || 1) - 1);
  const col = Math.max(0, (d.col || 1) - 1);
  const text = line < doc.lineCount ? doc.lineAt(line).text : "";
  let endLine = line;
  let endCol = col + 1;
  if (d.endLine && d.endCol) {
    endLine = Math.max(0, d.endLine - 1);
    endCol = Math.max(0, d.endCol - 1);
  } else if (col < text.length) {
    const rest = text.slice(col);
    const word = rest.match(/^[A-Za-z0-9_]+|^./);
    endCol = col + (word ? word[0].length : 1);
  }
  if (endLine === line) {
    endCol = Math.min(text.length, Math.max(col, endCol));
  }
  const range = new vscode.Range(line, Math.min(col, text.length), endLine, endCol);
  const sev = String(d.severity || "error").toLowerCase();
  let severity = vscode.DiagnosticSeverity.Error;
  if (sev === "warning" || sev === "warn") {
    severity = vscode.DiagnosticSeverity.Warning;
  } else if (sev === "info" || sev === "information") {
    severity = vscode.DiagnosticSeverity.Information;
  } else if (sev === "hint") {
    severity = vscode.DiagnosticSeverity.Hint;
  }
  let message = d.message || "error";
  if (d.hint) {
    message = message + "\n" + d.hint;
  }
  const item = new vscode.Diagnostic(range, message, severity);
  item.source = "rosegold";
  if (d.kind) {
    item.code = d.kind;
  }
  if (d.kind === "unused" && vscode.DiagnosticTag) {
    item.tags = [vscode.DiagnosticTag.Unnecessary];
  }
  return item;
}

async function provideHover(doc, position) {
  const local = terraliteHover(doc, position);
  let msg;
  try {
    msg = await request({ method: "hover", ...payload(doc, position) });
  } catch (err) {
    log(String(err.message || err));
    return local;
  }
  const info = msg.hover;
  if (local) {
    return local;
  }
  if (!info) {
    return null;
  }
  const md = new vscode.MarkdownString();
  md.appendCodeblock(info.signature || info.name || "", "rosegold");
  const bits = [];
  if (info.kind) {
    bits.push(info.kind);
  }
  if (info.crate) {
    bits.push(info.crate);
  }
  if (info.deprecated) {
    bits.push("deprecated");
  }
  if (bits.length) {
    md.appendMarkdown("\n\n" + bits.join(" · "));
  }
  if (info.doc) {
    md.appendMarkdown("\n\n" + info.doc);
  }
  const start = offsetPos(doc, info.start);
  const end = offsetPos(doc, info.end);
  return new vscode.Hover(md, new vscode.Range(start, end));
}

const COMPLETION_KIND = {
  keyword: vscode.CompletionItemKind.Keyword,
  type: vscode.CompletionItemKind.TypeParameter,
  function: vscode.CompletionItemKind.Function,
  method: vscode.CompletionItemKind.Method,
  field: vscode.CompletionItemKind.Field,
  variable: vscode.CompletionItemKind.Variable,
  class: vscode.CompletionItemKind.Class,
  struct: vscode.CompletionItemKind.Struct,
  module: vscode.CompletionItemKind.Module,
  signal: vscode.CompletionItemKind.Event,
};

async function provideCompletionItems(doc, position) {
  let remote = [];
  try {
    const msg = await request({ method: "complete", ...payload(doc, position) });
    remote = msg.items || [];
  } catch (err) {
    log(String(err.message || err));
  }
  const items = remote.map((item) => {
    const done = new vscode.CompletionItem(item.label, COMPLETION_KIND[item.kind] || vscode.CompletionItemKind.Text);
    done.detail = item.detail || item.kind;
    return done;
  });
  const seen = new Map(items.map((item) => [item.label, item]));
  const before = doc.getText(new vscode.Range(new vscode.Position(0, 0), position));
  for (const extra of terralite.complete(before)) {
    const kind = COMPLETION_KIND[extra.kind] || vscode.CompletionItemKind.Function;
    const md = new vscode.MarkdownString();
    if (extra.doc) {
      md.appendMarkdown(extra.doc);
    }
    const existing = seen.get(extra.label);
    if (existing) {
      existing.kind = kind;
      existing.detail = extra.detail;
      if (extra.doc) {
        existing.documentation = md;
      }
      continue;
    }
    const done = new vscode.CompletionItem(extra.label, kind);
    done.detail = extra.detail;
    if (extra.doc) {
      done.documentation = md;
    }
    seen.set(extra.label, done);
    items.push(done);
  }
  return items;
}

function terraliteHover(doc, position) {
  const line = doc.lineAt(position.line).text;
  const hit = terralite.at(line, position.character);
  if (!hit) {
    return null;
  }
  const info = terralite.hover(hit.qualifier, hit.name);
  if (!info) {
    return null;
  }
  const md = new vscode.MarkdownString();
  md.appendCodeblock(info.signature || info.name, "rosegold");
  if (info.kind) {
    md.appendMarkdown("\n\n" + info.kind);
  }
  if (info.doc) {
    md.appendMarkdown("\n\n" + info.doc);
  }
  return new vscode.Hover(md, new vscode.Range(position.line, hit.start, position.line, hit.end));
}

async function provideDefinition(doc, position) {
  let msg;
  try {
    msg = await request({ method: "goto", ...payload(doc, position) });
  } catch (err) {
    log(String(err.message || err));
    return null;
  }
  return msg.loc ? locToLocation(doc, msg.loc) : null;
}

async function provideReferences(doc, position) {
  let msg;
  try {
    msg = await request({ method: "usages", ...payload(doc, position) });
  } catch (err) {
    log(String(err.message || err));
    return [];
  }
  return (msg.hits || []).map((hit) => locToLocation(doc, hit)).filter(Boolean);
}

async function provideRenameEdits(doc, position, newName) {
  const msg = await request({ method: "rename", newName, ...payload(doc, position) });
  const hits = msg.hits || [];
  if (!hits.length) {
    throw new Error("No RoseGold symbol at this position");
  }
  const edit = new vscode.WorkspaceEdit();
  for (const hit of hits) {
    const loc = locToLocation(doc, hit);
    if (loc) {
      edit.replace(loc.uri, loc.range, newName);
    }
  }
  return edit;
}

async function provideSignatureHelp(doc, position) {
  let msg;
  try {
    msg = await request({ method: "signature", ...payload(doc, position) });
  } catch (err) {
    log(String(err.message || err));
    return null;
  }
  const info = msg.signature;
  if (!info) {
    return null;
  }
  const sig = new vscode.SignatureInformation(info.label || "");
  const params = info.params || [];
  for (let i = 0; i < params.length; i++) {
    const start = info.paramStarts ? info.paramStarts[i] : undefined;
    const end = info.paramEnds ? info.paramEnds[i] : undefined;
    const label = start != null && end != null ? [start, end] : params[i];
    sig.parameters.push(new vscode.ParameterInformation(label, params[i]));
  }
  const help = new vscode.SignatureHelp();
  help.signatures = [sig];
  help.activeSignature = 0;
  help.activeParameter = info.activeParam || 0;
  return help;
}

async function provideDocumentFormattingEdits(doc) {
  const msg = await request({
    method: "format",
    compact: cfg().get("format.compact") === true,
    comments: cfg().get("format.comments") !== false,
    ...payload(doc),
  });
  const full = new vscode.Range(doc.positionAt(0), doc.positionAt(doc.getText().length));
  return [vscode.TextEdit.replace(full, msg.text ?? "")];
}

function offsetPos(doc, offset) {
  const n = Math.max(0, Math.min(doc.getText().length, offset || 0));
  return doc.positionAt(n);
}

function locToLocation(doc, loc) {
  if (!loc) {
    return null;
  }
  const same = !loc.path || pathEquals(loc.path, filePath(doc));
  const uri = same ? doc.uri : vscode.Uri.file(loc.path);
  const line = Math.max(0, (loc.line || 1) - 1);
  const col = Math.max(0, (loc.col || 1) - 1);
  const width = Math.max(1, (loc.end || 0) - (loc.start || 0));
  return new vscode.Location(uri, new vscode.Range(line, col, line, col + width));
}

function pathEquals(a, b) {
  if (!a || !b) {
    return false;
  }
  const norm = (p) => path.normalize(p).replace(/\\/g, "/").toLowerCase();
  return norm(a) === norm(b);
}

function activeRose() {
  const doc = vscode.window.activeTextEditor && vscode.window.activeTextEditor.document;
  if (!isRose(doc) || doc.uri.scheme !== "file") {
    vscode.window.showInformationMessage("Open a RoseGold file to run it");
    return null;
  }
  return doc;
}

async function runCli(command) {
  const doc = activeRose();
  if (!doc) {
    return;
  }
  const bin = rgBin();
  const folder = vscode.workspace.getWorkspaceFolder(doc.uri);
  const cwd = folder ? folder.uri.fsPath : path.dirname(doc.uri.fsPath);
  const channel = output;
  channel.show(true);
  channel.appendLine("");
  channel.appendLine("> " + command + " " + doc.uri.fsPath);
  await new Promise((resolve) => {
    const child = cp.spawn(bin, [command, doc.uri.fsPath], {
      cwd,
      windowsHide: true,
    });
    const write = (chunk) => channel.append(String(chunk));
    child.stdout.on("data", write);
    child.stderr.on("data", write);
    child.on("error", (err) => {
      channel.appendLine(String(err));
      resolve();
    });
    child.on("exit", (code) => {
      channel.appendLine("exit " + code);
      resolve();
    });
  });
}

module.exports = { activate, deactivate };
