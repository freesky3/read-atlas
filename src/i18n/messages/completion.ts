// Application-owned UI text. Keep user content and generated prose unchanged.
export const completionEntries: Record<string, readonly [string, string]> = {
  "legacy_publication_year": ["旧记录·出版年份", "Legacy publication year"],
  "metadata_source_0": ["人工修改","Manually edited"],
  "metadata_source_1": ["旧记录中的人工值","Manual value from a legacy record"],
  "metadata_source_2": ["待核对","Needs verification"],
  "metadata_source_3": ["本次提取","Current extraction"],
  "metadata_source_4": ["未提供","Not provided"],
  "metadata_source_5": ["旧记录","Legacy record"],
  "metadata_source_6": ["年份待核对","Year needs verification"],
  "metadata_source_7": ["出版","Publication"],
  "metadata_source_8": ["印刷出版","Print publication"],
  "metadata_source_9": ["在线发表","Online publication"],
  "metadata_source_10": ["发布","Release"],
  "metadata_source_11": ["人工年份","Manual year"],
  "metadata_source_12": ["人工选择的日期","Manually selected date"],
  "metadata_source_13": ["所选日期待核对","Selected date needs verification"],
  "metadata_source_14": ["旧记录中的人工年份","Manual year from a legacy record"],
  "metadata_source_15": ["旧记录年份","Legacy year"],
  "metadata_source_16": ["人工选择的原文摘要","Manually selected source abstract"],
  "metadata_source_17": ["所选摘要待核对","Selected abstract needs verification"],
  "metadata_source_18": ["所属出版物·出版","Container · Publication"],
  "metadata_source_19": ["所属出版物·印刷出版","Container · Print publication"],
  "metadata_source_20": ["所属出版物·在线发表","Container · Online publication"],
  "metadata_source_21": ["所属出版物·发布","Container · Release"],

  "active_pages": ["页面 {start}–{end} 保持加载", "Pages {start}–{end} kept active"],
  "operation_error_details": ["操作错误详情", "Operation error details"],
  "original_diagnostic_details": ["以下为原始技术信息，仅保存在当前会话中。", "Original technical details, kept only for this session."],
  "order_unchanged": ["顺序未变化", "Order unchanged"],
  "content_source": ["内容出处", "Source evidence"],
  "active_work_safe_exit": [
    "运行任务 / 安全退出",
    "ACTIVE WORK / SAFE EXIT"
  ],
  "tasks_are_still_in_progress": [
    "仍有任务正在进行。",
    "Tasks are still in progress."
  ],
  "choose_whether_read_desktop_should_keep_working_preserve_recoverable_queue_state_or_cancel_active_work_before_exiting": [
    "选择继续在后台工作、保留可恢复任务，或取消任务后退出。",
    "Choose whether Read Atlas should keep working, preserve recoverable queue state, or cancel active work before exiting."
  ],
  "running": [
    "运行中",
    "running"
  ],
  "queued": [
    "排队中",
    "queued"
  ],
  "paused": [
    "已暂停",
    "paused"
  ],
  "visible_streaming_chat": [
    "正在输出的对话",
    "visible streaming chat"
  ],
  "continue_in_tray": [
    "在托盘中继续",
    "Continue in tray"
  ],
  "hide_the_window_and_let_current_work_finish": [
    "隐藏窗口，继续完成当前任务。",
    "Hide the window and let current work finish."
  ],
  "pause_and_exit": [
    "暂停并退出",
    "Pause and exit"
  ],
  "pause_safe_queue_work_paid_requests_with_unknown_outcomes_become_interrupted_unknown_and_are_never_retried_silently": [
    "暂停可安全停止的任务。结果尚不确定的付费请求会标为结果未知，不会自动重试。",
    "Pause safe queue work. Paid requests with unknown outcomes become interrupted_unknown and are never retried silently."
  ],
  "cancel_and_exit": [
    "取消并退出",
    "Cancel and exit"
  ],
  "keep_partial_chat_text_but_cancel_active_tasks": [
    "保留已输出的对话文字，取消当前任务。",
    "Keep partial chat text, but cancel active tasks."
  ],
  "keep_the_window_open": [
    "保持窗口打开",
    "Keep the window open"
  ],
  "page_and_zoom": [
    "页码与缩放",
    "Page and zoom"
  ],
  "previous_page": [
    "上一页",
    "Previous page"
  ],
  "page_number": [
    "页码",
    "Page number"
  ],
  "next_page": [
    "下一页",
    "Next page"
  ],
  "zoom_out": [
    "缩小",
    "Zoom out"
  ],
  "zoom_in": [
    "放大",
    "Zoom in"
  ],
  "fit_width": [
    "适合宽度",
    "Fit width"
  ],
  "fit_page": [
    "适合页面",
    "Fit page"
  ],
  "rotate": [
    "旋转",
    "Rotate"
  ],
  "loading_pdf_renderer": [
    "正在加载 PDF 阅读器…",
    "Loading PDF renderer…"
  ],
  "pages": [
    "页面",
    "Pages"
  ],
  "resize_discussion_panel": [
    "调整讨论面板宽度",
    "Resize discussion panel"
  ],
  "model_connection": [
    "模型连接",
    "MODEL CONNECTION"
  ],
  "add_a_key_fetch_the_models_available_to_it_and_choose_the_default_for_this_app": [
    "添加密钥，获取可用模型，再选择应用默认模型。",
    "Add a key, fetch the models available to it, and choose the default for this app."
  ],
  "open_model_configuration": [
    "打开模型配置",
    "Open model configuration"
  ],
  "what_problem_does_this_paper_solve": [
    "这篇论文解决了什么问题？",
    "What problem does this paper solve?"
  ],
  "walk_me_through_the_method": [
    "请带我理解论文的方法。",
    "Walk me through the method."
  ],
  "what_should_i_be_skeptical_about": [
    "这篇论文有哪些值得质疑的地方？",
    "What should I be skeptical about?"
  ],
  "quoted_ocr_blocks": [
    "引用的 OCR 区块",
    "Quoted OCR Blocks"
  ],
  "quoted_blocks": [
    "引用区块",
    "QUOTED BLOCKS"
  ],
  "32_persisted_until_sent": [
    "/32 · 发送前保留",
    "/32 · persisted until sent"
  ],
  "loading_reading_results": [
    "正在加载阅读成果…",
    "Loading reading results…"
  ],
  "context_inspector": [
    "上下文查看器",
    "CONTEXT INSPECTOR"
  ],
  "what_the_model_sees": [
    "模型看到的内容",
    "What the model sees"
  ],
  "context_epoch": [
    "上下文版本",
    "CONTEXT EPOCH"
  ],
  "stable_prefix_remains_cacheable": [
    "稳定前缀仍可缓存",
    "Stable prefix remains cacheable"
  ],
  "native_pdf": [
    "原生 PDF",
    "Native PDF"
  ],
  "you_control_the_question_and_the_evidence_anchor_the_stable_instruction_schema_and_cache_checkpoint_are_managed_by_the_adapter": [
    "你决定问题和证据位置，应用负责维护固定指令、输出结构与缓存。",
    "You control the question and the evidence anchor. The stable instruction, schema, and cache checkpoint are managed by the adapter."
  ],
  "image_preview": [
    "图片预览",
    "Image preview"
  ],
  "waiting_for_the_first_token": [
    "正在等待模型开始回答…",
    "Waiting for the first token…"
  ],
  "quoted_block_snapshots": [
    "引用区块快照",
    "Quoted Block snapshots"
  ],
  "in": [
    "输入",
    "in"
  ],
  "uncached": [
    "未缓存",
    "uncached"
  ],
  "cached": [
    "已缓存",
    "cached"
  ],
  "out": [
    "输出",
    "out"
  ],
  "reasoning": [
    "思考",
    "reasoning"
  ],
  "summary": [
    "摘要",
    "Summary"
  ],
  "method": [
    "方法",
    "Method"
  ],
  "limitations": [
    "局限",
    "Limitations"
  ],
  "research_question": [
    "研究问题",
    "Research question"
  ],
  "brief": [
    "简报",
    "BRIEF"
  ],
  "outline": [
    "地图",
    "Outline"
  ],
  "working": [
    "处理中",
    "working"
  ],
  "selected_block": [
    "选中区块",
    "Selected block"
  ],
  "evidence": [
    "证据",
    "EVIDENCE"
  ],
  "lens_questions": [
    "Lens 追问",
    "LENS QUESTIONS"
  ],
  "isolated_from_discussion": [
    "与讨论独立",
    "isolated from Discussion"
  ],
  "figure_preview": [
    "图片预览",
    "Figure preview"
  ],
  "table_preview": [
    "表格预览",
    "Table preview"
  ],
  "enlarged_crop_preview": [
    "选区放大预览",
    "Enlarged crop preview"
  ],
  "operations_view": [
    "任务中心视图",
    "Operations view"
  ],
  "tasks": [
    "任务",
    "Tasks"
  ],
  "storage": [
    "存储",
    "Storage"
  ],
  "trash": [
    "回收站",
    "Trash"
  ],
  "diagnostics": [
    "诊断",
    "Diagnostics"
  ],
  "raise_priority": [
    "↑ 提高优先级",
    "↑ Priority"
  ],
  "lower_priority": [
    "↓ 降低优先级",
    "↓ Priority"
  ],
  "pause": [
    "暂停",
    "Pause"
  ],
  "resume": [
    "继续",
    "Resume"
  ],
  "cancel": [
    "取消",
    "Cancel"
  ],
  "remote_cleanup": [
    "远端清理",
    "REMOTE CLEANUP"
  ],
  "provider_files_and_interactions_queued_by_paper_deletion": [
    "删除文档后待清理的服务商文件与交互记录",
    "Provider files and interactions queued by Paper deletion"
  ],
  "attempt": [
    "尝试",
    "attempt"
  ],
  "retry_cleanup": [
    "重试清理",
    "Retry cleanup"
  ],
  "resolved": [
    "已解决",
    "resolved"
  ],
  "workspace_footprint": [
    "工作区存储",
    "WORKSPACE FOOTPRINT"
  ],
  "source_pdfs": [
    "原始 PDF",
    "source PDFs"
  ],
  "internal_data": [
    "内部数据",
    "internal data"
  ],
  "internal_per_paper_values_are_logical_payload_sizes_not_invented_physical_sqlite_page_allocation_workspace_total_is_real_disk_use": [
    "单篇文档的内部数据按逻辑大小统计；工作区总量为实际磁盘占用。",
    "Internal per-paper values are logical payload sizes, not invented physical SQLite page allocation. Workspace total is real disk use."
  ],
  "artifacts": [
    "阅读成果",
    "Artifacts"
  ],
  "discussion": [
    "讨论",
    "Discussion"
  ],
  "redacted_diagnostics": [
    "脱敏诊断",
    "REDACTED DIAGNOSTICS"
  ],
  "preview_the_scope_before_writing_a_package": [
    "导出诊断包前先预览范围。",
    "Preview the scope before writing a package."
  ],
  "read_desktop_exports_aggregate_state_only_it_does_not_export_paper_content_or_identities": [
    "诊断包仅导出汇总状态，不包含论文内容或身份信息。",
    "Read Atlas exports aggregate state only. It does not export paper content or identities."
  ],
  "preview_scope": [
    "预览范围",
    "Preview scope"
  ],
  "export_json": [
    "导出 JSON…",
    "Export JSON…"
  ],
  "included": [
    "包含内容",
    "Included"
  ],
  "explicitly_excluded": [
    "排除内容",
    "Explicitly excluded"
  ],
  "previewed": [
    "已预览",
    "Previewed"
  ],
  "no_diagnostic_data_has_been_read_yet": [
    "尚未读取诊断数据",
    "No diagnostic data has been read yet"
  ],
  "preview_is_explicit_and_does_not_write_a_file": [
    "点击预览才会读取诊断数据，不写入文件。",
    "Preview is explicit and does not write a file."
  ],
  "trash_is_empty": [
    "回收站为空",
    "Trash is empty"
  ],
  "deleted_papers_remain_recoverable_for_30_days": [
    "删除的文档可在 30 天内恢复。",
    "Deleted papers remain recoverable for 30 days."
  ],
  "purge_after": [
    "· 永久清理日期",
    "· purge after"
  ],
  "restore": [
    "恢复",
    "Restore"
  ],
  "kept_active": [
    "保持加载",
    "kept active"
  ],
  "prompt_slots_list": [
    "提示词槽位列表",
    "Prompt slots list"
  ],
  "min": [
    "分钟",
    "min"
  ],
  "generate_local_map": [
    "生成局部关系图",
    "Generate local map"
  ],
  "generate_content_map": [
    "生成内容关系图",
    "Generate content map"
  ],
  "generate_argument_map": [
    "生成论证地图",
    "Generate argument map"
  ],
  "local_composition": [
    "局部构图",
    "Local composition"
  ],
  "draft_review": [
    "整体构图 + 检查与定稿",
    "Draft + review"
  ],
  "initialize_document_context": [
    " + 初始化文档根",
    " + initialize document context"
  ],
  "workflow_root_calls_calls_at_most_repairs_repairs": [
    "{workflow}{root}，共 {calls} 次调用，全程最多 {repairs} 次修复",
    "{workflow}{root}, {calls} calls, at most {repairs} repairs"
  ],
  "calls_calls_at_most_repairs_repairs": [
    "{calls} 次调用 + 最多 {repairs} 次 repair",
    "{calls} calls + at most {repairs} repairs"
  ],
  "unknown": [
    "未知",
    "Unknown"
  ],
  "count_pages": [
    "{count} 页",
    "{count} pages"
  ],
  "complete_ocr_first": [
    "先完成 OCR",
    "Complete OCR first"
  ],
  "the_map_needs_published_ocr_blocks_to_locate_evidence_run_full_document_ocr_from_the_toolbar_first": [
    "论证地图需要已发布的 OCR Block 才能定位证据。请先在顶栏运行整篇 OCR。",
    "The map needs published OCR blocks to locate evidence. Run full-document OCR from the toolbar first."
  ],
  "document_context_required": [
    "需要文档根",
    "Document context required"
  ],
  "initialize_the_document_context_then_return_to_confirm_the_map_plan": [
    "地图从文档根旁支生成。请先初始化文档上下文（与首次 Chat / Lens 相同），成功后再回到这里确认计划。",
    "Initialize the document context, then return to confirm the map plan."
  ],
  "model_context_window_exceeded": [
    "超出模型窗口",
    "Model context window exceeded"
  ],
  "the_pdf_and_ocr_catalog_exceed_this_model_s_context_window_no_pages_will_be_truncated_and_no_paid_job_will_be_queued": [
    "当前 PDF 与 OCR 目录估计会超过阅读模型上下文。不会截断中间页，也不会入队付费任务。",
    "The PDF and OCR catalog exceed this model’s context window. No pages will be truncated and no paid job will be queued."
  ],
  "native_pdf_is_not_supported": [
    "模型不支持原生 PDF",
    "Native PDF is not supported"
  ],
  "choose_a_reading_model_that_supports_native_pdf_then_plan_the_map_again": [
    "请更换支持原生 PDF 的阅读模型后再计划 Outline。",
    "Choose a reading model that supports native PDF, then plan the map again."
  ],
  "tags_updated": [
    "已更新标签",
    "Tags updated"
  ],
  "moved": [
    "已移动",
    "Moved"
  ],
  "moved_to_trash": [
    "已移入回收站",
    "Moved to trash"
  ],
  "imported": [
    "已导入",
    "Imported"
  ],
  "exported": [
    "已导出",
    "Exported"
  ],
  "retried": [
    "已重试",
    "Retried"
  ],
  "reading_status_updated": [
    "已更新阅读状态",
    "Reading status updated"
  ],
  "ocr_queued": [
    "已排队 OCR",
    "OCR queued"
  ],
  "brief_queued": [
    "已排队 Brief",
    "Brief queued"
  ],
  "undone": [
    "已撤销",
    "Undone"
  ],
  "verb_count_documents": [
    "{verb} {count} 篇",
    "{verb} {count} documents"
  ],
  "verb_count_documents_skipped_skipped": [
    "{verb} {count} 篇；{skipped} 篇跳过",
    "{verb} {count} documents; {skipped} skipped"
  ],
  "verb_count_documents_failed_failed_total_items": [
    "{verb} {count} 篇；{failed} 篇失败（共 {total} 项）",
    "{verb} {count} documents; {failed} failed ({total} items)"
  ],
  "complete_ocr_before_generating_margin_notes": [
    "先完成 OCR 再生成旁批",
    "Complete OCR before generating margin notes"
  ],
  "margin_notes_are_generating_cancel_them_in_the_job_center": [
    "旁批正在生成，可在任务中心取消",
    "Margin notes are generating; cancel them in the job center"
  ],
  "margin_note_planning_failed_error": [
    "旁批计划失败 · {error}",
    "Margin-note planning failed · {error}"
  ],
  "search_results_are_not_a_complete_order_clear_the_search_before_reordering": [
    "搜索结果不是完整顺序，请清除搜索后排序",
    "Search results are not a complete order. Clear the search before reordering."
  ],
  "switch_to_manual_sorting_to_reorder_by_dragging": [
    "切换到「手动」排序后才能拖拽改序",
    "Switch to manual sorting to reorder by dragging."
  ],
  "this_view_includes_subfolders_open_the_folder_to_reorder_its_documents": [
    "当前视图包含子目录文档，不是完整单层顺序，请进入该文件夹",
    "This view includes subfolders. Open the folder to reorder its documents."
  ],
  "use_move_to_to_move_between_papers_and_textbooks": [
    "不能跨 Papers / Textbooks 拖拽，请使用「移动到……」",
    "Use Move to… to move between Papers and Textbooks."
  ],
  "already_in_this_folder": [
    "已经在该目录中",
    "Already in this folder"
  ],
  "a_folder_cannot_be_moved_into_itself_or_its_descendants": [
    "不能把目录移动到自身或其子目录内",
    "A folder cannot be moved into itself or its descendants."
  ],
  "a_smart_collection_is_a_saved_query_not_a_folder": [
    "智能集合由规则生成，不是目录",
    "A smart collection is a saved query, not a folder."
  ],
  "smart_collections_cannot_be_manually_reordered": [
    "智能集合由规则生成，不能手排",
    "Smart collections cannot be manually reordered."
  ],
  "moving_between_roots_changes_the_document_kind_confirm_using_move_to": [
    "跨根移动会改变文献种类，请使用「移动到……」确认",
    "Moving between roots changes the document kind. Confirm using Move to…."
  ],
  "use_the_batch_toolbar_for_all_matching_results": [
    "「全选当前结果」是逻辑选择，请从批量工具栏执行",
    "Use the batch toolbar for all matching results."
  ],
  "select_a_physical_folder_on_the_left_before_dropping": [
    "请先在左侧选择一个物理目录再拖放",
    "Select a physical folder on the left before dropping."
  ],
  "select_a_folder_under_papers_or_textbooks_first": [
    "请先选择 Papers 或 Textbooks 下的目录",
    "Select a folder under Papers or Textbooks first"
  ],
  "select_a_physical_folder_under_papers_or_textbooks_to_choose_the_import_destination": [
    "当前不是物理目录，不能猜测导入位置。请先选中 Papers 或 Textbooks 下的文件夹。",
    "Select a physical folder under Papers or Textbooks to choose the import destination."
  ],
  "no_source_paths_to_import_folders_and_non_pdf_files_are_recorded_as_skipped_items": [
    "没有可导入的路径。目录和非 PDF 会记为逐项跳过，不会整批消失。",
    "No source paths to import. Folders and non-PDF files are recorded as skipped items."
  ],
  "import_at_most_count_source_files_at_a_time": [
    "一次最多 {count} 个源文件，请分批导入。",
    "Import at most {count} source files at a time."
  ],
  "filters_changed_the_previous_selection_was_cleared": [
    "筛选已变化，已清空旧选择",
    "Filters changed; the previous selection was cleared"
  ],
  "this_view_has_no_physical_folder_to_reorder": [
    "当前视图没有可改序的物理目录",
    "This view has no physical folder to reorder"
  ],
  "reordering_is_unavailable": [
    "改序尚未接通",
    "Reordering is unavailable"
  ],
  "reorder_failed_error": [
    "改序失败：{error}",
    "Reorder failed: {error}"
  ],
  "use_the_arrow_keys_to_focus_a_document_in_this_folder_first": [
    "请先用方向键聚焦到本层文档",
    "Use the arrow keys to focus a document in this folder first"
  ],
  "already_first": [
    "已经在最前面",
    "Already first"
  ],
  "already_last": [
    "已经在最后面",
    "Already last"
  ],
  "batch_operations_require_an_open_workspace": [
    "批量操作需要已打开的 Workspace 与 library_act 持久 Batch",
    "Batch operations require an open workspace"
  ],
  "reading_status_requires_an_open_workspace": [
    "阅读状态需要 Workspace 的 library_act 接线",
    "Reading status requires an open workspace"
  ],
  "this_selection_needs_a_current_result_snapshot_before_it_can_be_submitted": [
    "「全选当前筛选结果」是逻辑选择，提交需要 Selection Snapshot；当前查询还没有 hub_page 快照",
    "This selection needs a current result snapshot before it can be submitted"
  ],
  "question_context": [
    "问题背景",
    "Question context"
  ],
  "core_claim": [
    "核心主张",
    "Core claim"
  ],
  "concept_theory": [
    "概念理论",
    "Concept / theory"
  ],
  "method_design": [
    "方法设计",
    "Method design"
  ],
  "mechanism_process": [
    "机制过程",
    "Mechanism / process"
  ],
  "evidence_evaluation": [
    "证据评估",
    "Evidence / evaluation"
  ],
  "result_finding": [
    "结果发现",
    "Result / finding"
  ],
  "conclusion_implication": [
    "结论启示",
    "Conclusion / implication"
  ],
  "limitations_boundaries": [
    "局限边界",
    "Limitations / boundaries"
  ],
  "concept_introduction": [
    "概念引入",
    "Concept introduction"
  ],
  "definition": [
    "定义",
    "Definition"
  ],
  "worked_example": [
    "例题演示",
    "Worked example"
  ],
  "derivation": [
    "推导",
    "Derivation"
  ],
  "algorithm_procedure": [
    "算法流程",
    "Algorithm / procedure"
  ],
  "exercise": [
    "练习",
    "Exercise"
  ],
  "common_pitfalls": [
    "易错提醒",
    "Common pitfalls"
  ],
  "application_example": [
    "应用举例",
    "Application example"
  ],
  "recap": [
    "小结",
    "Recap"
  ],
  "supporting_evidence": [
    "内容依据",
    "Supporting evidence"
  ],
  "relation_evidence": [
    "关系依据",
    "Relation evidence"
  ],
  "block_id": [
    "块 {id}",
    "Block {id}"
  ],
  "page_reference_p_page": [
    "页级定位 · p.{page}",
    "Page reference · p.{page}"
  ],
  "no_location": [
    "无定位",
    "No location"
  ],
  "node_title": [
    "节点：{title}",
    "Node: {title}"
  ],
  "relation_label": [
    "关系：{label}",
    "Relation: {label}"
  ],
  "provider_is_not_configured_open_settings_to_continue": [
    "尚未配置 {provider} · 请打开设置继续",
    "{provider} is not configured · open Settings to continue"
  ],
  "no_active_provider_selected": [
    "尚未选择服务商",
    "No active provider selected"
  ],
  "provider_ready_model": [
    "{provider} 已就绪 · {model}",
    "{provider} ready · {model}"
  ],
  "provider_credential_cleared": [
    "{provider} 凭据已清除",
    "{provider} credential cleared"
  ],
  "configure_provider_to_ask_this_paper": [
    "配置 {provider} 后即可提问…",
    "Configure {provider} to ask this paper…"
  ],
  "connect_provider_to_begin": [
    "连接 {provider} 后开始阅读。",
    "Connect {provider} to begin."
  ],
  "pdf_chat_is_paused_until_a_verified_provider_configuration_is_active": [
    "请先完成 {provider} 配置与验证，再开始 PDF 对话。",
    "PDF Chat is paused until a verified {provider} configuration is active."
  ],
  "provider_is_not_configured": [
    "尚未配置 {provider}",
    "{provider} is not configured"
  ],
  "provider_not_configured_action": [
    "尚未配置 {provider}（{action}）",
    "{provider} not configured ({action})"
  ],
  "ai_model": [
    "AI 模型",
    "AI Model"
  ],
  "click_to_configure": [
    "点击配置",
    "click to configure"
  ],
  "imported_this_week_unread": [
    "本周导入但未读",
    "Imported this week, unread"
  ],
  "reading": [
    "阅读中",
    "Reading"
  ],
  "read_later": [
    "稍后阅读",
    "Read later"
  ],
  "needs_review": [
    "需要复习",
    "Needs review"
  ],
  "ocr_failed": [
    "OCR 失败",
    "OCR failed"
  ],
  "recently_opened": [
    "最近打开",
    "Recently opened"
  ],
  "unread": [
    "未读",
    "Unread"
  ],
  "read": [
    "已读",
    "Read"
  ],
  "daily_reading": [
    "日常阅读",
    "Daily reading"
  ],
  "experiments_and_evidence": [
    "实验与证据",
    "Experiments and evidence"
  ],
  "classics_club_duo": [
    "古典部双人",
    "Classics club duo"
  ]
};
export const zhCompletion = Object.fromEntries(Object.entries(completionEntries).map(([key, value]) => [key, value[0]]));
export const enCompletion = Object.fromEntries(Object.entries(completionEntries).map(([key, value]) => [key, value[1]]));
