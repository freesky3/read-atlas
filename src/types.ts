export type ThemeMode = "liquid-light" | "liquid-dark" | "warm-editorial";

export type SourceStatus = "ready" | "source_missing" | "deleted";

export type DocumentKind = "paper" | "textbook";

export type DocumentCard = {
  id: string;
  revisionId: string;
  title: string;
  authors: string;
  year: string;
  yearLabel?: string;
  pages: number;
  collection: string;
  kind: DocumentKind;
  sha256: string;
  pdfPath: string;
  sourceStatus: SourceStatus;
  briefStatus: "missing" | "queued" | "ready" | "stale" | "failed";
  hasOcr?: boolean;
  briefTakeaway?: string;
  keywords?: string[];
  chapterNumber?: string;
  lastReadPage?: number;
  updatedAt?: string;
  importedAt: string;
  fileName: string;
  lifecycleStatus?: ReadingLifecycleStatus;
  favorite?: boolean;
  priority?: 0 | 1 | 2 | 3;
  readLater?: boolean;
  reviewAt?: string | null;
  lifecycleVersion?: number;
  furthestPage?: number | null;
  lastOpenedAt?: string | null;
  ocrFailed?: boolean;
};

export type CollectionProjection = {
  id: string;
  parentId: string | null;
  name: string;
  relativePath: string;
  sortMode: "recent" | "year" | "title" | "manual" | "chapter";
  paperOrder: string[];
};

export type Citation = {
  revisionId: string;
  page?: number;
  region?: { x: number; y: number; width: number; height: number };
  regionHash?: string;
  excerpt?: string;
  confidence?: "high" | "medium" | "low";
};

export type UsageReceipt = {
  inputTokens: number | null;
  cachedInputTokens: number | null;
  uncachedInputTokens: number | null;
  outputTokens: number | null;
  reasoningTokens: number | null;
  cacheHitRate: number | null;
  latencyMs: number | null;
  estimatedCost: string | null;
  fileReuse: boolean | null;
  sessionResume: boolean | null;
  paperRootBranch: boolean | null;
  contextEpoch: string | null;
  provider: string;
  model: string;
};

export type BlockQuoteSnapshot = {
  revisionId: string;
  ocrRevisionId: string;
  blockId: string;
  pageNumber: number;
  blockIndex: number;
  blockType: string;
  textContent: string;
  contentDigest: string;
  bbox: [number, number, number, number];
  cropDataUrl?: string | null;
};

export type Message = {
  id: string;
  threadId: string;
  parentId: string | null;
  role: "user" | "assistant" | "system";
  content: string;
  citations: Citation[];
  blockQuotes: BlockQuoteSnapshot[];
  status: "streaming" | "complete" | "cancelled" | "failed";
  createdAt: string;
  usage?: UsageReceipt | null;
};

export type Thread = {
  id: string;
  revisionId: string;
  kind: "global" | "local" | "lens";
  title: string;
  status: "active" | "archived";
  activeMessageId: string | null;
  createdAt: string;
  updatedAt: string;
};

export type CloseThreadResult = {
  action: "archived" | "discarded";
  threadId: string;
};

export type DeleteDiscussionTurnResult = {
  deleted: number;
  nextHeadId: string | null;
};

export type TextbookBriefContent = {
  takeaway: string; keywords: string[];
  learningScope: string; motivation: string; prerequisites: string;
  knowledgeStructure: string; coreKnowledge: string; masteryGoals: string; connections: string;
};

export type Brief = Partial<TextbookBriefContent> & {
  briefProtocol?: "textbook-v2" | "v1";
  documentKind?: DocumentKind;
  revisionId: string;
  version: number;
  status: "queued" | "ready" | "stale" | "failed";
  takeaway?: string;
  keywords: string[];
  classification?: string;
  context?: string;
  backgroundAndProblem?: string;
  coreMethod?: string;
  findings?: string;
  evaluation?: string;
  futureWork?: string;
  summary?: string;
  researchQuestion?: string;
  method?: string;
  limitations?: string;
  evidence?: Citation[];
  model: string;
  createdAt: string;
};

export type WorkspaceInfo = {
  rootPath: string;
  databasePath: string;
  libraryPath: string;
  textbooksPath: string;
  unmanagedPdfNotice?: string | null;
  available: boolean;
  statusDetail: string;
};

export type GeminiModelOption = {
  id: string;
  displayName: string;
  description: string;
  inputTokenLimit?: number | null;
  outputTokenLimit?: number | null;
  supportsGenerateContent: boolean;
  supportsNativePdf: boolean;
  supportsInteractions: boolean;
};

export type ProviderKind = "gemini" | "openai_compatible" | "grok" | "gemini_proxy";

export type PaperProbeRecord = {
  fingerprint: string;
  passedAt: string;
  paperModel: string;
};

export type ProviderInstanceView = {
  id: string;
  name: string;
  kind: ProviderKind;
  baseUrl: string | null;
  paperModel: string;
  translationModel: string;
  models: GeminiModelOption[];
  modelsFetchedAt: string | null;
  connectionVerifiedAt: string | null;
  paperProbe: PaperProbeRecord | null;
  credentialConfigured: boolean;
  paperProbePassed: boolean;
  isCurrent: boolean;
  sortOrder: number;
};

export type ModelSettings = {
  currentProviderId: string | null;
  providers: ProviderInstanceView[];
  credentialStore: string;
  mistralCredentialConfigured: boolean;
  mistralCredentialStore: string;
  ocrModel: string;
};

export type ConnectionTestResult = {
  models: GeminiModelOption[];
  testedAt: string;
  usingStoredCredential: boolean;
  paperProbePassed: boolean | null;
  paperProbeError: string | null;
  modelsFetchError: string | null;
};

export type EvidenceAnchor = {
  revisionId: string;
  pageNumber: number;
  blockId: string | null;
  bbox: [number, number, number, number] | null;
  excerpt: string | null;
};

export type ArtifactProjection = {
  id: string;
  paperId: string;
  revisionId: string;
  ocrRevisionId: string | null;
  kind: string;
  objectKey: string;
  version: number;
  status: string;
  content: unknown;
  overrides: Record<string, unknown>;
  evidence: EvidenceAnchor[];
  dependencySnapshot: unknown;
  providerNodeId: string | null;
  createdAt: string;
};

export type UsageEnvelope = Omit<UsageReceipt, "cacheHitRate">;

export type ReadingArtifactOutcome = {
  artifact: ArtifactProjection;
  receipts: UsageEnvelope[];
  repaired: boolean;
};

export type LensQaTurn = {
  user: LensQaProjection;
  assistant: LensQaProjection;
  receipt: UsageEnvelope;
};

export type OcrBlockProjection = {
  id: string;
  pageNumber: number;
  blockIndex: number;
  blockType: string;
  textContent: string;
  contentDigest: string;
  bbox: [number, number, number, number];
};

export type OcrProjection = {
  id: string;
  revisionId: string;
  status: string;
  provider: string;
  model: string;
  blocks: OcrBlockProjection[];
  createdAt: string;
  publishedAt: string | null;
};

/** Durable user-owned anchor retained across OCR revisions. */
export type AnnotationKind = "highlight" | "bookmark" | "note";
export type AnnotationStatus = "active" | "migrated" | "orphaned" | "orphan" | "deleted";

export type AnnotationLocator = {
  documentRevisionId?: string | null;
  ocrRevisionId?: string | null;
  blockId?: string | null;
  blockIndex?: number | null;
  blockType?: string | null;
  contentDigest?: string | null;
  excerpt?: string | null;
  pageNumber: number;
  bbox?: [number, number, number, number] | null;
  pageWidth?: number | null;
  pageHeight?: number | null;
  coordinateSpace?: string | null;
  textRange?: { start: number; end: number } | null;
};

export type UserAnnotation = {
  id: string;
  paperId: string;
  kind: AnnotationKind;
  title?: string | null;
  body?: string | null;
  color?: string | null;
  locator: AnnotationLocator;
  status: AnnotationStatus;
  createdAt: string;
  updatedAt: string;
};

export type UserAnnotationInput = Omit<UserAnnotation, "id" | "createdAt" | "updatedAt" | "status"> & {
  status?: AnnotationStatus;
};
export type UserAnnotationUpdateRequest = {
  id: string;
  title?: string | null;
  body?: string | null;
  color?: string | null;
  locator?: AnnotationLocator;
  status?: AnnotationStatus;
};
export type UserAnnotationLinkKind = "related" | "supports" | "contradicts" | "question";
export type UserAnnotationLink = {
  id: string;
  sourceAnnotationId: string;
  targetAnnotationId: string;
  linkKind: UserAnnotationLinkKind;
  createdAt: string;
};
export type UserAnnotationLinkInput = {
  sourceAnnotationId: string;
  targetAnnotationId: string;
  linkKind: UserAnnotationLinkKind;
};

export type LensQaProjection = {
  id: string;
  lensArtifactId: string;
  parentId: string | null;
  role: "user" | "assistant";
  content: string;
  status: "streaming" | "complete" | "cancelled" | "failed";
  providerNodeId: string | null;
  createdAt: string;
};

export type JobState =
  | "queued"
  | "running"
  | "paused"
  | "completed"
  | "failed"
  | "cancelled"
  | "interrupted_unknown";

export type JobProgress = {
  step?: number;
  steps?: number;
  batch?: number;
  batches?: number;
  unitCount?: number;
  inputTokens?: number | null;
  outputTokens?: number | null;
  cachedInputTokens?: number | null;
};

export type ProviderRouteStatus =
  | "ready"
  | "legacy"
  | "action_required";

export type ProviderRouteModelsProjection = {
  paper: string | null;
  translation: string | null;
  operation: string | null;
};

/**
 * Safe route summary supplied by the desktop runtime. It deliberately excludes
 * credentials, route identifiers, endpoint scopes, and full endpoint URLs.
 */
export type ProviderRouteProjection = {
  instanceId: string | null;
  instanceName: string | null;
  kind: ProviderKind | null;
  models: ProviderRouteModelsProjection;
  endpointLabel: string | null;
  routeStatus: ProviderRouteStatus;
};

export type ProviderRequirementCode =
  | "provider_missing"
  | "credential_missing"
  | "provider_not_ready"
  | "provider_kind_changed"
  | "endpoint_changed"
  | "credential_changed"
  | "unsupported_snapshot_version"
  | "invalid_snapshot"
  | "legacy_ambiguous"
  | "legacy_unattributed"
  | "model_mismatch"
  | "endpoint_mismatch"
  | "provider_committed"
  | "quarantined"
  | "route_conflict";

export type ProviderRequirementProjection = {
  code: ProviderRequirementCode;
  providerKind: ProviderKind | null;
  providerInstanceId: string | null;
  canRebind: boolean;
  providerCommitted: boolean;
};

export type JobProjection = {
  id: string;
  kind: string;
  provider: string | null;
  paperId: string | null;
  revisionId: string | null;
  rootKey: string | null;
  artifactKey: string | null;
  dedupeKey: string;
  state: JobState;
  stage: string;
  providerCommitted: boolean;
  priority: number;
  payload: unknown;
  lastError: string | null;
  createdAt: string;
  updatedAt: string;
    progress?: JobProgress | null;
    providerRoute?: ProviderRouteProjection | null;
    providerRequirement?: ProviderRequirementProjection | null;
    retryDisposition?: "safe" | "confirm_possible_charge" | "unavailable";
    retryReason?: string;
    sourceLocator?: {
      paperId?: string | null;
      revisionId?: string | null;
      artifactKind?: string | null;
      objectKey?: string | null;
    } | null;
  };
export type PaperStorage = {
  paperId: string;
  sourceBytes: number;
  logicalDatabaseBytes: number;
  artifactBytes: number;
  discussionBytes: number;
  ocrBytes: number;
};

export type ArtifactStorage = {
  artifactId: string;
  paperId: string;
  kind: string;
  objectKey: string;
  version: number;
  logicalBytes: number;
};

export type DiagnosticPreview = {
  generatedAt: string;
  includedSections: string[];
  excludedData: string[];
  summary: Record<string, unknown>;
};

export type StorageReport = {
  workspaceBytes: number;
  papersBytes: number;
  internalBytes: number;
  paperLogicalBytes: PaperStorage[];
  artifactLogicalBytes: ArtifactStorage[];
};

export type TrashProjection = {
  id: string;
  paperId: string;
  title: string;
  fileName: string;
  originalRelativePath: string;
  sourceBytes: number;
  deletedAt: string;
  purgeAfter: string;
  kind?: "paper" | "reader_folder" | string;
};

export type ReaderContextScope = "workspace" | "folder" | "paper";

export type ReaderContextProjection = {
  scope: ReaderContextScope;
  collectionPath: string | null;
  paperId: string | null;
  text: string;
  charCount: number;
  warn: boolean;
  path: string;
};
export type RemoteTombstoneProjection = {
  id: string;
  provider: string;
  resourceKind: string;
  paperId: string | null;
  state: "pending" | "retrying" | "resolved";
  attempts: number;
  lastError: string | null;
  ownershipStatus: "exact" | "legacy_unattributed" | "abandoned";
  createdAt: string;
  updatedAt: string;
};
export type ReadingState = {
  paperId: string;
  revisionId: string;
  pageNumber: number;
  pageOffset: number;
  zoom: number;
  rotation: 0 | 90 | 180 | 270;
  rightTab: "discussion" | "artifacts";
  activeArtifactId: string | null;
  activeDiscussionId: string | null;
  discussionDraft: string;
  quoteBasket: BlockQuoteSnapshot[];
  workspaceLayout: WorkspaceLayout;
  activeOutlineNodeId: string | null;
  outlineView: OutlineView;
  outlineInspectorWidth: number;
  guideLayerVisible: boolean;
  longPdfWarningAcked?: boolean;
  updatedAt: string;
};

/** Paper-level intent. This is separate from the per-revision viewport state above. */
export type ReadingLifecycleStatus = "unread" | "reading" | "read";

export type ReadingLifecycleProjection = {
  paperId: string;
  status: ReadingLifecycleStatus;
  favorite: boolean;
  priority: 0 | 1 | 2 | 3;
  readLater: boolean;
  reviewAt: string | null;
  statusChangedAt: string;
  completedAt: string | null;
  version: number;
  updatedAt: string;
};

/** Progress belongs to one document revision and never leaks to a new revision. */
export type ReadingEngagementProjection = {
  paperId: string;
  revisionId: string;
  furthestPage: number;
  pageCountSnapshot: number;
  firstOpenedAt: string;
  lastOpenedAt: string;
  updatedAt: string;
};

export type ReadingContextProjection = {
  lifecycle: ReadingLifecycleProjection;
  engagement: ReadingEngagementProjection | null;
  session: ReadingState | null;
};

export type RecentReadingProjection = {
  /** Browser preview may have engagement without a hydrated library card. */
  document: DocumentCard | null;
  lifecycle: ReadingLifecycleProjection;
  engagement: ReadingEngagementProjection;
  session: ReadingState | null;
};

export type RecordReadingActivityRequest = {
  paperId: string;
  revisionId: string;
  pageNumber: number;
  pageCount: number;
  occurredAt?: string;
};

export type UpdateReadingLifecycleRequest = {
  paperId: string;
  expectedVersion: number;
  patch: Partial<Pick<
    ReadingLifecycleProjection,
    "status" | "favorite" | "priority" | "readLater" | "reviewAt"
  >>;
  occurredAt?: string;
};

export type WorkspaceLayout =
  | "pdf_discussion"
  | "pdf_outline"
  | "outline_only";

export type OutlineView = "overview" | "deep_dive";

export type GuideStatus =
  | "missing_ocr"
  | "ready_to_plan"
  | "generating"
  | "partial"
  | "published"
  | "stale";

export type GuideSpeakerId = string;

export type GuideCastMember = {
  id: string;
  revision: number;
  displayName: string;
  workTitle?: string;
  characterVersion?: string;
  description?: string;
  avatarAssetId?: string | null;
  workspaceAvatarPath?: string | null;
  inkColor: string;
  personality?: string;
  readingHabits?: string;
  expressionStyle?: string;
  avoidances?: string;
  exampleNotes?: string[];
};

export type GuideCastSnapshot = {
  schemaVersion: number;
  characters: GuideCastMember[];
  order: string[];
  relationHints?: string[];
  rulesVersion?: string;
  castDigest: string;
};

export type GuideCharacter = {
  id: string;
  revision: number;
  displayName: string;
  workTitle: string;
  characterVersion: string;
  description: string;
  avatarAssetId: string | null;
  inkColor: string;
  personality: string;
  readingHabits: string;
  expressionStyle: string;
  avoidances: string;
  exampleNotes: string[];
  presetId: string | null;
  presetVersion: number | null;
  createdAt: string;
  updatedAt: string;
};

export type GuidePresetCast = {
  id: string;
  name: string;
  characterIds: string[];
};

export type GuideCharacterSettings = {
  schemaVersion: number;
  storeRevision: number;
  characters: GuideCharacter[];
  defaultCharacterIds: string[];
  presetCasts: GuidePresetCast[];
};

export type GuideLocator = {
  blockId: string;
  pageNumber: number;
  blockType: string;
  bbox: [number, number, number, number];
};

export type GuideTrace = {
  id: string;
  kind: "trace";
  speakerId: GuideSpeakerId;
  anchor: GuideLocator;
};

export type GuideNote = {
  id: string;
  kind: "note";
  speakerId: GuideSpeakerId;
  weight: "line" | "short";
  anchor: GuideLocator;
  body: string;
};

export type GuideReply = {
  id: string;
  kind: "reply";
  speakerId: GuideSpeakerId;
  parentId: string;
  body: string;
};

export type GuideInk = GuideTrace | GuideNote | GuideReply;

export type GuideHeadProjection = {
  id: string;
  status: "published" | "partial";
  ocrRevisionId: string;
  protocolVersion: string;
  promptVersion: string;
  model: string;
  language: string;
  reusedOutlineRevisionId?: string | null;
  coverage: Record<string, unknown>;
  warnings: string[];
  context: Record<string, unknown>;
  inks: GuideInk[];
  castSnapshot?: GuideCastSnapshot | null;
};

export type GuideProjection = {
  status: GuideStatus;
  revisionId: string;
  ocrRevisionId: string | null;
  hasPaperRoot: boolean;
  head: GuideHeadProjection | null;
  activeJobId: string | null;
  preferredCharacterIds?: string[] | null;
};

export type GuidePlan = {
  revisionId: string;
  ocrRevisionId: string;
  catalogDigest: string;
  model: string;
  pageCount: number;
  batchCount: number;
  understandCalls: number;
  rootCalls?: number;
  annotationCalls: number;
  repairCalls: number;
  reusedOutline: boolean;
  hasPaperRoot: boolean;
  supportsNativePdf: boolean;
  estimatedCost: string | null;
  planId?: string | null;
  planDigest?: string | null;
  guideProtocol?: string | null;
  characterIds?: string[] | null;
};

export type OutlineStatus =
  | "missing_ocr"
  | "ready_to_plan"
  | "planned"
  | "generating"
  | "partial"
  | "published"
  | "stale";

export type OutlineCatalogEntry = {
  id: string;
  page: number;
  type: string;
  blockIndex: number;
  bbox: [number, number, number, number];
  excerpt?: string;
  nearbyCaptionBlockId?: string;
};

export type OutlineCatalog = {
  entries: OutlineCatalogEntry[];
  digest: string;
  tokenEstimate: number;
};

export type OutlineHeadProjection = {
  id: string;
  kind: string;
  status: string;
  ocrRevisionId: string;
  catalogDigest: string;
  protocolVersion: string;
  coverageWarnings: string[];
  reviewStatus?: string | null;
  reviewNotes?: string[];
  gaps?: OutlineGap[];
  graph?: OutlineGraph | null;
  units?: OutlineUnit[];
};

export type OutlineReference = {
  blockId?: string | null;
  pageNumber?: number | null;
  purpose: string;
};

export type OutlineGroup = {
  groupId: string;
  title: string;
  nodeIds: string[];
  description?: string | null;
};

export type OutlineGap = {
  description: string;
  relatedNodeIds?: string[];
  relatedEdgeIds?: string[];
  suggestedPages?: number[];
};

export type OutlineUnit = {
  unitId: string;
  roleClass: string;
  roleLabel?: string;
  title: string;
  takeaway: string;
  importance: string;
  evidenceIds: string[];
  confidence: number;
};

export type OutlineNode = {
  nodeId: string;
  title: string;
  takeaway: string;
  roleLabel?: string | null;
  roleClass?: string;
  importance?: string;
  sourceUnitIds?: string[];
  evidenceIds?: string[];
  confidence?: number;
  references?: OutlineReference[];
  uncertainty?: string | null;
};

export type OutlineEdge = {
  edgeId: string;
  sourceNodeId: string;
  targetNodeId: string;
  direction?: "directed" | "undirected" | string;
  label: string;
  rationale: string;
  references?: OutlineReference[];
  uncertainty?: string | null;
  tier?: "narrative" | "cross_link" | string;
  relationClass?: string;
  evidenceIds?: string[];
};

export type OutlineGraph = {
  title: string;
  summary: string;
  nodes: OutlineNode[];
  edges: OutlineEdge[];
  groups?: OutlineGroup[];
  gaps?: OutlineGap[];
};

export type OutlineProjection = {
  status: OutlineStatus;
  revisionId: string;
  ocrRevisionId: string | null;
  hasPaperRoot: boolean;
  catalog: OutlineCatalog | null;
  head: OutlineHeadProjection | null;
  latestAttempt?: OutlineHeadProjection | null;
  activeJobId: string | null;
  coverageWarnings: string[];
};

export type OutlinePlan = {
  revisionId: string;
  ocrRevisionId: string;
  catalogDigest: string;
  model: string;
  rootCalls?: number;
  extractCalls: number;
  composeCalls: number;
  maxRepairCalls: number;
  pageCount: number;
  catalogTokenEstimate: number;
  pdfTokenEstimate: number;
  estimatedCost: string | null;
  hasPaperRoot: boolean;
  supportsNativePdf: boolean;
  planId?: string | null;
  planDigest?: string | null;
  publishId?: string | null;
  protocolVersion?: string;
  workflow?: string;
  expectedHeadId?: string | null;
};

export type AppStats = {
  documents: number;
  threads: number;
  messages: number;
  inputTokens: number;
  cachedTokens: number;
  outputTokens: number;
  estimatedCost: string | null;
};

export type PromptSlotId =
  | "paper_root"
  | "orientation_pack"
  | "glossary"
  | "symbol_table"
  | "metadata"
  | "discussion"
  | "discussion_compaction"
  | "translation"
  | "explanation"
  | "lens_formula"
  | "lens_figure"
  | "lens_table"
  | "lens_repair_formula"
  | "lens_repair_figure"
  | "lens_repair_table"
  | "lens_qa"
  | "outline_extract"
  | "outline_compose"
  | "outline_deep_dive"
  | "reading_roadmap"
  | "guide_context"
  | "guide_annotate";

export type PromptSlotProjection = {
  text: string;
  outputProtocol?: string;
  previousText: string | null;
  updatedAt: string | null;
  isDefault: boolean;
};

export type PromptSlotKindPair = {
  paper: PromptSlotProjection;
  textbook: PromptSlotProjection;
};

export type PromptSettings = {
  schemaVersion: number;
  slots: Record<string, PromptSlotKindPair>;
};

export type UiLocaleProjection = { locale: "zh-CN" | "en" | null };

/* ── Reading Roadmap (精读路线) ── */

export type RoadmapEvidencePill = {
  label: string;
  page: number;
  blockId?: string;
  bbox?: [number, number, number, number];
};

export type RoadmapTask = {
  id: string;
  text: string;
  timeMinutes: number;
  required: boolean;
  completionCriteria: string;
  selfCheckQuestions: string[];
  evidence: RoadmapEvidencePill[];
};

export type RoadmapPass = {
  passNumber: 0 | 1 | 2 | 3;
  title: string;
  subtitle: string;
  timeBudget: string;
  exitCriteria: string;
  tasks: RoadmapTask[];
};

export type ReadingRoadmapContent = {
  version: number;
  generatedAt: string;
  paperTitle: string;
  passes: RoadmapPass[];
  learningObjectives?: string[];
  prerequisites?: string[];
  elevatorPitch?: string;
  oneChart?: {
    label: string;
    page: number;
    blockId?: string;
    reason: string;
  };
};

export type ReadingRoadmapProjection = {
  id: string;
  paperId: string;
  revisionId: string;
  status: "generating" | "published" | "failed";
  content: ReadingRoadmapContent | null;
  activeJobId: string | null;
  lastError: string | null;
  createdAt: string;
};

export type RoadmapProgressEntry = {
  taskId: string;
  completed: boolean;
  completedAt: string | null;
};

export type BlockAction = "quote" | "translate" | "explain" | "lens" | "copy";



export type AuxiliaryDocumentArtifactKind = "glossary" | "symbol_table" | "metadata";
