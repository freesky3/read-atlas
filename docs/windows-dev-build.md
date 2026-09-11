# Windows 开发构建

本页记录 **Windows 上编/跑 Read Desktop** 的坑。运行时崩溃和编译器崩溃退出码不同，不要混。新坑只在对应表末追加。

权威链接参数：`src-tauri/build.rs`（应用主线程栈）。权威 dev profile：`src-tauri/Cargo.toml` `[profile.dev]`、`src-tauri/.cargo/config.toml`。

## 1. 两种 Windows 失败，先看退出码

| 退出码 | 何时 | 是什么 | 不是什么 |
| --- | --- | --- | --- |
| `0xc00000fd` `STATUS_STACK_OVERFLOW` | **已经编出来的 exe 启动/跑 IPC** | 进程主线程栈爆了 | rustc 内存不够 |
| `0xc0000409` + `rustc-LLVM ERROR: out of memory` | **`tauri dev` / `cargo build` 正在编 `read-desktop` lib** | rustc/LLVM 堆内存不够，编译器自己死了 | 应用栈溢出；也不是业务语法错误 |

`cargo test` 绿、`tauri dev` 红，优先查本页 §3，不要去改 `generate_handler!`。

## 2. 运行时栈溢出（`0xc00000fd`）

Tauri 2 把 ~80 个 `#[tauri::command]` 展成巨大 match。Windows 默认主线程栈 1 MiB 会在启动时溢。

已做：

- `build.rs`：`cargo:rustc-link-arg=/STACK:8388608`（8 MiB **保留**，不是立刻提交）
- `AppState` 重资源全部 `Arc` / 堆上；回归 `app_state_fits_comfortably_on_a_small_stack_frame`（栈占用 < 4 KiB）

不要做：

- 在 `AppState` 上再放未装箱的大结构或大数组
- 删 `workspace.lock` 的 `File` 句柄（看起来 unused，是在持有独占锁）
- 把这个退出码当成「编译 OOM」去削 debuginfo

合同：[decisions.md D-036](decisions.md)、[agent-onboarding.md](agent-onboarding.md) §33。

## 3. 编译器 LLVM OOM（`0xc0000409`）

### 3.1 为什么会炸

这个 crate `[lib] crate-type = ["staticlib", "cdylib", "rlib"]`，一次 rustc 要出三份产物。默认 `debug = 2` + incremental **256** codegen units，再并行编 `windows-sys` 这类胖子，Windows 上 LLVM 很容易 `Allocation failed`。

被杀掉的编译会在 `src-tauri/target/debug/incremental/read_desktop_lib-*/s-*-working/` 留下 `dep-graph.part.bin`。下次增量接着用这份半成品，更容易再炸。

### 3.2 已经落地的缓解

| 位置 | 设置 | 作用 |
| --- | --- | --- |
| `src-tauri/Cargo.toml` `[profile.dev]` | `debug = 1`、`codegen-units = 16` | 本 crate 只留行号表，少开 LLVM 单元 |
| `[profile.dev.package."*"]` | `debug = false` | 依赖不带全量 debuginfo |
| `src-tauri/.cargo/config.toml` | `build.jobs = 2` | 同时最多两个 rustc，避免和 `windows-sys` 抢内存 |

2026-08-30：在上述设置下 `cargo build --no-default-features --locked` 约 4 分钟通过。`dead_code` 警告是旧债，与这次崩溃无关。

### 3.3 仍 OOM 时

1. 停掉另一个 `cargo test`、`tauri dev`、rust-analyzer 的 cargo。
2. 删增量（**不要**为了腾磁盘去删用户 Workspace）：

```powershell
Remove-Item -Recurse -Force src-tauri\target\debug\incremental
```

3. 再 `npm run tauri dev`。需要更狠时临时 `CARGO_BUILD_JOBS=1`。
4. 不要为了「修 OOM」去升 schema、改 IPC、或删 `crate-type` 里的 `rlib`（测试和 bin 还要用）。

### 3.4 踩坑日志（只追加）

| 日期 | 症状 | 不要再做 | 正确做法 |
| --- | --- | --- | --- |
| 2026-08-30 | `tauri dev`：`rustc-LLVM ERROR: out of memory` / `0xc0000409` | 当成 `0xc00000fd` 去加栈；或把 `[profile.dev] debug=1` 套到所有依赖后无限制并行重编 `windows-sys` | 本 crate 降 debuginfo + `jobs=2`；先清 `incremental` |
| 2026-08-30 | 全局 `package."*" debug=false` 触发全量重编，并行编 `windows-sys` 再次 OOM，其它 crate 报 `can't find crate for std` | 一次改 profile 又开满 CPU | `jobs=2` 后再编；`can't find std` 多半是邻居 rustc 被杀，不是缺 target |
| 2026-08-30 | incremental 目录里大量 `s-*-working` | 在半成品增量上继续编 | 删 `target/debug/incremental` |

## 4. 其它 Windows 环境

| 坑 | 做法 |
| --- | --- |
| C 盘满 → linker / Vitest `ENOSPC` | 进程 `TEMP`/`TMP` 指到 D 盘临时目录；不要删用户 Workspace |
| 没有 `python3` | 用 `py -3` |
| 改 `webview_pinch.rs` 或 Tauri 命令后界面没变 | 必须重启 `tauri dev`，热更不够 |
| `tauri dev`：`cargo metadata` / `program not found` | 不是缺 crate、也不是要重装 Rust。`CARGO_HOME` 有值但 **PATH 没有** `cargo.exe`。见 **§4.1** |
| 仓库路径被粘成 `project \read_desktop`（`project` 后多空格） | 真实路径无这段空格：`<repo>` |

### 4.1 rustup / cargo 搬家之后 PATH 仍指向空目录

以下以自定义工具链目录为例，使用你自己的实际目录：

| 变量 / 路径 | 值 |
| --- | --- |
| `CARGO_HOME` | `<CARGO_HOME>` |
| `RUSTUP_HOME` | `<RUSTUP_HOME>` |
| 真正的 `cargo.exe` | `<CARGO_HOME>\bin\cargo.exe` |

只设 `CARGO_HOME` / `RUSTUP_HOME` **不够**。`npm run tauri -- dev` 调的是 **当前 shell PATH 里的 `cargo`**。用户 PATH 若仍留着空的 `%USERPROFILE%\.cargo\bin`（搬家后这个目录没有 exe），就会 `cargo metadata ... program not found`。

正确做法：

1. 把 `<CARGO_HOME>\bin`（或 `%CARGO_HOME%\bin`）放到**用户 PATH 最前**，删掉或挪走空的旧 `.cargo\bin`。
2. **关掉所有已经打开的终端 / IDE 终端 / 旧 PowerShell**。用户 PATH 只对之后启动的进程生效。
3. 新开 PowerShell：

```powershell
Set-Location <repo>
where.exe cargo
cargo --version
rustc --version
npm run tauri -- dev
```

`where.exe cargo` 必须指向 `<CARGO_HOME>\bin\cargo.exe`。若仍指向 `%USERPROFILE%\.cargo\bin`，说明旧终端没关，或用户 PATH 没改对。

不要：把找不到 cargo 当成要重装 Rust；不要改仓库里的 Tauri 配置来「写死 cargo 路径」；不要在已经打开的 PowerShell 里反复试（它看不到新 PATH）。

### 4.2 环境类踩坑日志（只追加）

| 日期 | 症状 | 不要再做 | 正确做法 |
| --- | --- | --- | --- |
| 2026-09-04 | `npm run tauri -- dev` → `cargo metadata` program not found；用户已把 cargo 挪到 `<CARGO_HOME>` 并设了 `CARGO_HOME` | 只改环境变量、不改 PATH；在旧 PowerShell 里重试；重装 rustup | PATH 前置 `<CARGO_HOME>\bin`；关旧终端；`where.exe cargo` 验证 |

## 5. 验证编译

```powershell
cd src-tauri
cargo build --no-default-features --locked
```

通过后再 `npm run tauri dev`。全量 `cargo clippy --locked --all-targets -- -D warnings` 目前会被仓库里既有 `dead_code` 挡住，不要为消 warning 删看起来 unused 的锁/API。
