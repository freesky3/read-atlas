import type { Message } from "./types";

export type DiscussionTreeNode = {
  message: Message;
  depth: number;
  index: number;
  onActivePath: boolean;
  isHead: boolean;
  isBranchPoint: boolean;
};

export function layoutDiscussionTree(
  messages: Message[],
  activeMessageId: string | null,
): DiscussionTreeNode[] {
  const byId = new Map(
    messages
      .filter((message) => message.status !== "failed")
      .map((message) => [message.id, message]),
  );
  const children = new Map<string | null, Message[]>();
  for (const message of byId.values()) {
    const parentId =
      message.parentId && byId.has(message.parentId) ? message.parentId : null;
    const siblings = children.get(parentId) ?? [];
    siblings.push(message);
    children.set(parentId, siblings);
  }
  for (const siblings of children.values()) {
    siblings.sort((left, right) => {
      const byTime = left.createdAt.localeCompare(right.createdAt);
      return byTime !== 0 ? byTime : left.id.localeCompare(right.id);
    });
  }

  const activePath = new Set<string>();
  let cursor = activeMessageId;
  while (cursor && byId.has(cursor)) {
    activePath.add(cursor);
    cursor = byId.get(cursor)?.parentId ?? null;
  }

  const laid: DiscussionTreeNode[] = [];
  const walk = (parentId: string | null, depth: number) => {
    for (const message of children.get(parentId) ?? []) {
      const childCount = children.get(message.id)?.length ?? 0;
      laid.push({
        message,
        depth,
        index: laid.length,
        onActivePath: activePath.has(message.id),
        isHead: message.id === activeMessageId,
        isBranchPoint: childCount > 1,
      });
      walk(message.id, depth + 1);
    }
  };
  walk(null, 0);
  return laid;
}

export type DiscussionTurn = {
  user: Message;
  assistant: Message | null;
  depth: number;
  index: number;
  onActivePath: boolean;
  isHead: boolean;
  isBranchPoint: boolean;
};

function sortMessages(left: Message, right: Message) {
  const byTime = left.createdAt.localeCompare(right.createdAt);
  return byTime !== 0 ? byTime : left.id.localeCompare(right.id);
}

function groupChildren(messages: Message[]) {
  const byId = new Map(messages.map((message) => [message.id, message]));
  const children = new Map<string | null, Message[]>();
  for (const message of byId.values()) {
    const parentId =
      message.parentId && byId.has(message.parentId) ? message.parentId : null;
    const siblings = children.get(parentId) ?? [];
    siblings.push(message);
    children.set(parentId, siblings);
  }
  for (const siblings of children.values()) {
    siblings.sort(sortMessages);
  }
  return { byId, children };
}

function latestAssistant(
  children: Map<string | null, Message[]>,
  userId: string,
) {
  const kids = children.get(userId) ?? [];
  const live = kids.filter(
    (message) =>
      message.role === "assistant" && message.status !== "failed",
  );
  if (live.length > 0) return live[live.length - 1] ?? null;
  const failed = kids.filter((message) => message.role === "assistant");
  return failed[failed.length - 1] ?? null;
}

function childTurns(
  children: Map<string | null, Message[]>,
  user: Message,
) {
  const fromUser = (children.get(user.id) ?? []).filter(
    (message) => message.role === "user",
  );
  const fromAssistants = (children.get(user.id) ?? [])
    .filter((message) => message.role === "assistant")
    .flatMap((assistant) =>
      (children.get(assistant.id) ?? []).filter(
        (message) => message.role === "user",
      ),
    );
  return [...fromUser, ...fromAssistants].sort(sortMessages);
}

export function layoutDiscussionTurns(
  messages: Message[],
  activeMessageId: string | null,
): DiscussionTurn[] {
  const { byId, children } = groupChildren(messages);
  const users = [...byId.values()].filter((message) => message.role === "user");
  const childIds = new Set<string>();
  for (const user of users) {
    for (const child of childTurns(children, user)) {
      childIds.add(child.id);
    }
  }
  const roots = users
    .filter((user) => !childIds.has(user.id))
    .sort(sortMessages);

  const activePath = new Set<string>();
  let cursor = activeMessageId;
  while (cursor && byId.has(cursor)) {
    activePath.add(cursor);
    cursor = byId.get(cursor)?.parentId ?? null;
  }

  const laid: DiscussionTurn[] = [];
  const walk = (nextUsers: Message[], depth: number) => {
    for (const user of nextUsers) {
      const assistants = (children.get(user.id) ?? []).filter(
        (message) => message.role === "assistant",
      );
      const kids = childTurns(children, user);
      laid.push({
        user,
        assistant: latestAssistant(children, user.id),
        depth,
        index: laid.length,
        onActivePath:
          activePath.has(user.id) ||
          assistants.some((item) => activePath.has(item.id)),
        isHead:
          activeMessageId === user.id ||
          assistants.some((item) => item.id === activeMessageId),
        isBranchPoint: kids.length > 1,
      });
      walk(kids, depth + 1);
    }
  };
  walk(roots, 0);
  return laid;
}

export function countTurnSubtree(messages: Message[], userId: string) {
  const { byId, children } = groupChildren(messages);
  const user = byId.get(userId);
  if (!user || user.role !== "user") return 0;
  const walk = (current: Message): number =>
    1 +
    childTurns(children, current).reduce(
      (sum, child) => sum + walk(child),
      0,
    );
  return walk(user);
}

export function visibleDiscussionNodes(
  nodes: DiscussionTreeNode[],
  activeMessageId: string | null,
): DiscussionTreeNode[] {
  const extras = nodes.filter(
    (node) =>
      node.message.parentId === activeMessageId &&
      node.message.status !== "complete" &&
      node.message.status !== "failed",
  );
  const latestExtra = extras[extras.length - 1];
  return nodes.filter(
    (node) =>
      node.onActivePath ||
      node.message.status === "streaming" ||
      node.message.id === latestExtra?.message.id,
  );
}
