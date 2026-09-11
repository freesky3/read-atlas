# 导读与页边批注执行计划审查

- 审查日期：2026-09-09。
- 结论：**未通过计划验收。人物配置和部分界面已有实现，但 V2 生产生成链没有完成接线；不能按当前文档所述视为 P0–P5 已落地。**
- 需求基准：[执行计划](reading-guide-characters-plan.md)，结合本任务前序已经同意的产品要求。
- 比较基准：`c4159926ebd763a1a983957792be5c41de2f4f80`（当前 HEAD）。实现位于未提交工作树和新增文件中，没有独立实现提交；本次查看 `git diff <该提交> -- <旁批相关文件>` 及相关 untracked 文件。
- 范围：仅旁批角色、提示词、V2 生成、迁移及展示。共享文件中此前 Brief／翻译／Explanation／Lens 等工作不计入本次实现缺陷。
- 方法：依 code-review 技能分别审查 Standards 与 Spec，并复核实际生产调用链、现有定向测试与临时 Rust 探针。
- 本次仅审查，没有修复生产代码、改写提示词、修改用户配置或调用真实付费模型。

`P1` 表示核心功能／约定的数据恢复能力受阻，应优先修正；`P2` 表示明确功能或兼容缺陷，需要在本次计划验收前完成。两轴分别列出，不能用一轴通过抵消另一轴问题。

## Standards

本轴发现 3 项确定的合同违规，均为 P2；没有把一般代码风格偏好列为问题。

### S1 · P2：历史头像使用绝对文件路径，不能可靠显示或随工作区搬迁

**定位**：[lib.rs:8630](../src-tauri/src/lib.rs)、[personas.ts:38](../src/guide/personas.ts)、[GuideAvatar.tsx:34](../src/guide/GuideAvatar.tsx)。

**规范**：计划 §6.2–6.3 要求工作区内稳定的资产引用及跨机器历史显示；§11.2 要求通过受管资源机制展示头像。

**实际行为**：`start_reading_guide` 将 `path.display().to_string()` 的 Windows 绝对路径放入 `workspaceAvatarPath`；人物解析器原样返回它，`GuideAvatar` 直接将它作为 `<img src>`。这不是 WebView 可加载的受管资源 URL。即使补上 URL 转换，原目录搬家后快照仍指向旧盘符和目录。

**影响**：设置页使用 data URL 的头像可以预览，但正文旁批中的头像会加载失败并退回占位；工作区迁移进一步暴露路径绑定问题。

**建议**：历史快照保存资产 ID／工作区相对路径；投影按当前工作区解析，通过项目受管资源协议供前端读取。头像缺失时保留名字与颜色，但不能把正常导入后的普遍加载失败当成合理占位。

**验证性质**：已核对完整保存→投影→组件调用链，未执行真实桌面头像视觉验收。

### S2 · P2：编辑或恢复旧自定义提示词会丢失 V1 来源

**定位**：[prompt_settings.rs:450](../src-tauri/src/prompt_settings.rs)、[prompt_settings.rs:622](../src-tauri/src/prompt_settings.rs)、[prompt_settings.rs:1254](../src-tauri/src/prompt_settings.rs)。

**规范**：计划 §10.3 要求自定义稿和 previous_text 逐字保留，并在编辑、恢复时保持其协议来源。

**实际行为**：`save_slot` 只为 Auxiliary、Translation 和 Lens 继承 legacy 来源，Guide 没有对应逻辑；投影又仅凭当前文本是否命中 `legacy_guide_texts` 判为 V1。迁移记录当前 text，但没有记录旧自定义 previous_text。

**最小复现**：使用临时旧配置，其中两份 Guide 自定义稿都是 V1。迁移后仅微调 context 的文字，或仅恢复其旧自定义 previous_text，结果均为：

```json
{
  "originalWorkflow": "v1",
  "editedContextProtocol": "v2",
  "restoredContextProtocol": "v2",
  "workflowError": "旁批读懂与落笔属于不同协议代际，请恢复对应配对预设后再生成"
}
```

**影响**：正常微调旧稿后无法继续生成；若两份都编辑则可能错误转入 V2。用户原稿的文本虽在，运行语义已经改变。

**建议**：为稿件保存及恢复显式继承协议来源，并将迁移时存在的历史自定义稿纳入兼容记录。只有用户明确采用新协议时才切换，不能用新的文本摘要自动推断为 V2。

**验证性质**：直接导入当前生产 `prompt_settings.rs` 和 `library_paths.rs` 的 Rust 探针已复现；未访问真实 `prompt-settings.json`。

### S3 · P2：保存中断后人物库会静默恢复出厂

**定位**：[guide_character_assets.rs:180](../src-tauri/src/guide_character_assets.rs)、[guide_character_settings.rs:415](../src-tauri/src/guide_character_settings.rs)。

**规范**：计划 §6.3 要求原子替换、失败恢复及读取失败不静默清空自定义角色。

**实际行为**：`atomic_write` 先把正式文件改名为 `.bak`，再把 `.tmp` 改成正式文件，两步间正式路径不存在。进程在此期间退出后，下次 `read_file` 只检查正式路径；不存在就写入 factory，完全不检查残留备份和临时文件。

**最小复现**：临时目录模拟保存中断，旧配置有 6 人、版本 42，包含一个自定义角色。调用当前生产读取逻辑得到：

```json
{
  "beforeCharacterCount": 6,
  "afterCharacterCount": 5,
  "beforeStoreRevision": 42,
  "afterStoreRevision": 1,
  "customCharacterSilentlyGone": true,
  "backupStillExists": true
}
```

**影响**：自定义人物从应用中无提示消失；虽然 `.bak` 尚可人工恢复，但正常启动流程已经接受新出厂配置。

**建议**：采用支持失败恢复的原子替换协议，或者在创建初始配置前识别并恢复／报告残留事务文件。并发保存也需要配套串行化，不能只依赖读出后比较 revision。

**验证性质**：临时 Rust 探针已复现；所有文件只在 `src-tauri/target/guide-review/` 内。

## Spec

本轴发现 7 项应在验收前修正的实质问题：3 项 P1、4 项 P2。角色页面有实现、模块各自有测试，与生产行为完成是两个不同条件。

### F1 · P1：V2 落笔没有收到人物设定，实际校验还能将千反田改署名为阿林

**定位**：[lib.rs:8975](../src-tauri/src/lib.rs)、[lib.rs:8709](../src-tauri/src/lib.rs)、[guide_validate.rs:630](../src-tauri/src/guide_validate.rs)。

**对应计划**：§7.4、§9.1–9.2、P2／P4 要求动态人物注入、冻结阵容白名单，以及 V1／V2 校验隔离。

**实际行为**：入队 payload 有 `castSnapshot`，但生产落笔请求仍只发送旧 `context / batch / repairHint`，没有发送人物设定或阵容。它继续使用 `inks_schema()` 与 `apply_guide_batch_text`，后者调用旧 `validate_guide_inks`。新增 `annotate_user_input`、`inks_schema_v2`、`validate_batch`／`validate_guide_inks_v2` 并未接入该生产路径。

**最小复现**：向生产实际使用的旧校验器提交合法 V2 note：

```json
{
  "id": "n1",
  "kind": "note",
  "speakerId": "preset:chitanda",
  "blockId": "b1",
  "weight": "line",
  "body": "Keep this condition.",
  "parentId": null
}
```

返回一条有效旁批，但 `speakerId` 被改成 `alin`，`warnings` 为空。原因是旧人物解析失败后，`recover_loose_ink` 把未知人物当成默认阿林恢复。

**影响**：用户选择五人及编辑性格不会真正控制模型输出；即使模型写出正确的新人物 ID，结果也可能错署名。恢复和最终校验还再次使用旧校验器，不能只修第一次请求。

**修正方向**：为 V2 打通独立执行路径，使初次生成、补充、修复、恢复、最终校验共享同一冻结阵容与新协议；旧人物容错只保留在 V1。验收必须从真实任务入口使用假模型走通，而不只调用孤立 helper。

### F2 · P1：正文、读后备忘和文字密度仍按旧流程处理

**定位**：[lib.rs:8964](../src-tauri/src/lib.rs)、[lib.rs:9048](../src-tauri/src/lib.rs)、[lib.rs:9250](../src-tauri/src/lib.rs)。

**对应计划**：§4、§8.3–8.5、§9.3 及 P3 要求完整正文、跨页读后发现、文字密度和有依据的覆盖补充。

**实际行为**：虽然 V2 先取了完整 OCR 正文，随即又交给旧 `locator_catalog_value`，继续每页最多八块、每块 240 字。新备忘使用 `documentFocus / spans / observations / connections`，但落笔前调用的 `compact_guide_context` 只认旧 `thesis / sections`。稀疏检查仍使用全部可定位墨水数量，覆盖报告也只有失败批数等旧字段。

**已复现**：

- 同页输入 12 个 1000 字块，实际落笔目录只剩 8 块、每块 240 字。
- 有内容的新备忘经过实际调用的压缩函数后，结果为 `{ "thesis": "", "sections": [] }`。
- 代码显示六页达到三条可定位墨水即可不补充；trace 仍可满足这个旧门槛。

**影响**：用户最看重的密集、具体、前后连贯的阅读痕迹，没有得到实际材料支持。新增覆盖统计与材料分批测试即使通过，也不会改变生产行为。

**修正方向**：接入预算分批、相关备忘选择、不同主 note 位置统计和逐页缺口补充。修复消息同时应包含已接受墨迹、真实遗漏范围和错误，不仅是一句“本批不足”。

### F3 · P2：生成计划没有冻结阵容和预算，确认时会重新读取配置

**定位**：[guide_module.rs:364](../src-tauri/src/guide_module.rs)、[lib.rs:8579](../src-tauri/src/lib.rs)、[App.tsx:3551](../src/App.tsx)。

**对应计划**：§5.3、§9.4、§10.1 要求 planId／digest 绑定已确认内容、配置变化失效，以及实际调用预算。

**实际行为**：`plan_for_route` 返回的 `plan_id / plan_digest / guide_protocol / character_ids` 都为 None；前端 start 只提交 revisionId 和 characterIds。start 再调用旧 plan，重新加载当前提示词、模型和角色；收到的可选 planId／digest 仅被抄进 payload，并未校验。

**影响**：用户看到生成面板后编辑角色、切换配置再开始，实际执行内容可能与面板所展示的计划不同。已有 Outline 时面板按“复用地图、理解调用 0 次”估算，但 V2 强制不复用 Outline，仍会发一次全文调用。修复次数和人物输入预算也没有真正冻结。

**修正方向**：实现真实计划快照及摘要绑定，开始时校验依赖并使用被确认的快照；变化时刷新计划，不能把可选字段存在视为冻结已经实现。

### F4 · P1：付费响应不能可靠重放，发布也没有使用稳定成果 ID

**定位**：[lib.rs:8882](../src-tauri/src/lib.rs)、[lib.rs:9123](../src-tauri/src/lib.rs)、[lib.rs:9282](../src-tauri/src/lib.rs)。

**对应计划**：§10.2 与 P4 要求先可靠保存模型原始响应／节点／receipt，再解析，并按稳定发布 ID 恢复。

**实际行为**：记录 Provider 节点与费用后直接解析 `response.text`；只有解析成功的备忘或已接受的批次墨迹才进入 checkpoint。`record_route_interaction` 没有保存原始文本，模型 adapter 也只做 committed／取消控制。备忘没有计划要求的一次结构修复。发布调用向 `publish_v2` 传入 `None`，因此每次进入都创建新 UUID。

**影响**：模型已返回但本地解析失败、或返回后 checkpoint 前退出时，恢复没有原始材料可以重放，需要重新请求。发布成功但 Job complete 前退出后，再次进入会新建 head，而不能复用同一次发表。当前测试没有从真实执行入口覆盖这些中断点。

**修正方向**：每个逻辑调用的完整 outcome 先持久化，费用按稳定 operation 去重；保存批次和补救计数；入队时分配并保存 publish ID，恢复直接验证或复用它。此项包含未按计划补齐的旧流程缺口，并非声称这些代码全部由此次新引入。

### F5 · P2：单篇阵容实际是跨文档共享的临时状态

**定位**：[App.tsx:466](../src/App.tsx)、[App.tsx:3500](../src/App.tsx)、[guide_module.rs:703](../src-tauri/src/guide_module.rs)。

**对应计划**：§5.3 要求“本次草稿 > 文档记忆 > 全局默认”，取消不保存，文档之间互不覆盖。

**实际行为**：前端只有一个顶层 `guideCastIds`；打开面板时，只要它非空就继续使用。切文档和取消面板没有重置。后端虽写了 `save_document_cast`，对应 `document_cast` 没有生产调用点。

**触发**：在 A 文档选择临时人物后取消，打开 B 文档的生成面板，B 会继承 A 的临时选择；重启应用后又无法读回数据库保存的单篇阵容。被删除角色留在旧状态中也没有完整的不可用处理。

**修正方向**：以文档 ID 管理偏好，以面板生命周期管理草稿；读取单篇偏好，只在成功入队后保存。取消、切文档和生成失败需要有明确的状态处理。

### F6 · P2：切换默认参与状态会丢失未保存的人设草稿

**定位**：[SettingsGuideCharactersPage.tsx:176](../src/SettingsGuideCharactersPage.tsx)、[SettingsGuideCharactersPage.tsx:94](../src/SettingsGuideCharactersPage.tsx)。

**对应计划**：§5.1、P5 要求草稿保存反馈与离开编辑时的保护。

**触发**：修改一段性格说明但未保存，随后点击角色的“加入／移出默认阵容”。成功回调执行 `applyStore(next)`，它将 draft 改成服务器已保存版本并清掉 dirty。

**影响**：用户刚输入的人设无提示丢失。键盘 Enter／Space 切换角色也绕过鼠标路径的 dirty 检查，是相同草稿生命周期问题的另一入口。

**修正方向**：修改默认阵容时只更新相应配置及 revision，保留人设草稿；统一鼠标、键盘、关闭页面的离开编辑逻辑。增加真实用户操作序列测试。

### F7 · P2：主动试写只有恒报错的命令，没有可用入口

**定位**：[lib.rs:2007](../src-tauri/src/lib.rs)、[SettingsGuideCharactersPage.tsx:633](../src/SettingsGuideCharactersPage.tsx)。

**对应计划**：§5.4、P4／P5 要求草稿试写、同材料角色比较、取消、费用显示和主动采纳样稿。

**实际行为**：`preview_guide_character()` 无请求参数，并始终返回“试写旁批需要桌面运行时与已配置的模型”。即便运行于桌面且模型已配置，函数也不检查条件、更不会调用模型。角色设置页只有静态 exampleNotes 预览。

**影响**：用户无法验证自己修改的人设，也无法进行计划约定的人物校准；这不是尚待真实模型验收，而是功能本身未实现。

**修正方向**：完成显式触发的试写入口和后端，使用草稿快照及生产落笔核心；静态外观预览继续保留且不计费。

## 其他未完成项与接线前必须补的检查

这些项目不重复计入上述 10 项主发现，但不能在接线后忽略。

1. **备忘缓存与 PDF-only 来源根尚未接入**：`GuideMemoCacheKey / memo_cache_digest / load_memo / save_memo` 已定义但没有生产使用；全文阶段仍以 `remote_file_id=None`、`previous_interaction_id=None` 独立发送 PDF。文档所写的“复用或生成备忘”不能作为已完成事实。
2. **新分批器自身不能拆开超预算单页**：`guide_catalog.rs:110` 的预算检查要求 `end > cursor`，首个页面无论多长都会收下。临时探针将同页 20,000 字按 2,000 字预算处理，仍得到一个 20,000 字批次。片段被再次按页收拢；接线之前要完成同页拆批，并计入邻接材料、角色与备忘的预算。
3. **备忘本地校验不等于严格 schema**：`GuideMemo` 对必填数组使用 serde default；探针仅提交 schemaVersion 与 documentFocus 即被接受。子对象额外字段和定位白名单也未被全面核对。应补缺字段、非法页码／块、错误连接的测试。
4. **预设阵容没有 UI**：后端 `presetCasts` 已有值，角色页未提供实际选择路径。
5. **头像只有固定中心裁切**：未提供用户可操纵的裁剪；图片导入的内容验证、尺寸约束还需按计划验收。
6. **提示词细节仍需补全**：两份落笔稿对 OCR／备忘／旧旁批中的指令文字没有计划 §7.4 要求的明确边界；教材落笔稿缺论文稿已有的数学真实换行要求。
7. **文档状态偏乐观**：计划顶部宣称 P0–P5 的主要模块已落地，但生成合同把未接通的过程写成 authoritative 现状。应以实际端到端行为更新状态，至少把 P3／P4、试写及快照显示标为未通过。
8. **P7 未做的说明是准确的**：没有真实模型质量、五人辨识度、跨页密度和桌面视觉验收的证据；不能由单元测试数量推导这些已通过。

## 已核实的完成部分

- 五位人物预设存在；默认阵容为千反田、折木、芙莉莲。
- 人物资料包含详细性格、阅读习惯、避免表现和样稿，配置独立于公共提示词。
- 四份中文稿有 9／12 节结构，且与 `docs/note/reading-guide-prompt.md` 对应正文逐字一致。
- 四稿 LF 归一化长度分别为：读懂论文 1787、读懂教材 1866、落笔论文 2415、落笔教材 2269 个字符。**不能仅因比计划短就判为失败**；本报告指出的是具体遗漏和实际接线问题。
- 新 V2 校验、覆盖、分批、备忘、人物快照等模块已经写出，部分孤立测试通过。
- `publish_v2` 内部已经使用事务，历史人物快照也有字段与显示解析入口；缺的是完整正确的调用、稳定资源路径和恢复行为。
- 旧三人历史映射没有直接被替换成动漫角色，但生产误走旧容错仍会造成 F1 的错署名。

## 验证记录

| 检查 | 实际结果 | 能说明什么 |
| --- | --- | --- |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib guide_ --no-fail-fast` | 53 通过、0 失败，434 项过滤 | 旁批相关单元测试通过，不等于完整生产执行链通过 |
| `npm run test -- src/guide/ src/SettingsWorkbench.test.tsx tests/settingsGuideCharactersPage.test.tsx --maxWorkers=1` | 6 个测试文件、20 项通过 | 当前设置与旁批定向 UI 测试通过，未覆盖本文指出的交互序列 |
| `npm run build` | TypeScript 与 Vite 生产构建通过 | 可编译；保留现有大于 500KB chunk 提示 |
| 生产模块 Rust 探针 | 新角色被无警告改为阿林；8块／240字截断；备忘变空；单页超预算；必填备忘数组缺失仍接受 | 直接调用当前源文件的确定行为，未修改实现 |
| 配置中断探针 | 6个人物／revision42 → 出厂5人／revision1，无错误 | S3 已复现 |
| 旧稿迁移探针 | V1 自定义稿微调或恢复 previous 后误变 V2，workflow 报错 | S2 已复现 |
| 真实 Provider／桌面视觉 | 未执行 | 不做内容质量或头像视觉通过声明 |

本次没有扩大运行到完整 487 项 Rust 测试；定向 53 项与构建已足够证明“通过现有检查仍可存在生产接线缺口”。后续修复应补实际生成入口的假模型测试，再运行计划要求的完整回归。

探针代码和 JSON 结果位于 `src-tauri/target/guide-review/`，是临时审查产物；真实用户工作区和应用配置未被读取或改写。测试日志为项目根下 `guide-review-rust-tests.log`、`guide-review-ui-tests.log`、`guide-review-build.log`。关键复现结果已抄录到本报告，即使清理 target 也可阅读。

## 重新验收需要的证据

- 从实际 `start_reading_guide` 到执行、修复、恢复、发表，使用假模型完整走通默认三人及自定义五人阵容，断言实际请求含人物设定且没有旧三人回退。
- 对包含长正文、跨页观察的固定材料，检查模型真正收到的文本、相关备忘和目标覆盖；验证 trace 不满足主 note 密度。
- 模拟支付响应返回后解析失败、checkpoint 之前退出及发布后退出，证明没有再次发出同一逻辑调用，也不产生新的发布 ID。
- 回归旧自定义稿保存／恢复、人设保存中断及跨工作区头像路径，保留真实 fixture。
- 用 UI 测试覆盖“编辑人设→切默认阵容”、A 文档取消→B 文档、应用重启读回单篇偏好、删除人物后显示不可用选择。
- 完成试写和预设阵容入口，再做人物语气、密度与真实桌面布局的人工验收。

Standards：3 项 P2，主要风险是历史资源和用户配置不可靠；Spec：7 项（3 项 P1、4 项 P2），主要阻塞是 V2 生成主链未接通。两轴均未通过。
