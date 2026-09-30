import { createDraft, pair, type Draft, type ApiResponse } from "./request";
import type { HistoryEntry } from "./history";
import { resolveTokenDefinitions } from "./authorization";
import type { RequestGroup } from "./groups";
import { resolveForDisplay } from "./token-hints";

export type RequestSession = {
  id: number;
  groupId: number | null;
  draft: Draft;
  response: ApiResponse | null;
  busy: boolean;
  error: string;
  elapsed: number;
  sentFingerprint: string;
  view: RequestView;
  /** Past sends, newest first. */
  history?: HistoryEntry[];
  /** The request changed since the shown response was sent. Not saved. */
  stale?: boolean;
};
export type RequestView = {
  requestTab: string;
  responseTab: string;
  pretty: boolean;
  wrap: boolean;
  responseScroll: number;
};
export const createView = (): RequestView => ({
  requestTab: "query",
  responseTab: "body",
  pretty: true,
  wrap: false,
  responseScroll: 0,
});
let sequence = 0;
export function reserveSessionId(id: number) {
  sequence = Math.max(sequence, id);
}
export function draftFingerprint(draft: Draft) {
  const rows = (items: Draft["query"]) =>
    items.map(({ key, value, enabled, file }) => ({
      key,
      value,
      enabled,
      file,
    }));
  return JSON.stringify({
    ...draft,
    query: rows(draft.query),
    headers: rows(draft.headers),
    form: draft.form && rows(draft.form),
  });
}

/**
 * Fingerprint the fully-resolved RequestInput that was actually sent.
 * Includes effective auth type so that a group/global auth change that
 * changes the Authorization header marks the existing response stale.
 *
 * @param request  The resolved RequestInput, or null when build failed.
 * @param authType Optional resolved auth type tag for the fingerprint.
 */
export function requestFingerprint(
  request: import("./request").RequestInput | null,
  authType?: string,
): string {
  if (!request) return "";
  return JSON.stringify({
    method: request.method,
    url: request.url,
    headers: request.headers,
    body: request.body,
    bodyFile: request.bodyFile,
    multipart: request.multipart,
    _authType: authType,
  });
}
const emptyFingerprint = draftFingerprint(createDraft());
export function hasDraft(session: RequestSession) {
  return (
    draftFingerprint(session.draft) !== emptyFingerprint ||
    Boolean(session.response || session.error)
  );
}
export function createSession(source?: Draft): RequestSession {
  const cloneRows = (rows: Draft["query"]) =>
    rows.map((row) => ({
      ...pair(row.key, row.value),
      enabled: row.enabled,
      ...(row.file ? { file: true } : {}),
    }));
  return {
    id: ++sequence,
    groupId: null,
    draft: source
      ? {
          ...source,
          localAuth: source.localAuth ? { ...source.localAuth } : undefined,
          query: cloneRows(source.query),
          headers: cloneRows(source.headers),
          form: source.form && cloneRows(source.form),
        }
      : createDraft(),
    response: null,
    busy: false,
    error: "",
    elapsed: 0,
    sentFingerprint: "",
    view: createView(),
  };
}
/** Groups and global tokens, so labels can show resolved token values. */
export type LabelTokens = {
  groups: RequestGroup[];
  globalDefinitions: Record<string, string>;
};

function displayUrl(session: RequestSession, tokens?: LabelTokens) {
  if (!tokens) return session.draft.url;
  const ctx = resolveTokenDefinitions(
    session.groupId,
    tokens.groups,
    tokens.globalDefinitions,
  );
  return resolveForDisplay(session.draft.url, ctx);
}

export function sessionLabel(session: RequestSession, tokens?: LabelTokens) {
  try {
    const url = new URL(displayUrl(session, tokens));
    if (!["http:", "https:"].includes(url.protocol)) throw new Error();
    if (url.pathname === "/") return url.host;
    // Show token references such as {{id}} as typed, not percent-encoded.
    try {
      return decodeURI(url.pathname);
    } catch {
      return url.pathname;
    }
  } catch {
    return "Untitled " + String(session.id).padStart(2, "0");
  }
}

/**
 * Returns a short status label for the tab.
 * "Edited" / stale detection is handled by useRequestRunner.stale in the
 * response panel — sessionStatus just reflects transport state.
 */
export function sessionStatus(session: RequestSession) {
  if (session.busy) return "Sending";
  if (session.error) return "Failed";
  if (session.response) return String(session.response.status);
  return "Draft";
}
export function sessionHost(session: RequestSession, tokens?: LabelTokens) {
  try {
    const url = new URL(displayUrl(session, tokens));
    return ["http:", "https:"].includes(url.protocol) ? url.host : "";
  } catch {
    return "";
  }
}
