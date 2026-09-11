# 批注角色

状态：authoritative（2026-09-09，D-068）

设置 → 批注角色 管理当前工作区共用的人物库。本轮沿用实施后的 `Characters/` 文件夹布局；它是原计划“应用配置目录中的单个 JSON”的实现差异，角色库仍独立于 `prompt-settings.json`。

- `Characters/cast.json`：默认阵容、预设阵容、仓库修订号和退役 ID。
- `Characters/<人物目录>/character.json`：该人物的完整设定；头像可存为同目录 `avatar.*`。
- `Characters/_assets/`：按内容摘要存储的导入头像。
- `.read-desktop/guide-character-assets/`：计划快照使用的不可变头像副本，只保存相对工作区路径。

读写使用文件锁和修订号检查；多文件提交先保存完整 `.pending-store.json`，中断后重放同一完整版本。备份尚未恢复时不会静默恢复出厂。删除人物记录退役 ID，保留人物目录和用户自行放入的资料；历史旁批继续使用旧快照。

## 出厂人物

| ID | 显示名 | 默认 |
| --- | --- | --- |
| `preset:chitanda` | 千反田爱瑠 | 是 |
| `preset:oreki` | 折木奉太郎 | 是 |
| `preset:frieren` | 芙莉莲 | 是 |
| `preset:jotaro` | 空条承太郎 | 否 |
| `preset:conan` | 江户川柯南 | 否 |

完整设定见 `docs/note/guide-character-presets.md` 与 `src-tauri/prompts/guide-characters/`。

## 快照

成果与任务携带完整 `GuideCastSnapshot`，显示时不得改查最新全局人设。删除角色不删除历史头像副本。V1 的 `alin / laozhou / xiaxia` 只用于旧成果，绝不映射到上述五人。

## IPC

`get_guide_character_settings`、`save_guide_character`、`delete_guide_character`、`restore_guide_character_preset`、`duplicate_guide_character`、`save_guide_default_cast`、`import_guide_character_avatar`、`get_guide_character_avatar`、`preview_guide_character`、`cancel_guide_character_preview`。参数包 `{ request }`。浏览器 memory adapter 可本地编辑，不得伪造试写或生成成功。


## 草稿与试写

默认阵容或预设阵容变化不会覆盖未保存的人设。切换人物、导航、Esc 和关闭设置都经过同一草稿保护。试写由按钮明确触发，接收当前未保存草稿，可与最多两位其他角色使用同一段虚构材料比较；提供取消、真实用量与样稿采纳。采纳只修改草稿，仍须保存。打开设置与静态预览不调用模型。
