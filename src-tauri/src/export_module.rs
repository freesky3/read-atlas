//! §7 Export 行：阅读成果包（reading bundle）的渲染与落盘。
//!
//! 「一份导出 Markdown 长什么样、落在哪里、撞名了叫什么」只有这一处实现：
//! 旧的单篇命令 `export_reading_bundle` 与批量 Export Item 都调本模块。
//! 两处各写一遍的话，批量导出就会导出一个和单篇不一样的东西——那是用户
//! 拿出去给别人看的文件，不是内部表示。

use crate::artifact_module::{ArtifactModule, ArtifactProjection};
use crate::library_paths::EXPORT_DIR;
use crate::outline_module::OutlineModule;
use crate::paper_module::{PaperModule, PaperProjection};
use crate::ui_locale::UiLocale;
use crate::{db_path, library_query::sha256_hex, now, open_db};
use rusqlite::params;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// 一次导出的结论。
///
/// `relative_path` 是 Workspace 相对路径，可以进逐项结果与摘要；绝对路径只在
/// 写文件那一刻出现（§10.2），所以这里根本不存它。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExportedBundle {
    pub paper_id: String,
    pub revision_id: String,
    /// 形如 `export/attention-is-all-you-need.md`。
    pub relative_path: String,
    pub byte_size: u64,
    /// 本次写入的字节摘要：撤销时要用它确认「这个文件还是我们写下的那份」。
    pub content_digest: String,
    /// 写之前那个路径上已经有一个文件：这一项是**覆盖**，不是新建。
    /// §7：只有本批新建的输出可删，被覆盖的那一份回不来了。
    pub replaced_existing: bool,
}

/// 导出失败的分类。OS 原文只留在 `WriteFailed` 里给日志看，
/// 永远不进批次投影（§10.2）——逐项摘要由调用方换成固定文案。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ExportError {
    /// 这一项的 Paper 不在库里了（已进回收站或彻底删除）。
    PaperMissing,
    /// 目录建不出来或文件写不进去：换一台机器、清出空间再跑一次就可能成功。
    WriteFailed(String),
    /// 库自己的记录读不出来。
    Storage(String),
}

impl ExportError {
    /// 给用户看的结论，不含路径也不含 OS 文本。
    pub(crate) fn safe_summary(&self) -> &'static str {
        match self {
            Self::PaperMissing => "this Paper is no longer in the library",
            Self::WriteFailed(_) => "the export file could not be written",
            Self::Storage(_) => "the library could not read this Paper's reading results",
        }
    }
}

/// 命名规则看得见的三件事：PDF 的文件名、所在目录、这份 PDF 的 sha。
/// 把它单独列出来，是因为计划阶段只拿这三样也要能算出同一个输出名——
/// 「预览里说会写到哪」和「执行时真的写到哪」必须是同一条规则算出来的。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NamingFacts {
    pub file_name: String,
    pub collection_path: String,
    pub sha256: String,
}

impl NamingFacts {
    fn of_paper(paper: &PaperProjection) -> Self {
        Self {
            file_name: paper.file_name.clone(),
            collection_path: paper.collection_path.clone(),
            sha256: paper.sha256.clone(),
        }
    }
}

/// 这一份导出会落在哪个 Workspace 相对路径上。
pub(crate) fn planned_output_relative(root: &Path, facts: &NamingFacts) -> String {
    let file_name = export_file_name(&export_dir(root), facts);
    crate::library_paths::normalize_slashes(&format!("{EXPORT_DIR}/{file_name}"))
}

/// 渲染并写出这一份阅读成果包。文件名的冲突规则见 [`export_file_name`]。
pub(crate) fn export_bundle(
    module: &PaperModule,
    root: &Path,
    paper_id: &str,
    locale: UiLocale,
) -> Result<ExportedBundle, ExportError> {
    let paper = module
        .live_paper(paper_id)
        .map_err(ExportError::Storage)?
        .ok_or(ExportError::PaperMissing)?;
    let markdown = render_bundle(root, &paper, locale).map_err(ExportError::Storage)?;
    let directory = export_dir(root);
    fs::create_dir_all(&directory).map_err(|error| {
        ExportError::WriteFailed(format!("Unable to create export directory: {error}"))
    })?;
    let file_name = export_file_name(&directory, &NamingFacts::of_paper(&paper));
    let target = directory.join(&file_name);
    // 「之前有没有文件」必须在写之前问：写完再问永远是真的。
    let replaced_existing = target.exists();
    let bytes = markdown.into_bytes();
    let byte_size = bytes.len() as u64;
    fs::write(&target, &bytes).map_err(|error| {
        ExportError::WriteFailed(format!("Unable to write export file: {error}"))
    })?;
    Ok(ExportedBundle {
        paper_id: paper.id,
        revision_id: paper.revision_id,
        relative_path: crate::library_paths::normalize_slashes(&format!(
            "{EXPORT_DIR}/{file_name}"
        )),
        byte_size,
        content_digest: sha256_hex(&bytes),
        replaced_existing,
    })
}

/// 撤销的结果。§7 把 Export 的撤销限得很死：只有「本批新建且字节没变」的输出能删，
/// 其余情况一律不动用户的文件，把结论报回去。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UndoOutput {
    /// 已删除。
    Deleted,
    /// 那个位置上已经没有文件了。
    Missing,
    /// 位置上的文件不是我们写下的那一份。
    Changed,
}

/// 删掉本批新建的那一份输出。删除不可逆，所以路径与内容两头都要在这里验：
/// 目标必须是 `export/` 的直接子文件，字节摘要必须还是当初写下去的那一份。
pub(crate) fn undo_output(
    root: &Path,
    relative_path: &str,
    expected_digest: &str,
) -> Result<UndoOutput, ()> {
    let Some(path) = output_path(root, relative_path) else {
        // 记录里的输出路径不在 `export/` 直接子层：那不属于本批管的范围，什么都不删。
        return Ok(UndoOutput::Changed);
    };
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        // 「那个位置上已经没有文件」与「有文件但读不出字节」是两回事：
        // 前者是用户自己清掉了（结果本来就成立），后者要当成不能确认的东西留着。
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(UndoOutput::Missing)
        }
        Err(_) => return Ok(UndoOutput::Changed),
    };
    if sha256_hex(&bytes) != expected_digest {
        return Ok(UndoOutput::Changed);
    }
    fs::remove_file(&path).map_err(|_| ())?;
    Ok(UndoOutput::Deleted)
}

/// 这个导出位置在当前磁盘上对应哪个文件；不在 `export/` 直接子层的一律不算。
/// 计划侧问「会不会盖掉已有文件」用的是这一条：它含存在性判断。
pub(crate) fn output_on_disk(root: &Path, relative_path: &str) -> Option<PathBuf> {
    output_path(root, relative_path).filter(|path| path.is_file())
}

/// 只按形状解出导出文件该在哪，不判断它存不存在——撤销侧要先分清「没有文件」
/// 和「有但不是我们的那一份」。
fn output_path(root: &Path, relative_path: &str) -> Option<PathBuf> {
    let normalized = crate::library_paths::normalize_slashes(relative_path);
    let file_name = normalized.strip_prefix(&format!("{EXPORT_DIR}/"))?;
    // 后面接的是删除，所以形状要一次判完：`.` 与 `..` 不含斜杠却是父目录。
    if file_name.is_empty() || file_name.contains('/') || file_name == "." || file_name == ".." {
        return None;
    }
    Some(export_dir(root).join(file_name))
}

fn export_dir(root: &Path) -> PathBuf {
    root.join(EXPORT_DIR)
}

/// 命名与覆盖策略（§7 要求计划里冻结的那两样）：
/// 主文件名是 PDF 的 stem；同名的另一份内容会加一个父目录前缀，
/// 而「同一份内容的上一次导出」直接复用主文件名——那一次才是覆盖。
fn export_file_name(export_dir: &Path, facts: &NamingFacts) -> String {
    let stem = facts
        .file_name
        .rsplit_once('.')
        .map(|(base, _)| base.to_string())
        .unwrap_or_else(|| facts.file_name.clone());
    let parent = facts
        .collection_path
        .rsplit('/')
        .next()
        .filter(|segment| !segment.is_empty())
        .unwrap_or("Papers");
    let primary = format!("{stem}.md");
    let primary_path = export_dir.join(&primary);
    if !primary_path.exists() || export_file_matches_sha(&primary_path, &facts.sha256) {
        return primary;
    }
    format!("{parent}_{stem}.md")
}

fn export_file_matches_sha(path: &Path, sha: &str) -> bool {
    let Ok(content) = std::fs::read_to_string(path) else {
        return false;
    };
    content.lines().any(|line| line.contains(sha))
}

/// bundle 的正文。逐段拼出来的 Markdown 与旧的单篇命令一致，
/// 段落顺序就是导出文件的阅读顺序，改动要连 `tests` 一起改。
fn label(locale: UiLocale, text: &str) -> &str {
    if locale != UiLocale::En {
        return text;
    }
    match text {
        "学习范围与定位" => "Learning scope",
        "## 术语表\n\n" => "## Glossary\n\n",
        "## 符号表\n\n" => "## Symbols\n\n",
        "## 地图文字大纲\n\n" => "## Map outline\n\n",
        "## 讨论路径\n\n" => "## Discussion paths\n\n",
        "为什么学习这些内容" => "Why learn this",
        "必要基础" => "Prerequisites",
        "知识结构" => "Knowledge structure",
        "核心知识与方法" => "Core knowledge and methods",
        "应形成的能力" => "Mastery goals",
        "后续衔接与应用" => "Connections and applications",
        "分类" => "Classification",
        "背景" => "Context",
        "问题与动机" => "Problem and motivation",
        "核心方法" => "Core method",
        "发现" => "Findings",
        "评价" => "Evaluation",
        "展望" => "Future work",
        "标题" => "Title",
        "作者" => "Authors",
        "年份" => "Year",
        "期刊/会议" => "Venue",
        "摘要" => "Abstract",
        "书名" => "Book title",
        "章节" => "Chapter",
        "（无）\n" => "(None)\n",
        "\n**内容关系**\n\n" => "\n**Relations**\n\n",
        "\n**内容分组**\n\n" => "\n**Groups**\n\n",
        "\n**材料缺口**\n\n" => "\n**Source gaps**\n\n",
        "公式" => "Formula",
        "图表" => "Figure",
        "表格" => "Table",
        _ => text,
    }
}

fn render_bundle(root: &Path, paper: &PaperProjection, locale: UiLocale) -> Result<String, String> {
    let database = db_path(root);
    let artifact_module = ArtifactModule::open(&database)?;
    let outline_module = OutlineModule::open(&database)?;
    let revision_id = &paper.revision_id;
    let kind = crate::library_paths::document_kind_from_relative(&paper.relative_path)
        .unwrap_or(crate::library_paths::DocumentKind::Paper);

    let mut md = String::new();
    md.push_str(&format!("# {}\n\n", paper.title));
    md.push_str(&if locale == UiLocale::En {
        format!(
            "- Kind: {}\n",
            if kind == crate::library_paths::DocumentKind::Textbook {
                "教材"
            } else {
                "论文"
            }
        )
    } else {
        format!(
            "- 种类：{}\n",
            if kind == crate::library_paths::DocumentKind::Textbook {
                "教材"
            } else {
                "论文"
            }
        )
    });
    md.push_str(&if locale == UiLocale::En {
        format!("- Relative path: {}\n", paper.relative_path)
    } else {
        format!("- 相对路径：{}\n", paper.relative_path)
    });
    md.push_str(&if locale == UiLocale::En {
        format!("- Pages: {}\n", paper.page_count.unwrap_or(0))
    } else {
        format!("- 页数：{}\n", paper.page_count.unwrap_or(0))
    });
    md.push_str(&format!("- SHA-256：{}\n", paper.sha256));
    md.push_str(&if locale == UiLocale::En {
        format!("- Exported at: {}\n\n", now())
    } else {
        format!("- 导出时间：{}\n\n", now())
    });

    if let Some(brief) = artifact_module.head_for_revision(revision_id, "brief", "")? {
        md.push_str("## Brief\n\n");
        md.push_str(&render_brief_markdown(&brief.content, locale));
        md.push('\n');
    }
    if let Some(metadata) = artifact_module.head_for_revision(revision_id, "metadata", "")? {
        md.push_str("## Metadata\n\n");
        md.push_str(&render_metadata_markdown(&metadata.content, locale));
        md.push('\n');
    }
    if let Some(glossary) = artifact_module.head_for_revision(revision_id, "glossary", "")? {
        md.push_str(label(locale, "## 术语表\n\n"));
        md.push_str(&render_entries_markdown(&glossary.content, locale));
        md.push('\n');
    }
    if let Some(symbols) = artifact_module.head_for_revision(revision_id, "symbol_table", "")? {
        md.push_str(label(locale, "## 符号表\n\n"));
        md.push_str(&render_entries_markdown(&symbols.content, locale));
        md.push('\n');
    }
    if let Some(graph) = outline_module.current_graph(revision_id)? {
        md.push_str(label(locale, "## 地图文字大纲\n\n"));
        md.push_str(&render_outline_markdown(&graph, locale));
        md.push('\n');
    }

    let lens = artifact_module
        .list(&paper.id)?
        .into_iter()
        .filter(|artifact| {
            matches!(
                artifact.kind.as_str(),
                "lens_formula" | "lens_figure" | "lens_table"
            )
        })
        .collect::<Vec<_>>();
    if !lens.is_empty() {
        md.push_str("## Lens\n\n");
        for artifact in &lens {
            md.push_str(&render_lens_markdown(artifact, locale));
            md.push('\n');
        }
    }

    let threads = recent_discussion_snippets(root, revision_id)?;
    if !threads.is_empty() {
        md.push_str(label(locale, "## 讨论路径\n\n"));
        for (title, content) in threads {
            md.push_str(&format!("### {title}\n\n"));
            if let Some(text) = content {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    let snippet: String = trimmed.chars().take(200).collect();
                    if trimmed.chars().count() > 200 {
                        md.push_str(&format!("{snippet}…\n\n"));
                    } else {
                        md.push_str(&format!("{snippet}\n\n"));
                    }
                }
            }
        }
    }
    Ok(md)
}

/// 最近 5 条讨论的首楼摘要。导出是给没有打开应用的人看的，
/// 所以只带正文前 200 字，不带讨论的内部 id。
fn recent_discussion_snippets(
    root: &Path,
    revision_id: &str,
) -> Result<Vec<(String, Option<String>)>, String> {
    let connection = open_db(root)?;
    let mut statement = connection
        .prepare(
            "SELECT d.title, m.content FROM discussions d
             LEFT JOIN discussion_heads h ON h.discussion_id = d.id
             LEFT JOIN messages m ON m.id = h.message_id
             WHERE d.revision_id = ?1 AND d.status = 'active'
             ORDER BY d.updated_at DESC LIMIT 5",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![revision_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())
}

fn render_brief_markdown(content: &Value, locale: UiLocale) -> String {
    let mut out = String::new();
    if let Some(takeaway) = content.get("takeaway").and_then(Value::as_str) {
        if !takeaway.trim().is_empty() {
            out.push_str(&if locale == UiLocale::En {
                format!("**Takeaway:** {takeaway}\n\n")
            } else {
                format!("**核心摘要：** {takeaway}\n\n")
            });
        }
    }
    if let Some(keywords) = content.get("keywords").and_then(Value::as_array) {
        let tags: Vec<String> = keywords
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        if !tags.is_empty() {
            out.push_str(&if locale == UiLocale::En {
                format!(
                    "**Keywords:** {}\n\n",
                    tags.join(if locale == UiLocale::En { ", " } else { "、" })
                )
            } else {
                format!(
                    "**关键词：** {}\n\n",
                    tags.join(if locale == UiLocale::En { ", " } else { "、" })
                )
            });
        }
    }
    let sections =
        if content["briefProtocol"].as_str() == Some(crate::textbook_contract::BRIEF_PROTOCOL) {
            [
                ("learningScope", label(locale, "学习范围与定位")),
                ("motivation", label(locale, "为什么学习这些内容")),
                ("prerequisites", label(locale, "必要基础")),
                ("knowledgeStructure", label(locale, "知识结构")),
                ("coreKnowledge", label(locale, "核心知识与方法")),
                ("masteryGoals", label(locale, "应形成的能力")),
                ("connections", label(locale, "后续衔接与应用")),
            ]
        } else {
            [
                ("classification", label(locale, "分类")),
                ("context", label(locale, "背景")),
                ("backgroundAndProblem", label(locale, "问题与动机")),
                ("coreMethod", label(locale, "核心方法")),
                ("findings", label(locale, "发现")),
                ("evaluation", label(locale, "评价")),
                ("futureWork", label(locale, "展望")),
            ]
        };
    for (key, label) in sections {
        if let Some(value) = content.get(key).and_then(Value::as_str) {
            if !value.trim().is_empty() {
                out.push_str(&format!("**{label}：**\n\n{value}\n\n"));
            }
        }
    }
    out
}

fn render_metadata_markdown(content: &Value, locale: UiLocale) -> String {
    let mut out = String::new();
    for (key, label) in [
        ("title", label(locale, "标题")),
        ("authors", label(locale, "作者")),
        ("publicationYear", label(locale, "年份")),
        ("venue", label(locale, "期刊/会议")),
        ("doi", "DOI"),
        ("abstract", label(locale, "摘要")),
        ("bookName", label(locale, "书名")),
        ("isbn", "ISBN"),
        ("chapterNumber", label(locale, "章节")),
    ] {
        if let Some(value) = content.get(key) {
            let text = match value {
                Value::Array(items) => items
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", "),
                other => other.as_str().unwrap_or("").to_string(),
            };
            if !text.trim().is_empty() {
                out.push_str(&format!("- **{label}：** {text}\n"));
            }
        }
    }
    out.push('\n');
    out
}

fn render_entries_markdown(content: &Value, locale: UiLocale) -> String {
    let mut out = String::new();
    if let Some(entries) = content.get("entries").and_then(Value::as_array) {
        for entry in entries {
            if let Some(obj) = entry.as_object() {
                let term = obj
                    .get("term")
                    .or_else(|| obj.get("symbol"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let definition = obj
                    .get("definition")
                    .or_else(|| obj.get("meaning"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                out.push_str(&format!("- **{term}**：{definition}\n"));
            }
        }
    }
    if out.is_empty() {
        out.push_str(label(locale, "（无）\n"));
    }
    out
}

fn render_outline_markdown(graph: &Value, locale: UiLocale) -> String {
    let mut out = String::new();
    if let Some(title) = graph["title"].as_str() {
        out.push_str(&format!("**{title}**\n\n"));
    }
    if let Some(summary) = graph["summary"].as_str() {
        out.push_str(&format!("{summary}\n\n"));
    }
    let nodes = graph["nodes"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let titles: std::collections::HashMap<_, _> = nodes
        .iter()
        .filter_map(|n| Some((n["nodeId"].as_str()?, n["title"].as_str()?)))
        .collect();
    for node in nodes {
        let title = node["title"].as_str().unwrap_or("");
        let takeaway = node["takeaway"].as_str().unwrap_or("");
        let role = node["roleLabel"]
            .as_str()
            .filter(|s| !s.is_empty())
            .or_else(|| node["roleClass"].as_str())
            .unwrap_or("");
        if role.is_empty() {
            out.push_str(&format!("- {title}：{takeaway}\n"));
        } else {
            out.push_str(&format!("- [{role}] {title}：{takeaway}\n"));
        }
        render_outline_details(&mut out, node, locale);
    }
    if let Some(edges) = graph["edges"].as_array().filter(|v| !v.is_empty()) {
        out.push_str(label(locale, "\n**内容关系**\n\n"));
        for edge in edges {
            let source = edge["sourceNodeId"].as_str().unwrap_or("");
            let target = edge["targetNodeId"].as_str().unwrap_or("");
            let label = edge["label"].as_str().unwrap_or("");
            let from = titles.get(source).copied().unwrap_or(source);
            let to = titles.get(target).copied().unwrap_or(target);
            let arrow = if edge["direction"].as_str() == Some("undirected") {
                "—"
            } else {
                "→"
            };
            out.push_str(&format!("- {from} — {label} {arrow} {to}\n"));
            if let Some(reason) = edge["rationale"].as_str().filter(|s| !s.is_empty()) {
                out.push_str(&if locale == UiLocale::En {
                    format!("  Rationale: {reason}\n")
                } else {
                    format!("  关系说明：{reason}\n")
                });
            }
            render_outline_details(&mut out, edge, locale);
        }
    }
    if let Some(groups) = graph["groups"].as_array().filter(|v| !v.is_empty()) {
        out.push_str(label(locale, "\n**内容分组**\n\n"));
        for group in groups {
            let members = group["nodeIds"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|id| titles.get(id).copied().unwrap_or(id))
                .collect::<Vec<_>>()
                .join("、");
            out.push_str(&format!(
                "- {}：{members}\n",
                group["title"].as_str().unwrap_or("")
            ));
        }
    }
    if let Some(gaps) = graph["gaps"].as_array().filter(|v| !v.is_empty()) {
        out.push_str(label(locale, "\n**材料缺口**\n\n"));
        for gap in gaps {
            out.push_str(&format!(
                "- {}\n",
                gap["description"].as_str().unwrap_or("")
            ));
        }
    }
    out.push('\n');
    out
}

fn render_outline_details(out: &mut String, item: &Value, locale: UiLocale) {
    if let Some(uncertainty) = item["uncertainty"].as_str().filter(|s| !s.is_empty()) {
        out.push_str(&if locale == UiLocale::En {
            format!("  Limitations and uncertainty: {uncertainty}\n")
        } else {
            format!("  限制与不确定性：{uncertainty}\n")
        });
    }
    if let Some(references) = item["references"].as_array() {
        for reference in references {
            let page = reference["pageNumber"]
                .as_i64()
                .map(|p| {
                    if locale == UiLocale::En {
                        format!("Page {p}")
                    } else {
                        format!("第 {p} 页")
                    }
                })
                .unwrap_or_default();
            let block = reference["blockId"]
                .as_str()
                .map(|id| {
                    if locale == UiLocale::En {
                        format!("Block {id}")
                    } else {
                        format!("区块 {id}")
                    }
                })
                .unwrap_or_default();
            let location = [page, block]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("，");
            out.push_str(&if locale == UiLocale::En {
                format!(
                    "  Source: {location}; {}\n",
                    reference["purpose"].as_str().unwrap_or("")
                )
            } else {
                format!(
                    "  原文：{location}；{}\n",
                    reference["purpose"].as_str().unwrap_or("")
                )
            });
        }
    }
}

fn render_lens_markdown(artifact: &ArtifactProjection, locale: UiLocale) -> String {
    let kind_label = match artifact.kind.as_str() {
        "lens_formula" => label(locale, "公式"),
        "lens_figure" => label(locale, "图表"),
        "lens_table" => label(locale, "表格"),
        _ => "Lens",
    };
    let mut out = String::new();
    let pages: Vec<String> = artifact
        .evidence
        .iter()
        .map(|evidence| evidence.page_number.to_string())
        .collect();
    let page_suffix = if pages.is_empty() {
        String::new()
    } else {
        if locale == UiLocale::En {
            format!(
                " (pages {})",
                pages.join(if locale == UiLocale::En { ", " } else { "、" })
            )
        } else {
            format!(
                "（第 {} 页）",
                pages.join(if locale == UiLocale::En { ", " } else { "、" })
            )
        }
    };
    out.push_str(&format!("### {kind_label} Lens{page_suffix}\n\n"));
    if let Some(takeaway) = artifact
        .content
        .get("quickTakeaway")
        .and_then(|value| value.get("markdown"))
        .and_then(Value::as_str)
    {
        out.push_str(takeaway);
        out.push_str("\n\n");
    }
    if let Some(sections) = artifact.content.get("sections").and_then(Value::as_array) {
        for section in sections {
            if let Some(obj) = section.as_object() {
                if let (Some(title), Some(markdown)) = (
                    obj.get("title").and_then(Value::as_str),
                    obj.get("markdown").and_then(Value::as_str),
                ) {
                    out.push_str(&format!("**{title}**\n\n{markdown}\n\n"));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn english_export_translates_labels_without_rewriting_historical_prose() {
        let body = serde_json::json!({"takeaway":"历史中文正文：用户自定义内容", "findings":"原文保留", "keywords":["中文标签"]});
        let before = body.clone();
        let text = super::render_brief_markdown(&body, super::UiLocale::En);
        assert!(text.contains("Takeaway"));
        assert!(text.contains("Findings"));
        assert!(text.contains("历史中文正文：用户自定义内容"));
        assert!(text.contains("中文标签"));
        assert!(!text.contains("核心摘要"));
        assert_eq!(body, before);
    }

    use super::*;

    fn export_dir_fixture() -> tempfile::TempDir {
        let directory = tempfile::tempdir().expect("export directory");
        fs::create_dir_all(directory.path().join(EXPORT_DIR)).expect("export dir");
        directory
    }

    fn facts(file_name: &str, collection_path: &str, sha256: &str) -> NamingFacts {
        NamingFacts {
            file_name: file_name.to_string(),
            collection_path: collection_path.to_string(),
            sha256: sha256.to_string(),
        }
    }

    #[test]
    fn export_target_path_handles_collision_by_sha() {
        let directory = export_dir_fixture();
        let dir = directory.path().join(EXPORT_DIR);
        let primary = dir.join("attention.md");
        fs::write(&primary, "# t\n- SHA-256：abc123\n").expect("primary export");
        // Same sha -> primary (re-export overwrites in place).
        assert_eq!(
            export_file_name(&dir, &facts("attention.pdf", "Architectures", "abc123")),
            "attention.md"
        );
        // Different sha -> parent_stem fallback.
        assert_eq!(
            export_file_name(&dir, &facts("attention.pdf", "Architectures", "def456")),
            "Architectures_attention.md"
        );
        // No existing file -> primary.
        assert_eq!(
            export_file_name(&dir, &facts("fresh.pdf", "Papers", "aaa")),
            "fresh.md"
        );
    }

    #[test]
    fn export_renderers_include_brief_text_and_no_bbox_json() {
        let brief = serde_json::json!({
            "takeaway": "Transformer 核心突破",
            "keywords": ["Transformer", "Attention"],
            "classification": "理论"
        });
        let md = render_brief_markdown(&brief, UiLocale::ZhCn);
        assert!(md.contains("Transformer 核心突破"));
        assert!(md.contains("关键词"));
        assert!(!md.contains("bbox"));
        assert!(!md.contains("region"));

        let lens = ArtifactProjection {
            id: "lens-1".to_string(),
            paper_id: "p1".to_string(),
            revision_id: "r1".to_string(),
            ocr_revision_id: None,
            kind: "lens_formula".to_string(),
            object_key: String::new(),
            version: 1,
            status: "ready".to_string(),
            content: serde_json::json!({
                "quickTakeaway": {"title": "公式速览", "markdown": "E = mc^2"},
                "sections": [
                    {"sectionId": "s1", "title": "含义", "markdown": "质能等价"}
                ]
            }),
            overrides: serde_json::json!({}),
            evidence: vec![crate::artifact_module::EvidenceAnchor {
                revision_id: "r1".to_string(),
                page_number: 7,
                block_id: None,
                bbox: None,
                excerpt: None,
            }],
            dependency_snapshot: serde_json::json!({}),
            provider_node_id: None,
            created_at: "2026-08-23T00:00:00Z".to_string(),
        };
        let lens_md = render_lens_markdown(&lens, UiLocale::ZhCn);
        assert!(lens_md.contains("第 7 页"));
        assert!(lens_md.contains("质能等价"));
        assert!(!lens_md.contains("bbox"));
    }
    #[test]
    fn outline_export_preserves_free_relations_direction_references_and_limits() {
        let graph = serde_json::json!({"title":"地图","summary":"说明","nodes":[{"nodeId":"n1","title":"设计","takeaway":"具体方法"},{"nodeId":"n2","title":"条件","takeaway":"适用条件"}],"edges":[{"edgeId":"e","sourceNodeId":"n2","targetNodeId":"n1","direction":"directed","label":"限制其适用范围","rationale":"噪声实验提供边界","uncertainty":"不能推广到所有情境","references":[{"pageNumber":12,"purpose":"附录依据"}]}],"gaps":[{"description":"缺少外部验证"}]});
        let text = render_outline_markdown(&graph, UiLocale::ZhCn);
        for expected in [
            "条件 — 限制其适用范围 → 设计",
            "噪声实验提供边界",
            "不能推广到所有情境",
            "第 12 页",
            "附录依据",
            "缺少外部验证",
        ] {
            assert!(text.contains(expected), "{expected}");
        }
        let mut undirected = graph;
        undirected["edges"][0]["direction"] = serde_json::json!("undirected");
        assert!(!render_outline_markdown(&undirected, UiLocale::ZhCn).contains('→'));
    }
    #[test]
    fn textbook_brief_export_uses_learning_sections_and_preserves_math() {
        let body = serde_json::json!({"briefProtocol":"textbook-v2","takeaway":"条件关系","keywords":["概率"],"learningScope":"整本中的当前节选","motivation":"更新认识","prerequisites":"条件概率","knowledgeStructure":"定义连接到结论","coreKnowledge":"保留 $P(A|B)$ 的条件","masteryGoals":"能辨认条件方向","connections":"后续应用"});
        let md = render_brief_markdown(&body, UiLocale::ZhCn);
        assert!(md.contains("学习范围与定位"));
        assert!(md.contains("应形成的能力"));
        assert!(md.contains("$P(A|B)$"));
        assert!(!md.contains("**发现"));
        assert!(!md.contains("**评价"));
    }
}
