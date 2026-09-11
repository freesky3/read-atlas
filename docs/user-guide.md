# 使用指南 / User guide

当前版本以 Windows Alpha 为目标。先使用可丢弃的测试工作区熟悉操作。

## 首次配置

在 Settings → Workspace 选择专用目录。应用将创建 `Papers/`、`Textbooks/` 和 `.read-desktop/` 等数据目录。工作区应独立于源代码、下载目录和系统目录。

在 Settings → Models 配置模型服务商。Gemini Proxy 默认针对本机代理；OpenAI-compatible 的模型与端点由你提供。完成能力检查后设为当前服务商。Mistral OCR 使用独立凭据。

模型必须支持该功能所需的原生 PDF 或上下文能力。“接口兼容”不意味着某个模型已完成实际功能验证。详见 [隐私与费用](../PRIVACY.md)。

## 获取 API Key

设置中的密钥输入框旁提供官方获取入口。点击后在系统默认浏览器打开；登录服务商控制台创建密钥，再回到应用填入。

| 服务 | 获取页面 |
| --- | --- |
| Mistral OCR | [Mistral Console](https://console.mistral.ai) |
| Gemini | [Google AI Studio](https://aistudio.google.com/api-keys) |
| OpenAI | [OpenAI API keys](https://platform.openai.com/api-keys) |
| Grok | [xAI API keys](https://console.x.ai/team/default/api-keys) |
| DeepSeek 兼容端点 | [DeepSeek API keys](https://platform.deepseek.com/api_keys) |

自定义兼容地址应使用实际提供该接口的服务商密钥，本机代理使用代理管理页配置的密钥。链接只帮助获取凭据，不代表该服务或模型已经通过原生 PDF 能力验证。

## 阅读流程

1. 从文库导入论文或教材 PDF。导入会复制到工作区，重复内容按 hash 识别。
2. 打开 PDF，调整缩放并阅读。返回文库后可再次恢复阅读位置。
3. 需要选区、地图或旁批时，主动执行 OCR。
4. 选择文本、公式、图片或表格，执行翻译、解释、Lens 或加入讨论。
5. 从全篇成果生成 Brief、术语／符号表，或生成精读路线与论证／知识地图。
6. 旁批生成前选择角色并确认。**这项功能会消耗较长时间和大量 token。**

模型输出可能有错误；对关键推导、引用和结论应返回原 PDF 核对。中英文切换影响界面和后续生成，历史成果及自定义内容保留原文。

## 任务与退出

任务中心显示生成进度、失败和恢复入口。已经发出的模型请求可能产生费用，即使你稍后取消。遇到“结果未知”先查看恢复说明，避免直接重复提交。

关闭窗口可能继续托盘运行。需要停止时，选择暂停并退出或取消并退出，并确认进程已经结束。

## 备份与恢复

1. 暂停生成任务，选择退出应用；确认托盘进程也已退出。
2. 复制**整个工作区**到另一个目录或磁盘，包括隐藏的 `.read-desktop/`、`Characters/`、PDF 和读者背景文件。
3. 恢复时复制到一个新目录，在应用中选择这个目录作为工作区。
4. 检查文献数量、阅读位置、讨论、成果版本和待恢复任务，再继续日常使用。

不要只复制仍在运行中的 SQLite 主文件；其 WAL 可能包含尚未合并的数据。API Key 位于操作系统凭据管理器，不在工作区备份中。搬到新机器需重新配置，应用全局主题／模型设置也可能需要重新设置。

未知或过旧工作区可能要求重置。重置会删除界面确认范围内的数据；没有完整备份时不要确认。Alpha 版本的升级验收状态见 [发布检查表](release-checklist.md)。

## 删除与导出

应用回收站和 Explorer 删除不是同一机制。应用内移入回收站通常保留 30 天；远端清理失败会在任务中心记录。本地删除不能保证服务商立即删除数据。

成果导出用于阅读与分享，不是完整工作区备份。导出前检查是否包含私人文献内容、讨论或读者背景。

## English quick guide

Select a dedicated workspace, import PDFs, then configure a provider and Mistral OCR only when needed. Run the provider capability check before selecting it for AI work. Start OCR, discussions, maps, and margin notes explicitly.

Margin-note generation takes significant time and consumes many tokens. Review model claims against the original PDF. To back up, quit the app completely, including the tray process, and copy the entire workspace with hidden files. Restore into a new directory. API keys must be configured again on a different machine.
