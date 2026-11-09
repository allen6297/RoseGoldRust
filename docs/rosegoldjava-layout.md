---
cursor:
  subagentId: "bc-f525de17-ba6c-5dd5-9cd9-1e4ab0642e57"
---

# RoseGoldJava layout

Inventory of `C:\CODE\RoseGoldJava` for the Rust port. Read this tree, not `allen6297/RoseGold` or `allen6297/rosegoldcpp`. The working tree has uncommitted compiler, IDE, Godot, and VS Code work. That working tree is the source of truth.

The port plan Kalob reads is `C:\CODE\RoseGoldRust\docs\rust-toolchain-plan.md`. No Rust crate exists yet. The game is `C:\CODE\TerraLite`. Its current loader, `C:\CODE\TerraLite\crates\world\src\rosegold.rs`, is a stub dialect (`before-break`, `after-place`, `cancel`, `log`), not this language.

Counts from the working tree: 51 Java files in `com.rosegoldc.lang`, 87 in `com.rosegoldc.idea`, 27 JUnit classes, 109 `tests/pass` `.rg` files, 153 `tests/fail` `.rg` files, 26 `examples` `.rg` files.

## Tree

```text
C:\CODE\RoseGoldJava\
  src\main\Java\com\rosegoldc\lang\     language, CLI, hosts, IDE service
  src\main\Java\com\rosegoldc\idea\     IntelliJ plugin (client of lang)
  src\test\java\com\rosegoldc\lang\     JUnit
  builtin\std\                          std .rg sources
  builtin\godot\lib.rg                  generated Godot stubs, ~1.4 MB
  native\                               C runtime linked by llvm --link
  godot\                                GDExtension, demo, extension header
  examples\                             samples, including project.toml
  tests\pass\  tests\fail\              conformance suite
  vscode\                               TextMate grammar and extension manifest
  README.md  TASKS.md  build.gradle.kts
```

`TASKS.md` says the C++ tree is out of scope, JVM bytecode is not the backend, and `[profile.debug]` / `[profile.release]` must not grow keys. Those two tables are legal only when empty. `[profile.test] entry` is the only profile key `Project` reads.

Build and CLI, from the Java repo root, Java 21:

```text
./gradlew test
./gradlew rg --args="check examples/hello.rg"
./gradlew rg --args="run examples/hello.rg"
./gradlew rg --args="test"
./gradlew rg --args="fmt --check examples"
./gradlew rg --args="ir examples/hello.rg"
./gradlew rg --args="ide"
```

`Main` aliases accept the first letter and `--letter`. No args starts a `rosegold ` REPL. `quit` / `exit` leave it.

## Pipeline entry points

| Step | Type | Entry |
| --- | --- | --- |
| Lex | `Lexer` | `Lexer.tokenize(source, path, diags)` |
| Parse | `Parser` | `Parser.parseSource(source, path, diags)` returns `Program` |
| Check | `Check` | `Check.checkSource` / `checkFile` / `checkDirectory` |
| Names and modules | `Checker` | `Checker.check(program, path, diags, godotProject)` |
| Types | `TypeChecker` | called from `Checker` |
| Unused imports | `UnusedImports` | `UnusedImports.check` |
| Bytecode | `Compile` | `Compile.attach(checker)`; `Compile.dumpEntry` |
| VM | `Vm` | `Vm.exec(interp, fn.code, args, caps)` |
| Tree walk | `Interp` | `Interp.callUser` when `fn.code == null` |
| Run a file | `Run` | `Run.runFile` / `Run.runSource` |
| LLVM IR | `Llvm` | `Llvm.emit(checker, test, godot)` |
| Link | `LlvmLink` | clang, cache under `%LOCALAPPDATA%\RoseGold\run` |
| Project | `Project` | `Project.load`, `Project.locate` |
| Scaffold | `Scaffold` | `Scaffold.create`, `Scaffold.addModule` |
| Tests | `Tests`, `Run` | `@test` discovery; pass/fail dirs; `# expect:` |
| Format | `Format` | `Format.format` |
| IDE | `Ide` | `Ide.serve` line JSON. CLI command `ide` |
| Godot model | `GodotScript` | one leaf class that extends an engine base |
| Godot stubs | `GodotApi` | `extension_api.json` to `lib.rg` |

`Check.godotProject` walks up to 8 parents looking for `project.godot`. A hit loads `builtin/godot` the same way `llvm --godot` does.

`Interp.callUser` (`Interp.java` around the `fn.code` branch): if bytecode exists, `Vm.exec`, otherwise `execBlock` on the AST. A `Skip` inside `Compile.compileOne` sets `fn.code = null`. `ir` and LLVM then print `fn name { tree-walk }`. Native tests that did not lower call the interpreter (`Run.runMaybeNative`).

## Language surface

Keywords in `Lexer.keyword` / `Tok`: `fn`, `import`, `from`, `as`, `is`, `pub`, `abstract`, `final`, `static`, `private`, `protected`, `mod`, `class`, `trait`, `extends`, `for`, `in`, `super`, `enum`, `match`, `switch`, `var`, `const`, `export`, `struct`, `data`, `impl`, `signal`, `return`, `pass`, `break`, `continue`, `if`, `elif`, `else`, `while`, `try`, `do`, `throws`, `throw`, `catch`, `async`, `await`, `true`, `false`, `null`.

Comments: `//` line, `#` line (`hashLineComment`), `/#` block closed by `#/`.

Attributes in `Parser.parseOneAttr`: `@test`, `@deprecated`, `@constexpr`, `@ufcs`, `@optional`, `@onready`. Anything else is `unknown attribute`. Placement rules reject `@test` and `@ufcs` on methods, `@onready` off a class field, and `abstract` / `final` / `protected` on items that cannot take them.

AST root is `Program` (`Program.java`). Item kinds: import, fn, struct, class, trait, enum, impl, signal, mod, singleton, const. `Expr.Kind`: Int, Float, String, Bool, Var, Unary, Binary, Call, MethodCall, Member, Index, Array, Map, StructLit, Range, Try, Lambda, Await, Cast, Is, If, Format, New, NodePath, Null. `Stmt.Kind`: Expr, Var, Const, Assign, FieldAssign, IndexAssign, Return, If, While, For, Match, Pass, Break, Continue, Throw, Do, Comment. Match patterns: Wildcard, Int, Float, String, Bool, Variant.

Built-in type heads in `TypeChecker`: `Int`, `Float`, `String`, `Str`, `Bool`, `Void`, `Array`, `Map`, `Range`, `None`, `Self`, `Future`. Written form of an application is `Name[Arg, Arg]`. `Types.substType` substitutes through nested brackets.

Runtime values (`Value.Kind` and `native/rg_value.h`): Void, Bool, Int, Float, String, Array, Range, Map, Struct, EnumType, Enum, Fn, Future, Signal. C adds `RG_OBJECT` (Godot object id plus slot callbacks). `Int` ops use `Math.addExact` / `multiplyExact`. Overflow message is `integer overflow`. Map keys are strings. `String.len` uses Java `String.length()` (UTF-16 code units).

Builtins that `Interp.skipCaptureName` refuses to close over: `print`, `len`, `assert`, `argv`, `argv_len`, `super`.

Array methods: `len`, `push`, `pop`. Map methods: `len`, `has`, `keys`, `remove`, `insert`. Future method: `cancel`. Future constructors: `all`, `race`. Signal methods: `connect`, `emit`, `emit_deferred`.

Host modules (`Types.isHostModule`): `checks`, `process`, `__math`, `__str`, `__io`, `__uuid`, `__time`, `__path`, `__json`, `__regex`, `__ui`. Dispatch is `Interp` `call*` methods and `rg_host_call` in `native/runtime.c`.

`__math` names: `pow`, `rand_int`, `random`, `to_float`, `sin`, `cos`, `sqrt`, `floor`, `ceil`, `to_int`, `atan2`, `powf`.

`__io` names: `exists`, `remove`, `read_text`, `read_lines`, `write_text`. Sandbox root is the process cwd (`Interp.sandboxRoot`, `rg_sandbox_root`). Escape message: `path outside sandbox`.

`__time`: `now` (ms), `sleep` (blocks), `delay` (future + timer).

`__path`: `join`, `parent`, `stem`.

`__json`: `parse`, `valid`, `stringify`.

`__regex`: `valid`, `is_match`, `find`, `find_match`, `captures`, `findall`, `replace`, `split`.

Std crates (`Stdlib.CHILDREN`): `math`, `str`, `io`, `vec`, `time`, `path`, `json`, `regex`, `ui`. Root file `builtin/std/lib.rg` is `UUID`. A project-local `builtin/std` wins over the JAR copy. `Stdlib.findStdlibRoot` walks from the file.

Module resolve (`Checker.resolveModule`): `godot` stub dir, then std crate dir, then `[modules]` path, then `<stem>/` and a file in the same directory whose source contains `mod stem`. Bases are the importing file's parent and the entry file's parent. `from` imports allow two segments (`bindFromImport`). Longer `from` paths error with `nested from-imports longer than 2 segments are not supported`. Cycles: `cyclic import of module`.

`project.toml` keys (`Project.java`): top-level `project`, `modules`, `profile`. Project keys: `name`, `author`, `version`, `rosegold`, `entry`. Profile names: `debug`, `release`, `test`. Only `test` may contain `entry`.

## `com.rosegoldc.lang` by size

Largest files are the ones a port will otherwise re-read by accident.

| File | Bytes | Role |
| --- | --- | --- |
| `Interp.java` | 131190 | Tree-walk evaluator, host calls, async pump, signals, sandbox |
| `TypeChecker.java` | 125217 | Types, generics, traits, visibility, constexpr |
| `Llvm.java` | 86213 | LLVM IR from bytecode, including Godot forwarders |
| `Parser.java` | 85521 | AST builder |
| `Checker.java` | 79437 | Import graph, inheritance flatten, symbol tables |
| `Compile.java` | 69027 | AST to bytecode, or `Skip` back to the walk |
| `HostUi.java` | 58690 | Swing `__ui` |
| `Format.java` | 45583 | Formatter |
| `GodotApi.java` | 39652 | `extension_api.json` to stubs |
| `Fixes.java` | 37115 | Quick-fix rewrites |
| `HostSvg.java` | 36942 | Small SVG subset (`rect`, `circle`) |
| `Completions.java` | 32332 | Completion items |
| `LlvmLink.java` | 27694 | clang link and exe cache |
| `Goto.java` | 26539 | Go to definition |
| `Hover.java` | 25704 | Hover |
| `Lexer.java` | 20325 | Tokens |
| `Run.java` | 17979 | File and suite execution, `# expect:` |
| `NativeDebug.java` | 16947 | lldb-dap |
| `Ide.java` | 15448 | Line JSON service |
| `Inlays.java` | 15527 | Inlay hints |
| `GodotScript.java` | 15102 | Script class, exports, manifest model |
| `Signatures.java` | 13908 | Signature help |
| `Project.java` | 13547 | `project.toml` |
| `Json.java` | 13388 | JSON used by hosts and the IDE |
| `Value.java` | 12699 | Interpreter values and print |
| `Bytecode.java` | 6272 | Opcode enum and dump |
| `Vm.java` | 10459 | Register VM |
| `Toml.java` | 9106 | TOML subset for `project.toml` |
| `DapJson.java` / `DapClient.java` | ~8 KB each | DAP client for native debug |
| `Scaffold.java` | 7029 | `new` and `new module` |
| `Colors.java` | 7859 | Color literals for the IDE |
| `Usages.java` | 7812 | Find usages |
| `HostFont.java` | 7551 | Font metrics for UI |
| `Stdlib.java` | 21010 | Crate index and JAR unpack |
| `Debug.java` | 10167 | Interpreter breakpoints |
| `NativeRun.java` | 4251 | Spawn a linked exe |
| `Check.java` | 3585 | Public check API |
| `Tests.java` | 4187 | `@test` scan and `ok` / `FAIL` lines |
| `Program.java` | 9261 | AST types |
| `Types.java` | 5283 | Type-string helpers |
| `Rename.java` | 3910 | Rename edits |
| `Idents.java` | 3579 | Identifier rules |
| `Diagnostic.java` | 1325 | file, line, col, severity, message |
| `Tok.java` | 1190 | Token enum |
| `Token.java` | 577 | One token |
| `SourcePos.java` | 1939 | Line/col to offset |
| `LangException.java` | 267 | Parse/check abort |
| `DebugEngine.java` | 481 | Debug backend switch |
| `Main.java` | 24455 | CLI |

`Ide.handle` methods: `ping`, `check`, `hover`, `complete`, `goto`, `signature`, `usages`, `rename`, `format`. One JSON object per line. Request fields used by those methods include `id`, `method`, `source`, `path`, `offset`.

`Main` commands: `check`, `run`, `test`, `fmt`, `new`, `ide`, `ir`, `llvm`, `version`, `help`, `quit`.

Bytecode ops (`Bytecode.Op`): `LOAD_CONST`, `LOAD_FN`, `LOAD_ENUM`, `LOAD_SIGNAL`, `LOAD_SINGLETON`, `MOVE`, `BIN`, `UNARY`, `TRUTHY`, `JUMP`, `JUMP_F`, `JUMP_T`, `CALL`, `CALL_VAL`, `ARRAY`, `RANGE`, `INDEX_GET`, `INDEX_SET`, `ITER_ITEMS`, `LEN`, `MAP`, `METHOD`, `STATIC`, `SUPER`, `STRUCT`, `MEMBER`, `FIELD_SET`, `MATCH_LIT`, `MATCH_VAR`, `PAYLOAD`, `THROW`, `CLOSURE`, `AWAIT`, `RET`, `RET_VOID`, `DEBUG`.

`Compile` throws `Skip` (and leaves the function on the tree walk) for gaps that include: match arms whose bindings escape, some await-in-loop shapes, missing slots, const assignment, `if` expressions with too few children, module-qualified calls it cannot see, lambdas whose captures are not in slots, Godot `is` against a non-native base, struct literals with unknown or missing fields, and a null expression. Treat that list as incomplete. The rule is: `fn.code == null` means the walk is authoritative.

## Standard library files

```text
C:\CODE\RoseGoldJava\builtin\std\lib.rg            UUID
C:\CODE\RoseGoldJava\builtin\std\math\lib.rg
C:\CODE\RoseGoldJava\builtin\std\str\lib.rg
C:\CODE\RoseGoldJava\builtin\std\io\lib.rg
C:\CODE\RoseGoldJava\builtin\std\path\lib.rg
C:\CODE\RoseGoldJava\builtin\std\json\lib.rg
C:\CODE\RoseGoldJava\builtin\std\regex\lib.rg
C:\CODE\RoseGoldJava\builtin\std\time\lib.rg
C:\CODE\RoseGoldJava\builtin\std\vec\lib.rg       Vec2, Vec3
C:\CODE\RoseGoldJava\builtin\std\ui\lib.rg        Widget, Window, Dialog (~43 KB)
C:\CODE\RoseGoldJava\builtin\std\ui\widgets.rg    stacks, controls (~83 KB)
C:\CODE\RoseGoldJava\builtin\std\ui\color.rg      Color enum, Theme
C:\CODE\RoseGoldJava\builtin\godot\lib.rg         generated, do not hand-edit
```

UI types worth knowing if a later pass ports them: traits `Widget` and `LazyRows`; classes `Style`, `Pad`, `Backdrop`, `Window`, `Dialog`, `PopupMenu`; widget file classes `VStack`, `HStack`, `Spacer`, `Label`, `Button`, `TextField`, `Toggle`, `Checkbox`, `RadioGroup`, `ProgressBar`, `ScrollView`, `Slider`, `Dropdown`, `LazyColumn`, `ImageView`. Entry points: `ui.open`, `ui.open_hidden`, `ui.confirm`, `ui.alert`. Headless env: `RG_UI_HEADLESS=1`.

## Native and Godot

```text
C:\CODE\RoseGoldJava\native\rg_value.h       RGValue ABI and rg_* exports
C:\CODE\RoseGoldJava\native\runtime.c        values, calls, async, signals, rg_host_call (~118 KB)
C:\CODE\RoseGoldJava\native\host_rest.c      io, json, regex, and the rest of the non-UI hosts (~44 KB)
C:\CODE\RoseGoldJava\native\host_ui.c        Win32 __ui (~43 KB)
C:\CODE\RoseGoldJava\native\host_img.c       WIC images, small SVG
C:\CODE\RoseGoldJava\native\host_godot.c     Godot callback glue
C:\CODE\RoseGoldJava\godot\src\rosegold_language.c    GDExtension (~306 KB)
C:\CODE\RoseGoldJava\godot\include\gdextension_interface.h
C:\CODE\RoseGoldJava\godot\demo\             Godot 4.4 project, player.rg, face.rg
```

`rg_value.h` exports the embed surface the LLVM backend uses: setters, `rg_bin`, `rg_call_val`, `rg_host_call`, `rg_throw` / `rg_catch`, `rg_spawn` / `rg_await`, signal register, and `rg_set_godot_*` callbacks. A Rust interpreter does not need this ABI. A later native backend either keeps it or replaces it.

Godot script rules (`GodotScript`, README): one leaf class whose base resolves to an engine class; `export var` and the field hints (`hasRange`, `hasEnum`, `hasFlags`, `hasFile`, `hasDir`, `hasMultiline`); `struct` / `data` as resources; `Array` and `Map` pass through engine calls; packed arrays become typed `Array`s; `RID` is `Int`; `Callable` is a function; singletons (`Input`, `Time`, `Engine`) are values. `$path` is `Expr.Kind.NodePath` and compiles to `get_node` when `self` exists.

## IDE plugin and VS Code

`src\main\Java\com\rosegoldc\idea\` groups: `editor` (highlight, format, fold, braces, structure, postfix, move statement), `insight` (annotate, complete, hover docs, goto, inlays, signature, quick fix), `psi` (plugin lexer and parser, separate from `lang.Lexer`), `refactor` (rename, usages), `run` (run and test configurations), `debug` (breakpoints, interpreter and native). Plugin entry resources are under `src\main\resources` (`META-INF`, live templates, file templates). Run and Debug default to native LLVM. The plugin is a client. Port `lang` first. The PSI lexer is a second lexer and must track `lang.Lexer`, not the other way around.

`C:\CODE\RoseGoldJava\vscode\` contributes the `rosegold` language id for `.rg`, a TextMate grammar, snippets, and commands `rosegold.restart`, `rosegold.runFile`, `rosegold.testFile`. Settings: `rosegold.java`, `rosegold.classpath`, `rosegold.format.compact`, `rosegold.format.comments`. The extension's language features call `Main` `ide`. Grammar-only highlighting works without Java.

## Tests and examples

Fail files start with `# expect: <substring of the diagnostic or runtime error>`. Prefixes also accepted: `// expect:`, `/// expect:`. Reader: `Run.readExpect`. Pass files run `main` and expect success (exit 0). Directory layout includes package fixtures: `tests/pass/calc`, `pkg/math`, `relpkg`, `split`, `util`, `wrapper`, and `tests/fail/cycle_a`, `cycle_b`.

JUnit classes, all under `src\test\java\com\rosegoldc\lang\`: `BytecodeTest`, `CheckTest`, `ColorsTest`, `CompletionsTest`, `DebugTest`, `EvalTest`, `FixesTest`, `FormatTest`, `GodotApiTest`, `GodotScriptTest`, `GotoTest`, `HostFontTest`, `HoverTest`, `IdentsTest`, `IdeTest`, `InlaysTest`, `LexerTest`, `LlvmNativeTest`, `LlvmTest`, `MainTest`, `ProjectTest`, `RenameTest`, `SignaturesTest`, `SourcePosTest`, `StdlibTest`, `TestsTest`, `UsagesTest`.

`EvalTest` is the behavior oracle for the interpreter (`Run.runFile` / `Run.runSource`). `IdeTest` covers the line protocol. `GodotScriptTest` is large because it locks the engine stub mapping.

Examples worth parsing first: `hello.rg`, `control.rg`, `collections.rg`, `enum.rg`, `class.rg`, `generics.rg`, `type.rg`, `trait_objects.rg`, `ufcs.rg`, `signals.rg`, `async.rg`, `try.rg`, `constexpr.rg`, `modules.rg`, `import.rg`, `point.rg`, `stdlib.rg`. UI demos: `window.rg`, `widgets.rg`, `contacts.rg`, `todo.rg`. Godot sample: `godot_player.rg`. Project sample: `examples/project.toml` plus `examples/calc/lib.rg`.

## TerraLite stub

| Path | What it is |
| --- | --- |
| `C:\CODE\TerraLite\crates\world\src\rosegold.rs` | Stub parser. Words may contain `-`. Only `before-break` and `after-place`. |
| `C:\CODE\TerraLite\mods\keep-stone.rg` | `before-break "keep-stone" { cancel "stone" }` |
| `C:\CODE\TerraLite\mods\log-place.rg` | `after-place "log-place" { log }` |
| `C:\CODE\TerraLite\crates\world\src\lib.rs` | Re-exports `ScriptHook`, `load_dir`, `load_source` |
| Server load path in the TerraLite README | `C:\CODE\TerraLite\mods`, every `*.rg`, path order. A bad script aborts startup. |

`load_source` registers `Hooks::register_before_break` and `Hooks::register_after_place`. The Rust port replaces this file after `rosegold-eval` can call those hooks. Do not teach the Java grammar `before-break` unless the plan's open question is answered that way.

## Port slices

Use these boundaries when splitting work. Each slice's tests are the Java files named here.

| Slice | Java | Stop when |
| --- | --- | --- |
| Syntax | `Tok`, `Token`, `Lexer`, `Parser`, `Program`, `SourcePos` | `LexerTest` behavior; every suite file parses or matches its parse `# expect:` |
| Check | `Checker`, `TypeChecker`, `Types`, `Stdlib`, `Project`, `Toml`, `UnusedImports`, `Diagnostic` | `CheckTest`, `ProjectTest`, `StdlibTest`, remaining `tests/fail` |
| Eval | `Interp`, `Value`, and only then `Bytecode` / `Vm` / `Compile` | `EvalTest`, `tests/pass` except `stdlib_ui*` |
| Hosts | `Interp` host methods, `native/host_rest.c`, `builtin/std` except `ui` | `tests/pass/stdlib_math.rg` through `stdlib_vec.rg`, `stdlib_std.rg` |
| CLI | `Main`, `Run`, `Tests`, `Format`, `Scaffold` | `MainTest`, `FormatTest`, `TestsTest`, `rg test` on both directories |
| IDE | `Ide`, `Hover`, `Completions`, `Goto`, `Rename`, `Usages`, `Signatures`, `Fixes`, `Inlays` | `IdeTest` and the matching `*Test` classes |
| UI | `builtin/std/ui`, `HostUi`, `HostFont`, `HostSvg`, `host_ui.c`, `host_img.c` | `stdlib_ui*` and `RG_UI_HEADLESS` |
| Godot | `GodotApi`, `GodotScript`, `builtin/godot/lib.rg`, `native/host_godot.c`, `godot/` | `GodotApiTest`, `GodotScriptTest` |
| Native backend | `Compile`, `Vm`, `Llvm`, `LlvmLink`, `native/runtime.c` | `BytecodeTest`, `LlvmTest`, `LlvmNativeTest` |

Leave UI, Godot, the IntelliJ plugin, and the native backend until the eval slice matches the interpreter. `Compile.Skip` is why the walk has to come first.
