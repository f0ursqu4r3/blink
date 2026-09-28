import type { RequestGroup } from "./groups";
import { sessionLabel, type RequestSession } from "./session";

export type RequestMatch = {
  id: number;
  method: string;
  label: string;
  url: string;
  groupPath: string;
};

/** Group names from the root to `groupId`, joined with " / ". */
export function groupPath(groups: RequestGroup[], groupId: number | null) {
  const byId = new Map(groups.map((group) => [group.id, group]));
  const names: string[] = [];
  const seen = new Set<number>();
  let group = groupId === null ? undefined : byId.get(groupId);
  while (group && !seen.has(group.id)) {
    seen.add(group.id);
    names.unshift(group.name);
    group = group.parentId === null ? undefined : byId.get(group.parentId);
  }
  return names.join(" / ");
}

export function matchRequests(
  sessions: RequestSession[],
  groups: RequestGroup[],
  query: string,
): RequestMatch[] {
  const needle = query.trim().toLowerCase();
  return sessions
    .map((session) => ({
      id: session.id,
      method: session.draft.method,
      label: sessionLabel(session),
      url: session.draft.url,
      groupPath: groupPath(groups, session.groupId),
    }))
    .filter(
      (match) =>
        !needle ||
        [match.label, match.url, match.groupPath].some((text) =>
          text.toLowerCase().includes(needle),
        ),
    );
}
