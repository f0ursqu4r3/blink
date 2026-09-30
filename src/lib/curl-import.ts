import { createDraft, isMethod, pair, type Draft, type Pair } from "./request";

/**
 * Parse a pasted cURL command into a draft. Supports the options that
 * describe a request: method, URL, headers, data, forms, and user auth.
 * Options that only change curl's own output or transport, such as -s or
 * --proxy, are returned in `ignored`.
 */

/** True when pasted text looks like a cURL command. */
export const isCurlCommand = (text: string) => /^\s*curl(\s|$)/.test(text);

/** Split a command line as a POSIX shell does, without expansion. */
export function shellWords(command: string): string[] {
  const words: string[] = [];
  let word = "";
  let inWord = false;
  let i = 0;
  const ansi: Record<string, string> = {
    n: "\n",
    t: "\t",
    r: "\r",
    "\\": "\\",
    "'": "'",
    '"': '"',
    "0": "\0",
  };
  while (i < command.length) {
    const char = command[i];
    if (char === "\\" && command[i + 1] === "\n") {
      i += 2;
      continue;
    }
    if (char === "\\" && command[i + 1] === "\r" && command[i + 2] === "\n") {
      i += 3;
      continue;
    }
    if (/\s/.test(char)) {
      if (inWord) words.push(word);
      word = "";
      inWord = false;
      i++;
      continue;
    }
    inWord = true;
    if (char === "'") {
      const end = command.indexOf("'", i + 1);
      if (end < 0) throw new Error("The cURL command has an unclosed quote.");
      word += command.slice(i + 1, end);
      i = end + 1;
    } else if (char === "$" && command[i + 1] === "'") {
      i += 2;
      while (i < command.length && command[i] !== "'") {
        if (command[i] === "\\" && i + 1 < command.length) {
          word += ansi[command[i + 1]] ?? command[i + 1];
          i += 2;
        } else word += command[i++];
      }
      if (i >= command.length)
        throw new Error("The cURL command has an unclosed quote.");
      i++;
    } else if (char === '"') {
      i++;
      while (i < command.length && command[i] !== '"') {
        if (command[i] === "\\" && '"\\$`\n'.includes(command[i + 1] ?? "")) {
          if (command[i + 1] !== "\n") word += command[i + 1];
          i += 2;
        } else word += command[i++];
      }
      if (i >= command.length)
        throw new Error("The cURL command has an unclosed quote.");
      i++;
    } else if (char === "\\") {
      word += command[i + 1] ?? "";
      i += 2;
    } else {
      word += char;
      i++;
    }
  }
  if (inWord) words.push(word);
  return words;
}

type Option =
  | "request"
  | "header"
  | "data"
  | "data-raw"
  | "data-urlencode"
  | "json"
  | "form"
  | "form-string"
  | "user"
  | "get"
  | "head"
  | "cookie"
  | "user-agent"
  | "referer"
  | "url";

const longOptions: Record<string, Option> = {
  "--request": "request",
  "--header": "header",
  "--data": "data",
  "--data-ascii": "data",
  "--data-binary": "data",
  "--data-raw": "data-raw",
  "--data-urlencode": "data-urlencode",
  "--json": "json",
  "--form": "form",
  "--form-string": "form-string",
  "--user": "user",
  "--get": "get",
  "--head": "head",
  "--cookie": "cookie",
  "--user-agent": "user-agent",
  "--referer": "referer",
  "--url": "url",
};
const shortOptions: Record<string, Option> = {
  X: "request",
  H: "header",
  d: "data",
  F: "form",
  u: "user",
  G: "get",
  I: "head",
  b: "cookie",
  A: "user-agent",
  e: "referer",
};
const flagOptions = new Set<Option>(["get", "head"]);
/** Ignored options that take a value, so the value is not read as the URL. */
const ignoredWithValue = new Set([
  "-o",
  "--output",
  "-m",
  "--max-time",
  "--connect-timeout",
  "--max-redirs",
  "-x",
  "--proxy",
  "--retry",
  "-w",
  "--write-out",
  "-c",
  "--cookie-jar",
  "--cacert",
  "--cert",
  "--key",
  "-E",
  "--resolve",
  "--connect-to",
  "--limit-rate",
  "-r",
  "--range",
]);
const ignoredShortWithValue = new Set(["o", "m", "x", "w", "c", "E", "r"]);

function header(text: string): Pair | null {
  const colon = text.indexOf(":");
  if (colon > 0)
    return pair(text.slice(0, colon).trim(), text.slice(colon + 1).trim());
  // "Name;" sends an empty header in curl.
  if (text.endsWith(";")) return pair(text.slice(0, -1).trim(), "");
  return null;
}

function decodeBasic(encoded: string) {
  try {
    const bytes = Uint8Array.from(atob(encoded), (c) => c.charCodeAt(0));
    const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    const colon = text.indexOf(":");
    return colon < 0
      ? null
      : { username: text.slice(0, colon), password: text.slice(colon + 1) };
  } catch {
    return null;
  }
}

function encodeDataUrlencode(value: string) {
  // curl: "name=content" encodes content; "=content" and "content" encode all.
  const equals = value.indexOf("=");
  if (equals > 0)
    return `${value.slice(0, equals)}=${encodeURIComponent(value.slice(equals + 1))}`;
  return encodeURIComponent(equals === 0 ? value.slice(1) : value);
}

export type CurlImport = { draft: Draft; ignored: string[] };

export function parseCurl(command: string): CurlImport {
  const words = shellWords(command.trim());
  if (words[0] !== "curl")
    throw new Error("Paste a command that starts with curl.");
  let method: string | undefined;
  let url = "";
  let get = false;
  let head = false;
  let json = false;
  const headers: Pair[] = [];
  const data: string[] = [];
  let dataFile: string | undefined;
  const form: Pair[] = [];
  let user: string | undefined;
  const ignored: string[] = [];

  const apply = (option: Option, value: string) => {
    switch (option) {
      case "request":
        method = value.toUpperCase();
        break;
      case "header": {
        const row = header(value);
        if (row) headers.push(row);
        break;
      }
      case "data":
        if (value.startsWith("@")) dataFile = value.slice(1);
        else data.push(value.replace(/[\r\n]/g, ""));
        break;
      case "data-raw":
        data.push(value);
        break;
      case "data-urlencode":
        data.push(encodeDataUrlencode(value));
        break;
      case "json":
        json = true;
        data.push(value);
        break;
      case "form":
      case "form-string": {
        const equals = value.indexOf("=");
        if (equals <= 0) break;
        const key = value.slice(0, equals);
        const content = value.slice(equals + 1);
        if (option === "form" && content.startsWith("@"))
          form.push({
            ...pair(key, content.slice(1).split(";")[0]),
            file: true,
          });
        else form.push(pair(key, content));
        break;
      }
      case "user":
        user = value;
        break;
      case "get":
        get = true;
        break;
      case "head":
        head = true;
        break;
      case "cookie":
        // A value without "=" is a cookie file, which Blink cannot read.
        if (value.includes("=")) headers.push(pair("Cookie", value));
        else ignored.push("--cookie");
        break;
      case "user-agent":
        headers.push(pair("User-Agent", value));
        break;
      case "referer":
        headers.push(pair("Referer", value));
        break;
      case "url":
        url = value;
        break;
    }
  };

  for (let i = 1; i < words.length; i++) {
    const word = words[i];
    const takeValue = () => {
      if (i + 1 >= words.length) throw new Error(`${word} needs a value.`);
      return words[++i];
    };
    if (word.startsWith("--")) {
      const option = longOptions[word];
      if (option) apply(option, flagOptions.has(option) ? "" : takeValue());
      else {
        ignored.push(word);
        if (ignoredWithValue.has(word)) i++;
      }
    } else if (word.startsWith("-") && word.length > 1) {
      // Short options can be grouped (-sSL) or carry a value (-XPOST).
      for (let at = 1; at < word.length; at++) {
        const letter = word[at];
        const option = shortOptions[letter];
        const rest = word.slice(at + 1);
        if (option && flagOptions.has(option)) apply(option, "");
        else if (option) {
          apply(option, rest || takeValue());
          break;
        } else {
          ignored.push(`-${letter}`);
          if (ignoredShortWithValue.has(letter)) {
            if (!rest) i++;
            break;
          }
        }
      }
    } else if (!url) url = word;
    else ignored.push(word);
  }
  if (!url) throw new Error("The cURL command has no URL.");

  const contentType =
    headers.find((row) => row.key.toLowerCase() === "content-type")?.value ??
    "";
  if (json) {
    if (!contentType) headers.push(pair("Content-Type", "application/json"));
    if (!headers.some((row) => row.key.toLowerCase() === "accept"))
      headers.push(pair("Accept", "application/json"));
  }
  const draft = createDraft();
  draft.url = url;
  draft.headers = headers.length ? headers : [pair()];
  draft.query = [pair()];
  const body = data.join("&");
  if (get && body) {
    url += (url.includes("?") ? "&" : "?") + body;
    draft.url = url;
  } else if (form.length) {
    draft.bodyMode = "multipart";
    draft.form = form;
    // Multipart sets its own Content-Type with the boundary.
    draft.headers = draft.headers.filter(
      (row) => row.key.toLowerCase() !== "content-type",
    );
    if (!draft.headers.length) draft.headers = [pair()];
  } else if (dataFile !== undefined) {
    draft.bodyMode = "file";
    draft.bodyFile = dataFile;
  } else if (data.length) {
    draft.body = body;
    draft.bodyMode =
      /json/i.test(contentType) || json || isJson(body) ? "json" : "text";
  }
  const sendsBody =
    !get && (form.length > 0 || dataFile !== undefined || data.length > 0);
  draft.method = method ?? (head ? "HEAD" : sendsBody ? "POST" : "GET");
  if (!isMethod(draft.method))
    throw new Error(`Invalid method: ${draft.method}`);

  // Move credentials to the Auth tab, where Blink builds the header.
  const authIndex = draft.headers.findIndex(
    (row) => row.key.toLowerCase() === "authorization",
  );
  const authValue = authIndex >= 0 ? draft.headers[authIndex].value : "";
  const bearer = /^Bearer\s+(\S+)$/i.exec(authValue);
  const basic = /^Basic\s+(\S+)$/i.exec(authValue);
  const decoded = basic && decodeBasic(basic[1]);
  if (user !== undefined) {
    const colon = user.indexOf(":");
    draft.localAuth = {
      type: "basic",
      username: colon < 0 ? user : user.slice(0, colon),
      password: colon < 0 ? "" : user.slice(colon + 1),
    };
  } else if (bearer) draft.localAuth = { type: "bearer", token: bearer[1] };
  else if (decoded) draft.localAuth = { type: "basic", ...decoded };
  if (draft.localAuth && (bearer || decoded)) {
    draft.headers.splice(authIndex, 1);
    if (!draft.headers.length) draft.headers = [pair()];
  }
  return { draft, ignored: [...new Set(ignored)] };
}

function isJson(text: string) {
  if (!/^\s*[[{]/.test(text)) return false;
  try {
    JSON.parse(text);
    return true;
  } catch {
    return false;
  }
}
