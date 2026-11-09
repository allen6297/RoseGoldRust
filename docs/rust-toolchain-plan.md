# RoseGold in Rust

The language to port is the working tree at `C:\CODE\RoseGoldJava`. That checkout is ahead of any published copy and includes uncommitted compiler, IDE, and Godot work. Ignore `allen6297/RoseGold` and `allen6297/rosegoldcpp`.

The Rust toolchain will live in a new sibling folder, `C:\CODE\RoseGoldRust`. This document is the plan. No crate, `Cargo.toml`, or git commit exists yet. The game that will load the toolchain is TerraLite at `C:\CODE\TerraLite`.

## What the language is

RoseGold is a small statically checked language in `.rg` files. A program is a list of imports, functions, types, and nested `mod` blocks. `fn main(): Int` is the program entry. `main` returns an `Int` that the CLI uses as the process exit code. `print` writes one value and a newline.

Comments are `//` and `#` to end of line, and `/#` … `#/` blocks. Strings are double-quoted with escapes. `f"...{expr}..."` is an interpolated string; `{{` and `}}` are literal braces. Numbers are `Int` (checked 64-bit) and `Float` (64-bit). `true`, `false`, and `null` (`Void`) are literals.

Types the checker knows by name: `Int`, `Float`, `String` (the alias `Str` is accepted), `Bool`, `Void`, `Array[T]`, `Map`, `Range`, `Future[T]`, plus every `struct`, `class`, `data`, `enum`, and `trait` the program defines. Generics use brackets: `fn identity[T](x: T): T`, `struct Box[T]`, `class Slot[T] extends SlotParent[T]`. A type parameter can carry trait bounds. The checker infers many type arguments at the call.

Declarations:

- `fn`, including `async fn`, `throws`, default arguments, and a trailing `...` vararg.
- `struct` and immutable `data`, with fields, methods, and nested `impl Trait`.
- `class` with `extends`, `abstract`, `final`, `static`, and `super`.
- `trait` and `impl Trait for Type`. A trait name used as a type is a trait object (`var p: Named = Person { n: "ada" }`).
- `enum` with unit variants and payloads (`Circle(Int)`, `Rect(width: Int, height: Int)`).
- `signal` at the top level or on a type. Listeners use `connect`; firing uses `emit` and `emit_deferred`.
- `const` and `@constexpr` functions. A constexpr call can initialize a `const`. The body is restricted: the fail suite rejects `print`, `var`, lambdas, and calls that are not themselves constexpr.
- `mod` blocks, `import a.b`, and `from a import name as alias`.
- Visibility is `pub`, `private`, or `protected`. Combining two of them is an error. Omitting one means `pub`.

Statements and expressions the parser builds: `var` and `const`, assignment and compound assignment (`+=` through `>>=`), field and index assignment, `if` / `elif` / `else` (statement and expression), `while`, `for x in` arrays and ranges, `match` and `switch`, `break`, `continue`, `pass`, `return`, `throw`, and `do` / `catch`. `try` is an expression that propagates a thrown string. Ranges are `a..b` and `a..=b`. Operators cover arithmetic, comparison, `&&` and `||`, and bitwise `~ & | ^ << >>`. Lambdas are `fn (x: T): R { ... }` and capture locals. `is` tests a type. `$Node` is a Godot node path and lowers to `get_node`.

Attributes the parser accepts, and rejects everywhere else: `@test`, `@deprecated`, `@constexpr`, `@ufcs`, `@optional`, `@onready`. `@ufcs` makes a function callable as a method on its first parameter (`(3).doubled()`). `@optional` marks a field that a literal may omit. `@onready` and `export var` exist for the Godot inspector (range, enum, flags, file, dir, multiline hints live on the field).

A project file is `project.toml`:

```toml
[project]
name = "RoseGold Examples"
author = "Kalob"
version = "1.0.0"
rosegold = "1.0.0"
entry = "hello.rg"

[modules]
calc = "calc"

[profile.test]
entry = "tests.rg"
```

`[profile.debug]` and `[profile.release]` are accepted and must be empty. The only profile key the loader reads is `[profile.test] entry`. Unknown tables and unknown keys are errors. `rg new` writes `project.toml`, `main.rg`, and `tests.rg`. `rg new module <name>` adds `<name>/lib.rg` and a `[modules]` line.

## Pipeline

One pipeline, three ways to run a function that has already typechecked.

1. **Lex.** `Lexer` produces tokens with line and column. Keywords are the list in `Tok` (`fn`, `mod`, `class`, `trait`, `enum`, `struct`, `data`, `impl`, `signal`, `async`, `await`, `match`, `switch`, and the rest). There is no preprocessor.
2. **Parse.** `Parser` builds the `Program` tree in `Program.java`: `FnDecl`, `StructDecl`, `ClassDecl`, `TraitDecl`, `EnumDecl`, `ImplDecl`, `SignalDecl`, `ConstDecl`, `ImportDecl`, `ModDecl`, `Stmt`, `Expr`, `MatchArm`. Parse errors are `Diagnostic`s. A hard failure throws `LangException`.
3. **Load and check.** `Checker` resolves imports, flattens class fields through `extends`, and records modules. `TypeChecker` then types the tree. `UnusedImports` adds warnings. `Check.checkSource` is the single entry the CLI, the IDE service, and the Godot editor call.
4. **Lower, when it can.** `Compile` turns each function into register bytecode (`Bytecode.Op`: loads, `BIN` / `UNARY`, jumps, `CALL`, `METHOD`, `STRUCT`, `MATCH_LIT`, `AWAIT`, `THROW`, …). If a construct has no opcode path, `Compile` leaves `fn.code == null` and `ir` prints `fn name { tree-walk }`.
5. **Execute.** `Interp.callUser` uses `Vm` when `fn.code` is present and walks the AST otherwise. The walk is the complete semantics. The VM is a fast path for the functions `Compile` accepts.
6. **Native, optional.** `Llvm.emit` writes LLVM IR only for functions that have bytecode. `LlvmLink` invokes clang (LLVM 15+) and links `native/runtime.c` plus the host C files. `llvm --run` caches the exe under `%LOCALAPPDATA%\RoseGold\run`. A file that did not lower falls back to the interpreter during `test --native`.

Module lookup, in order: the `godot` crate when a script asks for it, then `builtin/std` (a project-local `builtin/std` wins over the copy packed in the JAR), then a `[modules]` path, then `<name>/` or a sibling file that contains `mod name`, searched from the importing file and from the entry file. Dotted names map to directories. A module directory is every `.rg` file in it. Cycles are errors. `from` imports only two path segments.

Diagnostics carry file, line, column, severity, and message. Fail tests pin the message with a first-line comment:

```rg
# expect: cannot add
```

`// expect:` and `/// expect:` work the same way. Pass files are programs whose `main` returns 0.

## Runtime

Values in the interpreter (`Value.Kind`) and in the C runtime (`RG_*`) are the same set: void, bool, int, float, string, array, range, map, struct, enum type, enum value, function (including a closure), future, signal. The C runtime adds `RG_OBJECT`, an integer id the Godot host maps back to an engine object through slot callbacks.

`Int` arithmetic uses checked `i64`. `9223372036854775807 + 1` throws `integer overflow`. Division or remainder by zero throws. Float math is unchecked `f64`. Adding an `Int` and a `Float` promotes to `Float`. Map keys are strings; a non-string key is turned into a string or rejected. Arrays are growable: `push`, `pop`, `len`, index get and set. Maps have `len`, `has`, `keys`, `insert`, `remove`. `String.len` is Java's UTF-16 code-unit length. `Future` has `cancel`, and the constructors `Future.all` and `Future.race`. Signals store listener closures and can defer an emit until the current emit finishes.

Thrown values are strings. `do` / `catch` handles them. An uncaught throw fails the program. `@deprecated` prints a warning on call. `assert` fails the process. `print`, `len`, `assert`, `argv`, and `argv_len` are built in and are not captured by closures.

Async is cooperative. `await` parks a `Future`. `__time.delay` schedules a timer; `__time.sleep` blocks. The interpreter pumps timers, deferred signal emits, and UI frames on one thread. There is no OS thread per `async fn`.

Host calls go through `rg_host_call(module, name, args)` on the native side and the matching `call*` methods in `Interp`. The modules are `checks`, `process`, `__math`, `__str`, `__io`, `__uuid`, `__time`, `__path`, `__json`, `__regex`, and `__ui`. File IO is sandboxed to the process working directory. A path that escapes with `..` throws `path outside sandbox`.

## Standard library

`builtin/std` is RoseGold source plus those host modules. Children: `math`, `str`, `io`, `vec`, `time`, `path`, `json`, `regex`, `ui`. The root crate `std` defines `UUID` (`nil`, `v4`, `valid`, `parse`) on top of `__uuid`.

| Crate | Role |
| --- | --- |
| `std.math` | `abs`, `sign`, `min`, `max`, `clamp`, `gcd`, `pow`, `rand_int`, `sin`, `cos`, `atan2`, `sqrt`, `powf`, `floor`, `ceil`, `lerp`, `move_toward`, `to_int`, `to_float`, `random` |
| `std.str` | `contains`, `starts_with`, `ends_with`, `length`, `is_empty`, `repeat`, `upper`, `lower`, `trim`, `slice`, `split`, `replace`, `find` |
| `std.io` | `exists`, `remove`, `read_text`, `read_lines`, `write_text`, all sandboxed, the throwing ones marked `throws` |
| `std.path` | `join`, `parent`, `stem` |
| `std.json` | `parse`, `stringify`, `valid` |
| `std.regex` | `valid`, `is_match`, `find`, `find_match`, `captures`, `findall`, `replace`, `split` |
| `std.time` | `now` (milliseconds), `sleep`, `async delay` |
| `std.vec` | `Vec2` and `Vec3 extends Vec2` with `length` and `add` |
| `std.ui` | A desktop widget tree. `Window`, `Dialog`, `Theme`, stacks, buttons, fields, and the rest call `__ui` |

`std.ui` is about 130 KB of RoseGold. The interpreter implements `__ui` in Swing (`HostUi.java`, plus font and a small SVG subset). The native binary implements it in Win32 (`native/host_ui.c`, images in `host_img.c`). `RG_UI_HEADLESS=1` makes `Window.run` return without a message pump. Linux and macOS native windows are still open work in `TASKS.md`.

`builtin/godot/lib.rg` is generated by `GodotApi` from Godot's `extension_api.json`. It is about 1.4 MB and 29,000 lines of class stubs. It is not hand-written library code.

## Tools that exist today

The CLI is `com.rosegoldc.lang.Main`. Commands: `check`, `run`, `test`, `fmt`, `new`, `ide`, `ir`, `llvm`, `version`, `help`. A bare `./gradlew rg` opens a REPL. `check --json` prints diagnostics. `fmt` can `--check`, `--write`, `--compact`, and `--no-comments`. `test` runs `@test` functions, a `project.toml` test entry, or the `tests/pass` and `tests/fail` directories. `ir` dumps bytecode. `llvm` prints IR, `--link` / `--run` build a native exe, `--test` builds a `@test` harness, and `--godot` links a script shared library plus a sidecar manifest.

`ide` is a line-delimited JSON service on stdin. Methods: `ping`, `check`, `hover`, `complete`, `goto`, `signature`, `usages`, `rename`, `format`. The VS Code extension in `vscode/` is a client of that service, plus a TextMate grammar. It needs Java 21. The IntelliJ plugin (`src/main/Java/com/rosegoldc/idea`, 87 files) is a second client: diagnostics, completion, hover, go to definition, usages, rename, format, inlays, signature help, color chips, quick fixes, structure view, and Run / Test / Debug. Native debug is `lldb-dap` with DWARF. Unchecking "Run natively" uses the in-process interpreter.

The Godot 4.4 host is `godot/`. `godot/src/rosegold_language.c` is the GDExtension (about 300 KB). A script `class Player extends Node2D` is the script class. `export var` shows in the inspector. `struct` and `data` become Godot resources. Calls such as `queue_free()` and `get_node()` go through the C runtime's Godot callbacks. The demo is `godot/demo`.

The conformance suite on disk today: 109 `.rg` files under `tests/pass`, 153 under `tests/fail`, 26 examples, and 27 JUnit classes that drive the same pipeline. `tests/pass` includes the stdlib and the UI widget suite. `tests/fail` pins parse errors, type errors, visibility, generics, overflow, sandbox escapes, and runtime failures.

## What the game does today

TerraLite already loads `.rg` files, but it does not run this language. `C:\CODE\TerraLite\crates\world\src\rosegold.rs` is a few hundred lines that accept only two forms:

```rg
before-break "keep-stone" {
    cancel "stone"
}

after-place "log-place" {
    log
}
```

Those files are `C:\CODE\TerraLite\mods\keep-stone.rg` and `log-place.rg`. The server also loads every `*.rg` in `C:\CODE\TerraLite\mods`, in path order. A missing folder loads nothing. A bad script stops the server before it listens. `before-break` can cancel a break. `after-place` runs only after a place changes a cell. The words `before-break`, `cancel`, and `log` are not RoseGold syntax. The Java parser rejects them.

The mod toolchain is the Java language, embedded in the server, with the game registered as a host. The stub loader comes out when that host can express the same two hooks.

## What the toolchain keeps

These are the parts a mod needs, and the parts the suite locks down.

- The syntax and type rules above, so existing `.rg` programs and the pass/fail suite still mean the same thing. Diagnostic text in `# expect:` comments is part of that contract. Checked `i64`, string-keyed maps, and `String.len` in UTF-16 code units are part of it too.
- `project.toml`, `[modules]`, and `mod` / `import` / `from`, so a mod is a folder of sources with an entry file.
- One embeddable interpreter that walks the AST. That walk is the semantic source. Bytecode and LLVM are optimizations on top of it, and both already give up and return to the walk.
- A host trait the game implements: call a RoseGold function by name, and let RoseGold call back into Rust (`before_break`, `after_place`, `print`, and later the `__math` / `__io` family). Values that cross the boundary are the runtime kinds above. A thrown string becomes a failed hook, and a failed load still stops the server.
- Load order: every `*.rg` in a mods directory, sorted by path. An empty or missing directory loads nothing.
- The standard library modules a mod will actually import: `math`, `str`, `vec`, `json`, `path`, `time`, `io`, `regex`, and `UUID`. IO stays sandboxed. The sandbox root is the process working directory in the Java tool. The game host passes its own root (the mod folder or the save) when it embeds the interpreter.
- `check`, `run`, `test`, and `fmt` as a `rg` binary, so mods can be typechecked and formatted without IntelliJ.
- The pass/fail suite read from `C:\CODE\RoseGoldJava\tests`. The Rust tests point at that tree so the suite is not forked.

The first game-facing hooks, written in real RoseGold once the host exists:

```rg
fn allow(block: String): Bool {
    return block != "stone";
}

fn placed(id: String, x: Int, y: Int, z: Int) {
    print(id);
}

fn main(): Int {
    hooks.before_break("keep-stone", allow);
    hooks.after_place("log-place", placed);
    return 0;
}
```

`allow` returning false cancels the break. `placed` runs only after the cell changes. Names here are the shape, and the question at the bottom is whether this replaces the stub files.

## What stays in the Java tree for now

The IntelliJ plugin, the Win32 and Swing UI, and the Godot GDExtension stay where they are. `std.ui` and `builtin/godot/lib.rg` are not copied into the first Rust tree. The line JSON `ide` service is a thin layer over check, hover, complete, goto, rename, and format, and it follows after `fmt` and the checker are faithful. A VS Code client can then talk to the Rust binary the same way it talks to `java … Main ide` today.

Bytecode and LLVM wait until the tree-walk interpreter matches the suite. Porting `Compile.java`, `Llvm.java`, and `native/runtime.c` before that would freeze the places where the Java backends already fall back to the walk. A native backend is worth doing later if a mod has to run without the interpreter inside the server. It is not required to load `keep-stone`.

## Crate layout

Workspace root: `C:\CODE\RoseGoldRust`. Create this when the port starts. Nothing under `crates/` exists yet.

```text
C:\CODE\RoseGoldRust\
  Cargo.toml                  workspace
  docs\
    rust-toolchain-plan.md
    rosegoldjava-layout.md
  crates\
    rosegold-syntax\          lexer, tokens, parser, AST, spans
    rosegold-check\           module loader, types, diagnostics
    rosegold-eval\            values, tree-walk interpreter, host trait
    rosegold-std\             std sources (no ui) and Rust host impls
    rosegold-cli\             rg check, run, test, fmt, new
    rosegold-ide\             line JSON service, after the CLI is faithful
  tests\
    conformance\              reads C:\CODE\RoseGoldJava\tests
```

`rosegold-syntax` depends on nothing in the workspace. `rosegold-check` depends on syntax. `rosegold-eval` depends on check and calls a `Host` trait defined in eval. `rosegold-std` implements that trait for math, strings, json, io, and the rest. `rosegold-cli` is the binary. TerraLite later takes a path dependency on `rosegold-eval` and `rosegold-std`, implements the two hook callbacks, and deletes `crates/world/src/rosegold.rs`. That TerraLite edit is a separate change after `rg run` passes the suite.

Public surface the game should see:

```rust
trait Host {
    fn call(&mut self, module: &str, name: &str, args: &[Value]) -> Result<Value, Error>;
}

fn load_source(source: &str, path: &str, host: &mut dyn Host) -> Result<Module, Error>;
fn call(module: &Module, name: &str, args: &[Value], host: &mut dyn Host) -> Result<Value, Error>;
```

`load_dir` walks `*.rg` in path order and aggregates the errors the server already treats as fatal.

## Implementation order

Each step is done when the named slice of the Java suite matches. Run the Java interpreter on the same file when a result looks wrong. The Java tree stays the oracle.

1. **Syntax.** Lexer and parser. Tokens, spans, and the `Program` tree. Parse every file in `tests/pass`, `tests/fail`, and `examples`. Parse errors match the fail files whose `# expect:` text is a parse error.
2. **Check.** Module loader, `project.toml`, inheritance, generics, traits, visibility, constexpr. The rest of `tests/fail` matches, including line and column.
3. **Eval.** Tree-walk interpreter for functions, control flow, structs, classes, enums, arrays, maps, closures, try/throw, and signals. `tests/pass` programs that do not import `std.ui` run and match stdout and the exit code. Overflow, `div_zero`, and empty `pop` match too.
4. **Host and std.** Rust implementations of the `__*` modules and the `.rg` sources for every std child except `ui`. `tests/pass/stdlib_*.rg` passes, skipping `stdlib_ui*`.
5. **CLI.** `rg check`, `rg run`, `rg test`, `rg fmt`, `rg new`. `fmt --check` matches `Format.java` on the suite. `test` understands `@test`, pass directories, fail directories, and `[profile.test]`.
6. **Game host.** A TerraLite dependency that loads a mods directory, registers `hooks.before_break` and `hooks.after_place`, and reproduces keep-stone and log-place. The two stub files are rewritten in RoseGold in that change. The server still refuses to listen when a script fails to load.
7. **IDE service.** The `ide` line protocol, then a VS Code client pointed at the Rust binary.
8. **Later, only if a mod needs them.** Bytecode VM, a native backend, `std.ui`, the Godot stub generator and GDExtension.

## Questions that block the start

Two decisions change the first crate. Everything else above is sequenced so work can start without it.

1. **The stub dialect.** The plan treats `before-break` / `cancel` / `log` as TerraLite's temporary loader, and rewrites `keep-stone.rg` and `log-place.rg` in real RoseGold when the host exists. If those two files have to keep parsing unchanged, the grammar grows a second language and the Java suite is no longer the whole spec. Say so before the parser is written.
2. **How wide the first milestone is.** The plan's acceptance test is the Java suite: 109 pass files and 153 fail files, with `std.ui` and Godot deferred. A narrower first milestone (functions, strings, bools, structs, and a host call) would let TerraLite drop `rosegold.rs` sooner and would leave types, generics, classes, and async for a second pass. Say so if the game hook is more urgent than suite parity.

Location is settled: `C:\CODE\RoseGoldRust`, next to `C:\CODE\RoseGoldJava`. The interpreter is the first runtime. Godot and desktop UI stay in the Java tree.

File map for the Java tree, for whoever starts step 1: `C:\CODE\RoseGoldRust\docs\rosegoldjava-layout.md`.
