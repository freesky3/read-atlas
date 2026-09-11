export const errorMessages: Record<string, readonly [string, string]> = {
  "over_window": ["PDF 与材料超出模型窗口，请选择容量更大的模型。", "The PDF and material exceed the model context window. Choose a larger model."],
  "unsupported_pdf": ["当前模型不支持原生 PDF，请更换模型。", "The model does not support native PDF. Choose another model."],
  "invalid_query": [
    "请求无效，请检查输入。",
    "The request is invalid. Check the input."
  ],
  "workspace_unavailable": [
    "工作区不可用，请在设置中重新选择。",
    "The workspace is unavailable. Choose it again in Settings."
  ],
  "idempotency_conflict": [
    "这项操作的输入已变化，请重新确认。",
    "The operation inputs changed. Review and confirm again."
  ],
  "stale_selection": [
    "选择已过期，请重新选择文档。",
    "The selection is stale. Select the documents again."
  ],
  "stale_batch_plan": [
    "批量计划已变化，请重新预览。",
    "The batch plan changed. Preview it again."
  ],
  "selection_empty": [
    "请先选择文档。",
    "Select documents first."
  ],
  "batch_not_found": [
    "找不到这项批量任务。",
    "The batch could not be found."
  ],
  "batch_too_large": [
    "本次选择过多，请分批操作。",
    "Too many items were selected. Use smaller batches."
  ],
  "paper_not_found": [
    "该文档已不在文库中。",
    "This document is no longer in the library."
  ],
  "target_path_conflict": [
    "目标位置已有同名文件，请选择其他位置。",
    "A file already exists at the destination. Choose another location."
  ],
  "source_missing": [
    "源文件不存在，请检查路径。",
    "The source file is missing. Check its location."
  ],
  "invalid_source": [
    "源文件无效或不受支持。",
    "The source file is invalid or unsupported."
  ],
  "source_changed": [
    "源文件已变化，请重新发起操作。",
    "The source file changed. Start the operation again."
  ],
  "confirmation_required": [
    "此操作需要先确认预览。",
    "Review and confirm the preview first."
  ],
  "not_reversible": [
    "这项操作无法撤销。",
    "This operation cannot be undone."
  ],
  "undo_expired": [
    "撤销时限已过。",
    "The undo period has expired."
  ],
  "undo_conflict": [
    "相关内容已变化，无法直接撤销。",
    "The affected content changed and cannot be undone directly."
  ],
  "unsupported_for_scope": [
    "当前范围不支持这项操作。",
    "This operation is unavailable for the current scope."
  ],
  "workspace_busy": [
    "工作区正忙，请稍后重试。",
    "The workspace is busy. Try again shortly."
  ],
  "interrupted_unknown": [
    "上次请求结果未知，请在任务中心核对后处理。",
    "The previous request outcome is unknown. Review it in the job center."
  ],
  "stale_library_snapshot": [
    "文库已变化，请刷新后重试。",
    "The library changed. Refresh and try again."
  ],
  "provider_route_unavailable": [
    "模型服务配置不可用，请打开设置检查。",
    "The model provider configuration is unavailable. Check Settings."
  ],
  "cost_confirmation_required": [
    "请先确认此次任务的费用预览。",
    "Confirm the cost preview before starting this job."
  ],
  "possible_duplicate_charge": [
    "上次请求可能已计费，请在任务中心核对。",
    "The previous request may have been charged. Review it in the job center."
  ],
  "unauthorized": [
    "凭据无效或已失效，请检查模型设置。",
    "The credential is invalid or expired. Check model settings."
  ],
  "rate_limited": [
    "服务请求达到限制，请稍后重试。",
    "The provider rate limit was reached. Try again later."
  ],
  "cancelled": [
    "操作已取消。",
    "The operation was cancelled."
  ],
  "name_required": [
    "显示名称不能为空。",
    "The display name cannot be empty."
  ],
  "settings_changed": [
    "设置已被其他窗口更新，请重新加载。",
    "Settings changed in another window. Reload them first."
  ],
  "missing_ocr": [
    "请先完成 OCR。",
    "Complete OCR first."
  ],
  "stale_plan": [
    "任务输入已变化，请重新生成计划。",
    "The task inputs changed. Create a new plan."
  ],
  "unknown": [
    "操作未完成。可在任务中心的诊断页查看错误详情。",
    "The operation could not be completed. See the Diagnostics tab in the job center for details."
  ]
};
