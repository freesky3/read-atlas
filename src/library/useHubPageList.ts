// Hub 主列表的分页读：有 libraryClient 时走 hub_page，不再把整库 DocumentCard 滤一遍。
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { LibraryWorkspaceClient } from "./libraryWorkspaceClient";
import {
  hubPageRequest,
  type HubPageResult,
  type HubPaperCard,
  type LibraryHubSort,
  type LibraryQueryFilter,
} from "./libraryWorkspaceTypes";
import { querySnapshotFromPage } from "./hubFilters";
import type { QuerySnapshot as SelectionSnapshot } from "./selectionModel";

export function useHubPageList(input: {
  client: LibraryWorkspaceClient | undefined;
  filters: readonly LibraryQueryFilter[];
  sort: LibraryHubSort;
  direction?: "asc" | "desc";
  enabled: boolean;
}): {
  papers: readonly HubPaperCard[];
  totalCount: number;
  hasMore: boolean;
  loading: boolean;
  snapshot: SelectionSnapshot | null;
  revision: number;
  loadMore: () => void;
} {
  const { client, filters, sort, direction, enabled } = input;
  const [page, setPage] = useState<HubPageResult | null>(null);
  const [papers, setPapers] = useState<HubPaperCard[]>([]);
  const [loading, setLoading] = useState(() => Boolean(enabled && client));
  const [epoch, setEpoch] = useState(0);
  const generation = useRef(0);
  const prevFilterKey = useRef<string | null>(null);

  const filterKey = JSON.stringify({ filters, sort, direction });

  useEffect(() => {
    if (!enabled || !client) return;
    let cancelled = false;
    let unsub: (() => void) | undefined;
    void client
      .watch(null, () => {
        if (!cancelled) setEpoch((value) => value + 1);
      })
      .then((handle) => {
        if (cancelled) handle.unsubscribe();
        else unsub = handle.unsubscribe;
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, [client, enabled]);

  useEffect(() => {
    if (!enabled || !client) {
      setPage(null);
      setPapers([]);
      setLoading(false);
      return;
    }
    const gen = ++generation.current;
    const filterChanged = prevFilterKey.current !== filterKey;
    prevFilterKey.current = filterKey;
    if (filterChanged) {
      setPapers([]);
      setPage(null);
    }
    let cancelled = false;
    setLoading(true);
    void client
      .read(
        hubPageRequest({ filters: [...filters], sort, direction, limit: 100 }),
      )
      .then((result) => {
        if (
          cancelled ||
          gen !== generation.current ||
          result.kind !== "hub_page"
        )
          return;
        setPage(result.page);
        setPapers([...result.page.papers]);
      })
      .catch(() => {
        if (!cancelled && gen === generation.current) {
          setPage(null);
          setPapers([]);
        }
      })
      .finally(() => {
        if (!cancelled && gen === generation.current) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [client, enabled, epoch, filterKey, filters, sort, direction]);

  const loadMore = useCallback(() => {
    if (!client || !page?.hasMore || !page.nextCursor || loading) return;
    const gen = generation.current;
    setLoading(true);
    void client
      .read(
        hubPageRequest({
          filters: [...filters],
          sort,
          direction,
          limit: 100,
          cursor: page.nextCursor,
        }),
      )
      .then((result) => {
        if (gen !== generation.current || result.kind !== "hub_page") return;
        setPage(result.page);
        setPapers((current) => {
          const seen = new Set(current.map((card) => card.id));
          return [
            ...current,
            ...result.page.papers.filter((card) => !seen.has(card.id)),
          ];
        });
      })
      .finally(() => {
        if (gen === generation.current) setLoading(false);
      });
  }, [client, filters, loading, page, sort, direction]);

  const snapshot = useMemo(
    () => (page ? querySnapshotFromPage(page) : null),
    [page],
  );

  return {
    papers,
    totalCount: page?.totalCount ?? 0,
    hasMore: Boolean(page?.hasMore),
    loading,
    snapshot,
    revision: page?.revision ?? 0,
    loadMore,
  };
}
