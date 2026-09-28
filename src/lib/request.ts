import { interpolate } from "./interpolation";
import type { InterpolationContext } from "./interpolation";
import type { ResolvedRequestContext } from "./authorization";

export const methods = [
  "GET",
  "POST",
  "PUT",
  "PATCH",
  "DELETE",
  "HEAD",
  "OPTIONS",
] as const;
export type Method = (typeof methods)[number];
export type Pair = { id: number; key: string; value: string; enabled: boolean };
export type Header = { key: string; value: string };
export const bodyModes = ["none", "json", "text", "graphql"] as const;
export type BodyMode = (typeof bodyModes)[number];
export type Draft = {
  method: Method;
  url: string;
  query: Pair[];
  headers: Pair[];
  bodyMode: BodyMode;
  body: string;
  /** GraphQL variables as JSON text. Used only in graphql body mode. */
  variables?: string;
  auth: "none" | "bearer" | "basic";
  token: string;
  username: string;
  password: string;
  /** Structured local auth override. Undefined = inherit from group. */
  localAuth?: import("./authorization").AuthorizationConfig | undefined;
};
export type RequestInput = {
  method: Method;
  url: string;
  headers: Header[];
  body: string | null;
};
export type ApiResponse = {
  status: number;
  statusText: string;
  durationMs: number;
  headers: Header[];
  /** Preview text: complete unless `truncated`; empty when `binary`. */
  body: string;
  /** Full body size, not the preview size. */
  sizeBytes: number;
  /** Stored raw body. Not saved in the workspace. */
  bodyId?: string;
  truncated?: boolean;
  binary?: boolean;
  /** Set only when a redirect happened. */
  finalUrl?: string;
  /** Desktop only. */
  redirectCount?: number;
};
let nextId = 0;
export function reservePairId(id: number) {
  nextId = Math.max(nextId, id);
}
export const pair = (key = "", value = ""): Pair => ({
  id: ++nextId,
  key,
  value,
  enabled: true,
});
export const createDraft = (): Draft => ({
  method: "GET",
  url: "",
  query: [pair()],
  headers: [pair("Accept", "application/json")],
  bodyMode: "none",
  body: "",
  auth: "none",
  token: "",
  username: "",
  password: "",
});
export const activePairs = (rows: Pair[]) =>
  rows.filter((row) => row.enabled && row.key.trim());
export const supportsBody = (method: string) =>
  method !== "GET" && method !== "HEAD";

export function buildRequest(
  draft: Draft,
  ctx?: ResolvedRequestContext | InterpolationContext,
): RequestInput {
  const interp = ctx ? (s: string) => interpolate(s, ctx) : (s: string) => s;

  const rawUrl = interp(draft.url.trim());

  let target: URL;
  try {
    target = new URL(rawUrl);
  } catch {
    throw new Error("Enter an absolute HTTP or HTTPS URL.");
  }
  if (!["http:", "https:"].includes(target.protocol))
    throw new Error("Only HTTP and HTTPS URLs are supported.");
  if (!/^https?:\/\/[^/\\\s]/i.test(rawUrl))
    throw new Error("Enter an absolute URL starting with http:// or https://.");
  if (target.username || target.password)
    throw new Error("Use the Auth tab instead of credentials in the URL.");
  let url = rawUrl.split("#")[0];
  const query = activePairs(draft.query);
  if (query.length) {
    const additions = new URLSearchParams(
      query.map(({ key, value }) => [interp(key), interp(value)]),
    );
    const separator = url.includes("?") ? (/[?&]$/.test(url) ? "" : "&") : "?";
    url += separator + additions.toString();
  }
  const headers = activePairs(draft.headers).map(({ key, value }) => ({
    key: key.trim(),
    value: interp(value),
  }));
  for (const header of headers) {
    if (!/^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/.test(header.key))
      throw new Error(`Invalid header name: ${header.key}`);
    if (/[\r\n]/.test(header.value))
      throw new Error(`Line breaks are not allowed in ${header.key}.`);
  }

  const resolvedAuth =
    (ctx as ResolvedRequestContext | undefined)?.auth ??
    draft.localAuth ??
    (draft.auth === "bearer"
      ? { type: "bearer" as const, token: draft.token }
      : draft.auth === "basic"
        ? {
            type: "basic" as const,
            username: draft.username,
            password: draft.password,
          }
        : { type: "none" as const });
  const effectiveAuthType = resolvedAuth.type;

  if (effectiveAuthType !== "none") {
    if (headers.some((h) => h.key.toLowerCase() === "authorization"))
      throw new Error("Remove the Authorization header or select No auth.");
    if (effectiveAuthType === "bearer") {
      const token =
        resolvedAuth.type === "bearer" ? interp(resolvedAuth.token).trim() : "";
      if (!token || /\s/.test(token))
        throw new Error("Enter a bearer token without spaces or line breaks.");
      headers.push({
        key: "Authorization",
        value: `Bearer ${token}`,
      });
    } else {
      const username =
        resolvedAuth.type === "basic" ? interp(resolvedAuth.username) : "";
      const password =
        resolvedAuth.type === "basic" ? interp(resolvedAuth.password) : "";
      if (username.includes(":"))
        throw new Error("Basic auth usernames cannot contain a colon.");
      const bytes = new TextEncoder().encode(`${username}:${password}`);
      headers.push({
        key: "Authorization",
        value: `Basic ${btoa(Array.from(bytes, (b) => String.fromCharCode(b)).join(""))}`,
      });
    }
  }

  let body =
    supportsBody(draft.method) && draft.bodyMode !== "none" ? draft.body : null;
  if (body !== null && ctx) {
    body = interp(body);
  }
  if (body !== null && draft.bodyMode === "json") {
    try {
      JSON.parse(body);
    } catch {
      throw new Error("Invalid JSON body. Fix the JSON or select Text.");
    }
  }
  if (body !== null && draft.bodyMode === "graphql") {
    if (!body.trim()) throw new Error("Enter a GraphQL query.");
    const rawVariables = interp(draft.variables ?? "");
    let variables: unknown;
    if (rawVariables.trim()) {
      try {
        variables = JSON.parse(rawVariables);
      } catch {
        variables = null;
      }
      if (
        !variables ||
        typeof variables !== "object" ||
        Array.isArray(variables)
      )
        throw new Error("GraphQL variables must be a JSON object.");
    }
    body = JSON.stringify(
      variables === undefined ? { query: body } : { query: body, variables },
    );
  }
  if (
    body !== null &&
    !headers.some((h) => h.key.toLowerCase() === "content-type")
  )
    headers.push({
      key: "Content-Type",
      value:
        draft.bodyMode === "json" || draft.bodyMode === "graphql"
          ? "application/json"
          : "text/plain; charset=utf-8",
    });
  return { method: draft.method, url, headers, body };
}

const quote = (value: string) => "'" + value.replace(/'/g, "'\"'\"'") + "'";
export function toCurl(request: RequestInput) {
  const lines = [
    "curl --disable --globoff --max-time 30",
    request.method === "HEAD" ? "--head" : `--request ${request.method}`,
    `--url ${quote(request.url)}`,
  ];
  request.headers.forEach(({ key, value }) =>
    lines.push(`--header ${quote(`${key}: ${value}`)}`),
  );
  if (request.body !== null) lines.push(`--data-raw ${quote(request.body)}`);
  return lines.join(" \\\n  ");
}
export function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MiB`;
}
