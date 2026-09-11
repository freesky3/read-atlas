# Backlog

## V2 发布前 P0

- [ ] Tauri release build 已通过并生成 MSI/NSIS；仍需验证安装、升级、卸载和签名配置。
- [ ] 实测冷启动 P95 ≤ 2 s、空闲 ≤ 180 MB、单 PDF ≤ 350 MB、Reader 交互 P95 < 100 ms、安装包 ≤ 25 MB。
- [ ] 用 born-digital、扫描、公式、图、表、500 页、重复文件和 Explorer 外部移动样本完成验收记录。
- [ ] 用显式付费开关运行 Gemini/Mistral live smoke：文件复用、Interaction resume、cache token、stale provider ID、429/timeout、OCR staging 恢复和远端删除。
- [x] 已实现托盘继续运行、暂停并退出、取消并退出；事务测试确认付费请求的 interrupted_unknown 语义，仍需桌面人工点击验收。
- [ ] 定义/验证远端 Gemini file/cache TTL；不得把本地成果生命周期误当 provider TTL。
- [x] Paper 已进入 Trash 后，tombstone 登记失败作为 cleanup warning 返回，不再把已发生的本地删除显示为失败。
- [x] 已在本次 release 构建工作树重跑 git diff --check 和 V1 残留扫描。

## V2 质量 P1

- [ ] 增加 App 级引用篮交互测试：移除、排序、发送成功清空、失败保留、Message snapshot 点击回跳。
- [ ] 增加 Chat 集成 fixture：历史 Block 快照进入 recovery/compaction prompt，compaction digest 变化可审计。
- [ ] 为诊断导出增加临时目录文件内容测试和覆盖确认行为测试。
- [ ] 为 watcher 的多事件风暴、网络盘/可移动盘掉线增加 Windows 实测。
- [x] 后端 clippy `-D warnings` 已作为本轮加固验收；不要动 `workspace.lock` 的 `File` 持有者。新增 dead_code 仍须有理由，不要用 `allow` 掩盖真问题。
- [x] 统一 SQLite factory、热点索引、流式 PDF/OCR、tombstone 分批、`WorkspaceRuntime` 一次换上。细节与未搬完的 `send_chat` 见 [backend-hardening.md](backend-hardening.md)。
- [ ] 把 `src/App.tsx` 的剩余编排继续下沉为 Library/Discussion/Reader shell hooks，不改变 `DesktopClient` seam。
- [ ] 重启 Tauri 后用精密封控板实机确认：PDF 张开/捏合只改页和百分比；两指滑动只滚动；库和对话不跟着缩放。

## Full Outline V1（已授权）

- [x] Overview + Deep dive 代码闭环。合同：[full-outline-v1.md](full-outline-v1.md)。
- [x] 提示词目录、删除/重生成、任务中心按 `updatedAt` 新→旧、局部图目录去 ±1。
- [x] dagre TB + lazy React Flow；分叉出厂稿；`outline_epoch=3` 一次性清旧图；紧凑卡片、可拖节点、可收起详情栏。
- [x] 生成完成画布高度为 0、局部图点节点回总图：已修（见 onboarding §28）。
- [ ] 待 live Gemini 实机验收：分叉结构、边标签、删除/重生成、改提示词后再生成、详情栏拖宽。

## OpenAI-compatible / Grok（代码已落地）

- [x] 设置里并存 `gemini` / `openai_compatible` / `grok`；显式「设为当前」后论文/翻译走同一家。合同见 D-017、[agent-onboarding.md](agent-onboarding.md) §29、[2026-08-18-openai-compatible-and-grok-providers.md](superpowers/plans/2026-08-18-openai-compatible-and-grok-providers.md)。
- [ ] 待 live 实机：官方 OpenAI 探针通过后能 Chat / Pack / Lens / Outline；Grok 若只有 `attachment_search` 则不能成为当前论文 provider。

## V2 之后

- [ ] 搜索（产品重新授权后再设计；当前明确不做）。
- [ ] Semantic View/正文重排。
- [x] Reading Guide V1：生成管线、页边 UI、顶岛（D-040）。PDF 虚拟列表已改为绝对定位+实测页高。待重启 `tauri dev` 后对长文 live 验收：能看见 `· N` 条、页不重叠、上滑不弹回。
- [ ] 多文档标签、文库级 Chat、跨论文比较。
- [ ] 非 PDF 格式与联网作者研究。
- [ ] Outline 之后：通用表面分栏（讨论/成果纳入同一壳）；Author Research 只留槽位。
