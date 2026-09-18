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
export type Draft = {
  method: Method;
  url: string;
  query: Pair[];
  headers: Pair[];
  bodyMode: "none" | "json" | "text";
  body: string;
  auth: "none" | "bearer" | "basic";
  token: string;
  username: string;
  password: string;
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
  body: string;
  sizeBytes: number;
};
let nextId = 0;
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

export function buildRequest(draft: Draft): RequestInput {
  let target: URL;
  try {
    target = new URL(draft.url.trim());
  } catch {
    throw new Error("Enter an absolute HTTP or HTTPS URL.");
  }
  if (!["http:", "https:"].includes(target.protocol))
    throw new Error("Only HTTP and HTTPS URLs are supported.");
  if (!/^https?:\/\/[^/\\\s]/i.test(draft.url.trim()))
    throw new Error("Enter an absolute URL starting with http:// or https://.");
  if (target.username || target.password)
    throw new Error("Use the Auth tab instead of credentials in the URL.");
  // Append only new rows; do not re-encode the original URL or query string.
  let url = draft.url.trim().split("#")[0];
  const query = activePairs(draft.query);
  if (query.length) {
    const additions = new URLSearchParams(
      query.map(({ key, value }) => [key, value]),
    );
    const separator = url.includes("?") ? (/[?&]$/.test(url) ? "" : "&") : "?";
    url += separator + additions.toString();
  }
  const headers = activePairs(draft.headers).map(({ key, value }) => ({
    key: key.trim(),
    value,
  }));
  for (const header of headers) {
    if (!/^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/.test(header.key))
      throw new Error(`Invalid header name: ${header.key}`);
    if (/[\r\n]/.test(header.value))
      throw new Error(`Line breaks are not allowed in ${header.key}.`);
  }
  if (draft.auth !== "none") {
    if (headers.some((h) => h.key.toLowerCase() === "authorization"))
      throw new Error("Remove the Authorization header or select No auth.");
    if (draft.auth === "bearer") {
      if (!draft.token.trim() || /\s/.test(draft.token.trim()))
        throw new Error("Enter a bearer token without spaces or line breaks.");
      headers.push({
        key: "Authorization",
        value: `Bearer ${draft.token.trim()}`,
      });
    } else {
      if (draft.username.includes(":"))
        throw new Error("Basic auth usernames cannot contain a colon.");
      const bytes = new TextEncoder().encode(
        `${draft.username}:${draft.password}`,
      );
      headers.push({
        key: "Authorization",
        value: `Basic ${btoa(Array.from(bytes, (b) => String.fromCharCode(b)).join(""))}`,
      });
    }
  }
  const body =
    supportsBody(draft.method) && draft.bodyMode !== "none" ? draft.body : null;
  if (body !== null && draft.bodyMode === "json") {
    try {
      JSON.parse(body);
    } catch {
      throw new Error("Invalid JSON body. Fix the JSON or select Text.");
    }
  }
  if (
    body !== null &&
    !headers.some((h) => h.key.toLowerCase() === "content-type")
  )
    headers.push({
      key: "Content-Type",
      value:
        draft.bodyMode === "json"
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
