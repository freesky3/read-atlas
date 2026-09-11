import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import OperationsDrawer from "./OperationsDrawer";
import type {
  DiagnosticPreview,
  JobProjection,
  RemoteTombstoneProjection,
  StorageReport,
  TrashProjection,
  ProviderInstanceView,
} from "./types";
import type { BatchProjection } from "./library/libraryActTypes";
import { EMPTY_COST_PREVIEW } from "./library/libraryActTypes";
import { clearUndoTokens, rememberUndoToken } from "./library/undoTokenStore";
import { renderWithLocale as render } from "./i18n/testUtils";

const job: JobProjection = {
  id: "job-12345678",
  kind: "reading_artifact",
  provider: "gemini",
  paperId: "paper-1",
  revisionId: "revision-1",
  rootKey: "root",
  artifactKey: "lens:block-1",
  dedupeKey: "dedupe",
  state: "paused",
  stage: "paused",
  providerCommitted: false,
  priority: 80,
  payload: { action: "lens" },
  lastError: null,
  createdAt: "2026-08-16T10:00:00Z",
  updatedAt: "2026-08-16T10:01:00Z",
};

const storage: StorageReport = {
  workspaceBytes: 4096,
  papersBytes: 3072,
  internalBytes: 1024,
  paperLogicalBytes: [
    {
      paperId: "paper-1",
      sourceBytes: 3072,
      logicalDatabaseBytes: 300,
      artifactBytes: 200,
      discussionBytes: 60,
      ocrBytes: 40,
    },
  ],
  artifactLogicalBytes: [
    {
      artifactId: "artifact-1",
      paperId: "paper-1",
      kind: "lens_figure",
      objectKey: "block-1",
      version: 2,
      logicalBytes: 200,
    },
  ],
};

const diagnostics: DiagnosticPreview = {
  generatedAt: "2026-08-16T10:02:00Z",
  includedSections: ["aggregate Job states"],
  excludedData: ["API keys", "PDF bytes, Prompt text, and Block text"],
  summary: { counts: { jobs: 1 } },
};

const trash: TrashProjection = {
  id: "trash-1",
  paperId: "paper-trash",
  title: "Recoverable paper",
  fileName: "paper.pdf",
  originalRelativePath: "Methods/paper.pdf",
  sourceBytes: 1024,
  deletedAt: "2026-08-16T10:00:00Z",
  purgeAfter: "2026-09-15T10:00:00Z",
};

const tombstone: RemoteTombstoneProjection = {
  id: "remote-1",
  provider: "gemini",
  resourceKind: "file",
  paperId: "paper-trash",
  state: "pending",
  attempts: 1,
  lastError: "temporary provider failure",
  ownershipStatus: "exact",
  createdAt: "2026-08-16T10:00:00Z",
  updatedAt: "2026-08-16T10:01:00Z",
};
describe("OperationsDrawer", () => {
  it("controls durable jobs and explains logical storage before restoring Trash", async () => {
    const user = userEvent.setup();
    const onResume = vi.fn();
    const onReprioritize = vi.fn();
    const onRestore = vi.fn();
    const onPreviewDiagnostics = vi.fn();
    const onRetryRemote = vi.fn();

    render(
      <OperationsDrawer
        open
        jobs={[job]}
        storage={storage}
        diagnostics={diagnostics}
        trash={[trash]}
        remoteTombstones={[tombstone]}
        documents={[]}
        busyJobId=""
        onClose={vi.fn()}
        onRefresh={vi.fn()}
        onPause={vi.fn()}
        onResume={onResume}
        onReprioritize={onReprioritize}
        onCancel={vi.fn()}
        onRestore={onRestore}
        onRetryRemote={onRetryRemote}
        onPreviewDiagnostics={onPreviewDiagnostics}
        onExportDiagnostics={vi.fn()}
      />,
    );

    await user.click(
      screen.getByRole("button", { name: /提高 选区 Lens 的优先级/i }),
    );
    expect(onReprioritize).toHaveBeenCalledWith(job.id, 90);
    await user.click(screen.getByRole("button", { name: "继续" }));
    expect(onResume).toHaveBeenCalledWith(job.id);
    await user.click(screen.getByRole("button", { name: "重试清理" }));
    expect(onRetryRemote).toHaveBeenCalledWith(tombstone.id);

    await user.click(screen.getByRole("button", { name: "存储" }));
    expect(screen.getByText("工作区存储")).toBeInTheDocument();
    expect(screen.getByText(/按逻辑大小统计/)).toBeInTheDocument();
    expect(screen.getByText(/lens figure v2/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "诊断" }));
    expect(screen.getByText("排除内容")).toBeInTheDocument();
    expect(screen.getByText(/Prompt text/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "预览范围" }));
    expect(onPreviewDiagnostics).toHaveBeenCalledTimes(1);

    await user.click(screen.getByRole("button", { name: /回收站/ }));
    expect(screen.getByText("Recoverable paper")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "恢复" }));
    expect(onRestore).toHaveBeenCalledWith(trash.paperId);
  });

  it("renders newest jobs at the top of the task list", () => {
    const olderJob: JobProjection = {
      ...job,
      id: "job-older-1111",
      createdAt: "2026-08-16T09:00:00Z",
      updatedAt: "2026-08-16T09:00:00Z",
      payload: { action: "translate" },
    };
    const newerJob: JobProjection = {
      ...job,
      id: "job-newer-2222",
      createdAt: "2026-08-16T12:00:00Z",
      updatedAt: "2026-08-16T12:00:00Z",
      payload: { action: "explain" },
    };

    render(
      <OperationsDrawer
        open
        jobs={[olderJob, newerJob]}
        storage={storage}
        diagnostics={diagnostics}
        trash={[trash]}
        remoteTombstones={[tombstone]}
        documents={[]}
        busyJobId=""
        onClose={vi.fn()}
        onRefresh={vi.fn()}
        onPause={vi.fn()}
        onResume={vi.fn()}
        onReprioritize={vi.fn()}
        onCancel={vi.fn()}
        onRestore={vi.fn()}
        onRetryRemote={vi.fn()}
        onPreviewDiagnostics={vi.fn()}
        onExportDiagnostics={vi.fn()}
      />,
    );

    const taskCards = screen.getAllByRole("article");
    // newerJob (12:00) should be rendered before olderJob (09:00)
    expect(taskCards[0]).toHaveTextContent("区块解释");
    expect(taskCards[1]).toHaveTextContent("区块翻译");
  });

  it("orders jobs by updatedAt newest first even when an older job is still running", () => {
    const runningOld: JobProjection = {
      ...job,
      id: "old-run",
      state: "running",
      createdAt: "2026-08-16T09:00:00Z",
      updatedAt: "2026-08-16T09:00:00Z",
    };
    const completedNew: JobProjection = {
      ...job,
      id: "new-done",
      state: "completed",
      createdAt: "2026-08-16T11:00:00Z",
      updatedAt: "2026-08-16T12:00:00Z",
      payload: { action: "explain" },
    };
    render(
      <OperationsDrawer
        open
        jobs={[runningOld, completedNew]}
        storage={storage}
        diagnostics={diagnostics}
        trash={[trash]}
        remoteTombstones={[tombstone]}
        documents={[]}
        busyJobId=""
        onClose={vi.fn()}
        onRefresh={vi.fn()}
        onPause={vi.fn()}
        onResume={vi.fn()}
        onReprioritize={vi.fn()}
        onCancel={vi.fn()}
        onRestore={vi.fn()}
        onRetryRemote={vi.fn()}
        onPreviewDiagnostics={vi.fn()}
        onExportDiagnostics={vi.fn()}
      />,
    );
    const cards = document.querySelectorAll("article.job-card");
    expect(cards).toHaveLength(2);
    expect(cards[0]).toHaveTextContent("区块解释");
    expect(cards[1]).toHaveTextContent("选区 Lens");
  });

  it("requires an explicit exact Provider before rebinding an uncommitted job", async () => {
    const user = userEvent.setup();
    const onRebindProvider = vi.fn();
    const exactCandidate: ProviderInstanceView = {
      id: "provider-gemini-exact",
      name: "Research Gemini",
      kind: "gemini",
      baseUrl: null,
      paperModel: "gemini-2.5-flash",
      translationModel: "gemini-2.5-flash-lite",
      models: [],
      modelsFetchedAt: null,
      connectionVerifiedAt: "2026-08-24T10:00:00Z",
      paperProbe: null,
      credentialConfigured: true,
      paperProbePassed: true,
      isCurrent: false,
      sortOrder: 1,
    };
    const candidates: ProviderInstanceView[] = [
      exactCandidate,
      {
        ...exactCandidate,
        id: "provider-openai-wrong-kind",
        name: "Wrong kind",
        kind: "openai_compatible",
      },
      {
        ...exactCandidate,
        id: "provider-gemini-wrong-paper",
        name: "Wrong paper model",
        paperModel: "gemini-2.5-pro",
      },
      {
        ...exactCandidate,
        id: "provider-gemini-wrong-translation",
        name: "Wrong translation model",
        translationModel: "gemini-2.0-flash",
      },
    ];
    const actionRequiredJob: JobProjection = {
      ...job,
      providerRoute: {
        instanceId: "original-provider",
        instanceName: "Previous Provider",
        kind: "gemini",
        models: {
          paper: "gemini-2.5-flash",
          translation: "gemini-2.5-flash-lite",
          operation: "paper",
        },
        endpointLabel: "gemini.google.com",
        routeStatus: "action_required",
      },
      providerRequirement: {
        code: "credential_missing",
        providerKind: "gemini",
        providerInstanceId: "original-provider",
        canRebind: true,
        providerCommitted: false,
      },
    };

    render(
      <OperationsDrawer
        open
        jobs={[actionRequiredJob]}
        storage={storage}
        diagnostics={diagnostics}
        trash={[]}
        remoteTombstones={[]}
        documents={[]}
        busyJobId=""
        onClose={vi.fn()}
        onRefresh={vi.fn()}
        onPause={vi.fn()}
        onResume={vi.fn()}
        onReprioritize={vi.fn()}
        onCancel={vi.fn()}
        onRestore={vi.fn()}
        onRetryRemote={vi.fn()}
        onPreviewDiagnostics={vi.fn()}
        onExportDiagnostics={vi.fn()}
        providerInstances={candidates}
        onRebindProvider={onRebindProvider}
      />,
    );

    expect(screen.getByRole("alert")).toHaveTextContent("\u9700\u8981\u5904\u7406");
    expect(screen.queryByRole("button", { name: "继续" })).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /提高 选区 Lens 的优先级/i }),
    ).not.toBeInTheDocument();

    const selector = screen.getByLabelText("\u4e3a 选区 Lens \u9009\u62e9\u53ef\u5b89\u5168\u7ed1\u5b9a\u7684 Provider");
    expect(selector).toHaveValue("");
    expect(screen.getByRole("option", { name: /Research Gemini/ })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: "Wrong kind" })).not.toBeInTheDocument();
    expect(screen.queryByRole("option", { name: "Wrong paper model" })).not.toBeInTheDocument();
    expect(screen.queryByRole("option", { name: "Wrong translation model" })).not.toBeInTheDocument();

    const bind = screen.getByRole("button", { name: "\u7ed1\u5b9a\u6240\u9009 Provider" });
    expect(bind).toBeDisabled();
    await user.selectOptions(selector, exactCandidate.id);
    await user.click(bind);
    expect(onRebindProvider).toHaveBeenCalledWith(
      actionRequiredJob.id,
      exactCandidate.id,
    );
  });

  it("guides a missing credential to its exact Provider settings and recheck action", async () => {
    const user = userEvent.setup();
    const onOpenProviderSettings = vi.fn();
    const onRecheckProvider = vi.fn();
    const credentialJob: JobProjection = {
      ...job,
      id: "credential-job-123",
      providerRequirement: {
        code: "credential_missing",
        providerKind: "gemini",
        providerInstanceId: "provider-needing-key",
        canRebind: false,
        providerCommitted: false,
      },
    };

    render(
      <OperationsDrawer
        open
        jobs={[credentialJob]}
        storage={storage}
        diagnostics={diagnostics}
        trash={[]}
        remoteTombstones={[]}
        documents={[]}
        busyJobId=""
        onClose={vi.fn()}
        onRefresh={vi.fn()}
        onPause={vi.fn()}
        onResume={vi.fn()}
        onReprioritize={vi.fn()}
        onCancel={vi.fn()}
        onRestore={vi.fn()}
        onRetryRemote={vi.fn()}
        onPreviewDiagnostics={vi.fn()}
        onExportDiagnostics={vi.fn()}
        onOpenProviderSettings={onOpenProviderSettings}
        onRecheckProvider={onRecheckProvider}
      />,
    );

    await user.click(screen.getByRole("button", { name: "\u6253\u5f00\u6a21\u578b\u8bbe\u7f6e" }));
    expect(onOpenProviderSettings).toHaveBeenCalledWith("provider-needing-key");
    await user.click(screen.getByRole("button", { name: "\u91cd\u65b0\u68c0\u67e5\u539f Provider" }));
    expect(onRecheckProvider).toHaveBeenCalledWith(credentialJob.id);
  });

  it("requires a second confirmation before abandoning a legacy Provider job", async () => {
    const user = userEvent.setup();
    const onAbandonLegacyProviderJob = vi.fn();
    const legacyJob: JobProjection = {
      ...job,
      id: "legacy-job-123",
      providerRoute: null,
      providerRequirement: {
        code: "legacy_unattributed",
        providerKind: "gemini",
        providerInstanceId: null,
        canRebind: false,
        providerCommitted: true,
      },
    };

    render(
      <OperationsDrawer
        open
        jobs={[legacyJob]}
        storage={storage}
        diagnostics={diagnostics}
        trash={[]}
        remoteTombstones={[]}
        documents={[]}
        busyJobId=""
        onClose={vi.fn()}
        onRefresh={vi.fn()}
        onPause={vi.fn()}
        onResume={vi.fn()}
        onReprioritize={vi.fn()}
        onCancel={vi.fn()}
        onRestore={vi.fn()}
        onRetryRemote={vi.fn()}
        onPreviewDiagnostics={vi.fn()}
        onExportDiagnostics={vi.fn()}
        onAbandonLegacyProviderJob={onAbandonLegacyProviderJob}
      />,
    );

    expect(screen.getByRole("alert")).toHaveTextContent(
      "\u65e0\u6cd5\u786e\u8ba4\u5f53\u65f6\u4f7f\u7528\u7684 Provider",
    );
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "\u653e\u5f03\u672c\u5730\u4efb\u52a1" }));
    expect(onAbandonLegacyProviderJob).not.toHaveBeenCalled();
    expect(screen.getByText(/\u6b64\u64cd\u4f5c\u4e0d\u4f1a\u8054\u7f51\u6e05\u7406\u8fdc\u7aef\u8d44\u6e90/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "\u786e\u8ba4\u653e\u5f03\uff08\u53ef\u80fd\u5df2\u7ecf\u4ea7\u751f\u8d39\u7528\uff09" }));
    expect(onAbandonLegacyProviderJob).toHaveBeenCalledWith(legacyJob.id, true);
  });
});

  it("requires a second confirmation before abandoning legacy remote cleanup", async () => {
    const user = userEvent.setup();
    const onRetryRemote = vi.fn();
    const onAbandonRemoteCleanup = vi.fn();
    const legacyTombstone = {
      ...tombstone,
      id: "legacy-remote-1",
      ownershipStatus: "legacy_unattributed",
    } as RemoteTombstoneProjection & { ownershipStatus: "legacy_unattributed" };

    render(
      <OperationsDrawer
        open
        jobs={[]}
        storage={storage}
        diagnostics={diagnostics}
        trash={[]}
        remoteTombstones={[legacyTombstone]}
        documents={[]}
        busyJobId=""
        onClose={vi.fn()}
        onRefresh={vi.fn()}
        onPause={vi.fn()}
        onResume={vi.fn()}
        onReprioritize={vi.fn()}
        onCancel={vi.fn()}
        onRestore={vi.fn()}
        onRetryRemote={onRetryRemote}
        onPreviewDiagnostics={vi.fn()}
        onExportDiagnostics={vi.fn()}
        onAbandonRemoteCleanup={onAbandonRemoteCleanup}
      />,
    );

    expect(screen.queryByRole("button", { name: "重试清理" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "\u653e\u5f03\u81ea\u52a8\u6e05\u7406" }));
    expect(onAbandonRemoteCleanup).not.toHaveBeenCalled();
    expect(screen.getByText(/\u8fdc\u7aef\u8d44\u6e90\u53ef\u80fd\u4ecd\u5b58\u5728/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "\u786e\u8ba4\u653e\u5f03\u81ea\u52a8\u6e05\u7406" }));
    expect(onAbandonRemoteCleanup).toHaveBeenCalledWith(legacyTombstone.id, true);
  });

  it("任务中心在 Undo Token 有效期内显示撤销整批", async () => {
    const user = userEvent.setup();
    const onUndoBatch = vi.fn();
    rememberUndoToken("batch-1", "plain-token", new Date(Date.now() + 60_000).toISOString());
    const batch: BatchProjection = {
      protocolVersion: 1,
      id: "batch-1",
      commandKind: "move",
      parentCommandKind: null,
      parentBatchId: null,
      relation: null,
      state: "completed",
      planDigest: "plan",
      targetDigest: "target",
      undoPolicy: "full",
      totalItems: 2,
      counts: {
        planned: 0,
        queued: 0,
        running: 0,
        paused: 0,
        actionRequired: 0,
        interruptedUnknown: 0,
        succeeded: 2,
        failed: 0,
        skipped: 0,
        cancelled: 0,
      },
      requirements: [],
      isCancelling: false,
      planExpiresAt: null,
      createdAt: "2026-09-03T10:00:00Z",
      startedAt: "2026-09-03T10:00:01Z",
      finishedAt: "2026-09-03T10:00:02Z",
      updatedAt: "2026-09-03T10:00:02Z",
      retrySummary: null,
      compensationSummary: null,
      undo: { state: "available", expiresAt: new Date(Date.now() + 60_000).toISOString() },
      costPreview: EMPTY_COST_PREVIEW,
    };
    render(
      <OperationsDrawer
        open
        jobs={[]}
        batches={[batch]}
        storage={storage}
        diagnostics={diagnostics}
        trash={[]}
        remoteTombstones={[]}
        documents={[]}
        busyJobId=""
        onClose={vi.fn()}
        onRefresh={vi.fn()}
        onPause={vi.fn()}
        onResume={vi.fn()}
        onReprioritize={vi.fn()}
        onCancel={vi.fn()}
        onRestore={vi.fn()}
        onRetryRemote={vi.fn()}
        onPreviewDiagnostics={vi.fn()}
        onExportDiagnostics={vi.fn()}
        onUndoBatch={onUndoBatch}
      />,
    );
    await user.click(screen.getByRole("button", { name: "撤销整批" }));
    expect(onUndoBatch).toHaveBeenCalledWith("batch-1", "plain-token");
    clearUndoTokens();
  });

