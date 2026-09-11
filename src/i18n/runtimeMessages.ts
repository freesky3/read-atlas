export const runtimeMessages: Record<string, readonly [string, string]> = {
  "requirement_0": [
    "{count} 项任务可能产生服务商费用；预览并非最终账单",
    "{count} item(s) may incur provider charges; preview is not the final bill"
  ],
  "requirement_1": [
    "{count} 项费用未知，不会显示为零",
    "{count} item(s) have unknown cost and will not be shown as 0"
  ],
  "requirement_2": [
    "{count} 份 PDF 达到或超过 80 页，发送给服务商前需要确认",
    "{count} PDF(s) have 80 or more pages; confirm before sending them to a provider"
  ],
  "requirement_3": [
    "{count} 份文档将改变类型，其 Brief、地图和旁批会被移除",
    "{count} Paper(s) change document kind; their Brief, outline and guide artifacts are dropped"
  ],
  "requirement_4": [
    "{count} 份文档的目标位置已有文件，这些项目将失败",
    "{count} Paper(s) already have a file at the target path; those items will fail"
  ],
  "requirement_5": [
    "{count} 份文档将移入回收站，正在执行的任务会被取消；撤销仅恢复工作区内的文件",
    "{count} Paper(s) move to Trash; running jobs are cancelled and only files inside the Workspace come back on undo"
  ],
  "requirement_6": [
    "{count} 份文档在记录路径中找不到源文件，这些项目将失败",
    "{count} Paper(s) have no file at the recorded path; those items will fail"
  ],
  "requirement_7": [
    "{count} 个源文件的目标位置已有文件，这些项目将记为冲突，不会新增文档",
    "{count} source(s) already have a file at the target path; those items end as a conflict instead of a new Paper"
  ],
  "requirement_8": [
    "{count} 份导出将覆盖已有文件，被覆盖的版本无法恢复",
    "{count} export(s) replace a file already in the export folder; the replaced version is not recoverable"
  ],
  "requirement_9": [
    "{count} 项重试可能再次计费，因为原服务商请求已提交",
    "{count} retry item(s) may charge again because the original provider request was already committed"
  ],
  "stage_queued": [
    "排队中",
    "Queued"
  ],
  "stage_running": [
    "运行中",
    "Running"
  ],
  "stage_preparing": [
    "准备中",
    "Preparing"
  ],
  "stage_prepared": [
    "已准备",
    "Prepared"
  ],
  "stage_extracting": [
    "抽取中",
    "Extracting"
  ],
  "stage_extracted": [
    "已抽取",
    "Extracted"
  ],
  "stage_drafting": [
    "构图中",
    "Drafting"
  ],
  "stage_drafted": [
    "已构图",
    "Drafted"
  ],
  "stage_reviewing": [
    "检查中",
    "Reviewing"
  ],
  "stage_composing": [
    "构图中",
    "Composing"
  ],
  "stage_repairing": [
    "修复中",
    "Repairing"
  ],
  "stage_published": [
    "已发布",
    "Published"
  ],
  "stage_understanding": [
    "阅读理解中",
    "Understanding"
  ],
  "stage_annotating": [
    "生成旁批中",
    "Annotating"
  ],
  "stage_validating": [
    "校验中",
    "Validating"
  ],
  "stage_publishing": [
    "发布中",
    "Publishing"
  ],
  "stage_uploading": [
    "上传中",
    "Uploading"
  ],
  "stage_completed": [
    "已完成",
    "Completed"
  ],
  "stage_failed": [
    "失败",
    "Failed"
  ],
  "stage_cancelled": [
    "已取消",
    "Cancelled"
  ],
  "stage_interrupted_unknown": [
    "中断，结果未知",
    "Interrupted; outcome unknown"
  ],
  "stage_paused": [
    "已暂停",
    "Paused"
  ],
  "stage_pending": [
    "等待中",
    "Pending"
  ],
  "stage_resolved": [
    "已解决",
    "Resolved"
  ],
  "stage_abandoned": [
    "已放弃",
    "Abandoned"
  ],
  "stage_deleted": [
    "已删除",
    "Deleted"
  ],
  "stage_local": [
    "本地",
    "Local"
  ]
};
export const runtimeZh = Object.fromEntries(Object.entries(runtimeMessages).map(([key, value]) => [key, value[0]]));
export const runtimeEn = Object.fromEntries(Object.entries(runtimeMessages).map(([key, value]) => [key, value[1]]));
