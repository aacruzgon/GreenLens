// Every list is paged, at most 100 items a page (contracts §1, lists; README §9, lists).

export interface Page<T> {
  items: T[];
  next_cursor?: string | null;
}

export const MAX_PAGE_SIZE = 100;

/** `getNextPageParam` for useInfiniteQuery: the cursor, or undefined at the end. */
export function nextCursor<T>(page: Page<T>): string | undefined {
  return page.next_cursor ?? undefined;
}

/** Follows `next_cursor` to the end, for lists that must be complete (the map's plants). */
export async function fetchAllPages<T>(fetchPage: (cursor: string | undefined) => Promise<Page<T>>): Promise<T[]> {
  const items: T[] = [];
  let cursor: string | undefined;
  do {
    const page = await fetchPage(cursor);
    items.push(...page.items);
    cursor = nextCursor(page);
  } while (cursor);
  return items;
}

/** The items of every loaded page of an infinite query, in order. */
export function flattenPages<T>(data: { pages: Page<T>[] } | undefined): T[] {
  return data ? data.pages.flatMap((p) => p.items) : [];
}
