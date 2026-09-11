export type SendChatInput = {
  revisionId: string;
  threadId: string;
  parentId: string | null;
  question: string;
  page: number | null;
  blockIds: string[];
  regenerateFromId?: string | null;
};

export function buildSendChatInvokeArgs(input: SendChatInput) {
  return {
    request: {
      revisionId: input.revisionId,
      threadId: input.threadId,
      parentId: input.parentId,
      question: input.question,
      page: input.page,
      blockIds: input.blockIds,
      regenerateFromId: input.regenerateFromId ?? null,
    },
  };
}
