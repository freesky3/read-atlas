# 隐私与费用 / Privacy and costs

本说明描述当前 Windows Alpha 的数据流，不承诺任何服务商的留存期限或收费标准。

## 本地保存的内容

- 你选择的工作区保存 PDF、OCR 结果、讨论、成果、任务、阅读状态与读者背景。
- 工作区的 `Characters/` 保存角色配置；头像等资源保存在工作区应用数据中。
- 应用全局设置保存模型端点、模型名称、主题和语言等配置。
- API Key 通过 Windows Credential Manager 保存。备份工作区不会备份系统凭据；更换机器后需重新配置。
- 界面字体使用本机字体与随依赖打包的数学字体，不从 Google Fonts 下载。

## 什么情况下联网

| 操作 | 可能发送的材料 | 接收方 |
| --- | --- | --- |
| 模型能力检查、凭据验证 | 小型探测材料及 API 身份验证信息 | 你选择的服务商／代理端点 |
| OCR | PDF 文件 | Mistral OCR |
| 文档根、Brief、路线、讨论等 | 完整 PDF 或服务商文件引用、问题、必要的对话上下文 | 当前配置的模型服务商／代理 |
| 翻译、解释、Lens 与追问 | 选区文本、图像、问题及功能需要的文档上下文；可能需要完整 PDF 来源根 | 当前配置的模型服务商／代理 |
| 地图与旁批 | PDF 来源根、OCR 内容、读者背景及所选角色／提示词 | 当前配置的模型服务商／代理 |

读者背景、自定义提示词和角色说明也可能随生成请求发送。只配置你信任的端点；使用代理时，代理运营方也处于请求路径中。普通本地阅读和文库整理不需要 AI Key。

生成和 OCR 由用户操作发起；已确认任务可以按设置继续执行，重启后可能恢复。关闭窗口可能继续托盘运行，真正停止前请查看任务中心并选择暂停或取消后退出。

## 费用与取消

API 费用由对应服务商收取。应用记录可获得的 token 与用量信息，不能保证它等同于最终账单；这里不提供金额或耗时计算。

**生成旁批会消耗较长时间和大量 token。** 确认页显示同样的提示。取消会停止后续工作，但已经发出的请求可能仍被计费；结果未知的中断任务需要明确处理，不应直接重复提交。

## 删除与远端留存

应用内移入回收站通常保留 30 天。Explorer 直接删除与应用回收站是不同操作。删除本地文献会安排适用的远端对象清理，失败时记录在任务中心；本地删除不保证服务商立即删除全部副本。

服务商缓存、文件和日志的 TTL、训练用途与保留策略由该服务商决定。请阅读其当前条款；无法验证删除时，应用不能承诺已清除远端数据。

## 诊断、备份和报告问题

诊断由用户主动预览和导出。分享之前再次检查内容，不要上传 API Key、完整私人 PDF、私人讨论或未脱敏日志。聊天与成果本身是用户数据，不是可公开的诊断材料。

退出应用后备份整个工作区（包括隐藏目录），恢复到独立目录后再选择它。不要只复制正在使用中的 SQLite 主文件。详见 [使用指南](docs/user-guide.md)。

## English

Read Atlas stores PDFs, OCR, discussions, artifacts, jobs, and reading state in your workspace. API keys are stored in Windows Credential Manager and are not included in a workspace backup. UI fonts are local; there are no Google Fonts downloads.

AI operations send the material they need to your configured provider or proxy. This can include the entire PDF, region images and text, conversation history, reader background, custom prompts, and character instructions. Provider probes also contact the selected endpoint. Previously confirmed jobs may continue in the tray or recover after a restart.

Providers charge for API usage. **Generating margin notes takes significant time and consumes a large number of tokens.** Canceling does not refund requests already sent; interrupted requests may have an unknown billing outcome.

Local deletion does not guarantee immediate remote deletion. Cleanup can fail, and provider retention, caching, logging, and training policies are governed by the provider's own terms. Preview diagnostics before sharing. Quit the app and copy the entire workspace, including hidden directories, before upgrading or restoring.
