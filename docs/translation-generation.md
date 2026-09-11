# 翻译生成合同

## 定稿与来源

翻译提示词按用户逐段确认的八部分完整落地，不压缩正文：

- [讨论定稿](note/translation-prompt.md)
- [生产正文](../src-tauri/prompts/translation.md)
- [旧出厂稿](../src-tauri/prompts/v1/translation.md)

完整正文为 10,463 个字符（按 LF 计）、八部分、45 个条款小节。最终校对增加材料与指令的边界、实质恢复的原文依据、部分译文必须有可理解内容、状态与字段的对应约束。测试直接比较完整定稿与运行时正文。

论文与教材共用此稿；保留 `{output_language}` 占位符，提示词本身为中文。

## 输入与隔离

继续调用独立翻译模型的文本接口，只发送 `targetBlock / localContext / targetLanguage`。局部上下文保持为同页附近文本、Brief 的简短主题信息和命中的术语/符号。既有 Brief 的 `summary` 兼容映射来自 `takeaway`，不发送九字段全文。

不发送 PDF，不续接文档根，不读取读者上下文，不加入讨论历史，不自动更新全局术语表。目标文本及辅助材料中的指令按文档内容处理。实质恢复须有目标文本或相关原文依据，辅助成果不能单独证明原文应该是什么。

## v2 输出与本地校验

模型只输出六个字段：

| 字段 | 类型与约束 |
| --- | --- |
| `status` | `translated / unchanged / partial / unavailable` |
| `sourceLanguage` | 非空语言代码；无法识别用 `und`，实质性混合语言用 `mul` |
| `targetLanguage` | 必须与本次请求一致 |
| `translation` | 字符串；仅 `unavailable` 必须为空，其余状态必须包含非空内容 |
| `notes` | 字符串数组，条目不可为空白；`partial / unavailable` 至少一条 |
| `terms` | 数组；每项恰含 `source / target / note`；前两项非空，`note` 可以为空 |

`unavailable` 的 `terms` 必须为 `[]`。`partial` 正文不能只有已约定的 `[文本缺损]` / `[无法辨认]` 标记。模型输出校验后，应用才附加 `translationProtocol`、来源块和本地证据等内部字段；内部字段不进入模型 Schema。

严格 Schema 与字段关系校验位于 [translation_contract.rs](../src-tauri/src/translation_contract.rs)。为保持现有 Provider 的 Schema 兼容，跨字段条件由本地校验实现，不依赖 Provider 支持条件 Schema。

本地校验能检查字段、类型、空值、状态组合、目标语言和已知占位标记；译文忠实性、语义完整性、语言识别正确性及译注是否充分仍须依靠模型质量和人工评估，不能由结构测试证明。

## 发布与展示

- `unavailable` 是一个有效的翻译结果，作为 ready 成果发布；译文字段为空，界面显示“无法翻译”与具体译注，不补造正文，不自动 repair。
- 其余状态显示“已翻译”“无需转换”“部分完成”。源语言 `und / mul` 显示为“源语言未确定 / 多种语言”。
- 网络错误、接口错误和协议校验失败继续走任务失败流程，与无法形成译文的内容状态区分。
- 历史五字段成果正常展示，不猜测或回填 `status`。原始 JSON、人工数据与旧成果均不重写。

## 提示词与任务兼容

`prompt-settings.json` 保持 schema 2，新增 `translationGeneration = 2` 和旧翻译文本登记。迁移前将原配置完整备份至 `prompt-settings.before-translation-v2.json`。

- 仅当前文本匹配已知旧出厂稿时自动切换完整中文定稿；论文和教材分别判断。
- 旧自定义文本及已保存的上一版完整保留；已登记的旧稿使用 v1 五字段协议。
- 编辑旧自定义稿继续使用 v1。恢复默认后使用 v2，基于该默认进行的新编辑也使用 v2。恢复上一版时依据已登记文本恢复对应协议。
- 入队时从同一份设置快照冻结提示词与 `prompts.translationProtocol`。旧任务缺少该字段时按 v1 执行；未知版本拒绝执行。
- v1、v2 各有固定 Schema 与对应的缺省提示词，旧任务执行中不读取新默认稿。v1 保持原有接受条件，v2 严格校验新增规则。

## 验证

验证覆盖完整原稿一致性、论文/教材共用默认稿、迁移备份、自定义与上一版恢复、四种状态与错误组合、旧任务兼容、翻译文本接口隔离、空译文发布以及界面显示。

本次验证结果：

- `cargo test --manifest-path src-tauri/Cargo.toml --lib --no-fail-fast`：439 项通过。
- `npx vitest run src/ArtifactPanel.test.tsx`：20 项通过。
- `npm run build`：TypeScript 检查与前端生产构建通过，保留现有体积提示。

未运行付费模型调用或真实论文翻译质量评估。
