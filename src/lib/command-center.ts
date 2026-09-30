import type { RequestGroup } from "./groups";
import { sessionLabel, type LabelTokens, type RequestSession } from "./session";

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
  globalDefinitions: Record<string, string> = {},
): RequestMatch[] {
  const tokens: LabelTokens = { groups, globalDefinitions };
  const needle = query.trim().toLowerCase();
  return sessions
    .map((session) => ({
      id: session.id,
      method: session.draft.method,
      label: sessionLabel(session, tokens),
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

/** An app action in the command center. `>` in the search switches to these. */
export type Command = {
  id: string;
  label: string;
  /** Keys for `shortcutLabel`, such as `["mod", "\\"]`. */
  shortcut?: string[];
  disabled?: boolean;
};

export const COMMAND_PREFIX = ">";
export const isCommandQuery = (query: string) =>
  query.startsWith(COMMAND_PREFIX);

/** Commands whose label contains every word of `query`, in list order. */
export function matchCommands(commands: Command[], query: string) {
  const words = query
    .slice(isCommandQuery(query) ? COMMAND_PREFIX.length : 0)
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean);
  return commands.filter((command) => {
    const label = command.label.toLowerCase();
    return words.every((word) => label.includes(word));
  });
}
