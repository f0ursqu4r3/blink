import { describe, expect, it } from "vitest";
import { isCurlCommand, parseCurl, shellWords } from "../curl-import";
import { buildRequest, createDraft, pair, toCurl } from "../request";

const rows = (items: { key: string; value: string }[]) =>
  items.map(({ key, value }) => [key, value]);

describe("shellWords", () => {
  it("handles quotes, escapes, and line continuations", () => {
    expect(
      shellWords(`curl 'a b' "c \\"d\\" \\$e" f\\ g \\\n  $'h\\nI' 'it'"'"'s'`),
    ).toEqual(["curl", "a b", 'c "d" $e', "f g", "h\nI", "it's"]);
  });

  it("rejects an unclosed quote", () => {
    expect(() => shellWords("curl 'oops")).toThrow("unclosed quote");
  });
});

describe("parseCurl", () => {
  it("detects cURL commands", () => {
    expect(isCurlCommand("  curl https://x")).toBe(true);
    expect(isCurlCommand("https://x/curl")).toBe(false);
  });

  it("reads method, URL, headers, and a JSON body", () => {
    const { draft } = parseCurl(
      `curl -X PATCH https://api.test/items/1 -H 'Content-Type: application/json' -H 'X-Trace: a:b' -d '{"name":"x"}'`,
    );
    expect(draft.method).toBe("PATCH");
    expect(draft.url).toBe("https://api.test/items/1");
    expect(rows(draft.headers)).toEqual([
      ["Content-Type", "application/json"],
      ["X-Trace", "a:b"],
    ]);
    expect(draft.bodyMode).toBe("json");
    expect(draft.body).toBe('{"name":"x"}');
  });

  it("defaults to POST with data and GET without", () => {
    expect(parseCurl("curl https://x -d a=1").draft.method).toBe("POST");
    expect(parseCurl("curl https://x").draft.method).toBe("GET");
    expect(parseCurl("curl -I https://x").draft.method).toBe("HEAD");
  });

  it("joins data flags and moves them to the URL with --get", () => {
    const { draft } = parseCurl(
      "curl -G https://x/s?a=1 -d q=blue --data-urlencode 'name=a b'",
    );
    expect(draft.method).toBe("GET");
    expect(draft.url).toBe("https://x/s?a=1&q=blue&name=a%20b");
  });

  it("reads grouped short flags and attached values", () => {
    const { draft, ignored } = parseCurl(
      "curl -sSL -XPUT -HAccept:text/plain -o out.txt https://x",
    );
    expect(draft.method).toBe("PUT");
    expect(draft.url).toBe("https://x");
    expect(rows(draft.headers)).toEqual([["Accept", "text/plain"]]);
    expect(ignored).toEqual(["-s", "-S", "-L", "-o"]);
  });

  it("moves bearer and basic credentials to the Auth tab", () => {
    const bearer = parseCurl(
      "curl https://x -H 'Authorization: Bearer abc.def'",
    ).draft;
    expect(bearer.localAuth).toEqual({ type: "bearer", token: "abc.def" });
    expect(rows(bearer.headers)).toEqual([["", ""]]);
    const user = parseCurl("curl -u ada:s3:cret https://x").draft;
    expect(user.localAuth).toEqual({
      type: "basic",
      username: "ada",
      password: "s3:cret",
    });
    const header = parseCurl(
      `curl https://x -H "Authorization: Basic ${btoa("ada:pw")}"`,
    ).draft;
    expect(header.localAuth).toEqual({
      type: "basic",
      username: "ada",
      password: "pw",
    });
  });

  it("reads multipart forms with files", () => {
    const { draft } = parseCurl(
      "curl https://x -F title=Hi -F 'upload=@/tmp/a.png;type=image/png' -H 'Content-Type: multipart/form-data'",
    );
    expect(draft.method).toBe("POST");
    expect(draft.bodyMode).toBe("multipart");
    expect(draft.form).toMatchObject([
      { key: "title", value: "Hi" },
      { key: "upload", value: "/tmp/a.png", file: true },
    ]);
    expect(rows(draft.headers)).toEqual([["", ""]]);
  });

  it("reads --json and a file body", () => {
    const json = parseCurl(`curl --json '{"a":1}' https://x`).draft;
    expect(json.bodyMode).toBe("json");
    expect(rows(json.headers)).toEqual([
      ["Content-Type", "application/json"],
      ["Accept", "application/json"],
    ]);
    const file = parseCurl("curl --data-binary @dump.bin https://x").draft;
    expect(file.bodyMode).toBe("file");
    expect(file.bodyFile).toBe("dump.bin");
  });

  it("explains a command without a URL", () => {
    expect(() => parseCurl("curl -s")).toThrow("no URL");
    expect(() => parseCurl("curl -X")).toThrow("-X needs a value.");
  });

  it("round-trips a Blink cURL export", () => {
    const source = {
      ...createDraft(),
      method: "POST",
      url: "https://api.test/items?x=1",
      headers: [pair("Accept", "application/json"), pair("X-A", "it's")],
      bodyMode: "json" as const,
      body: '{"a": "b c"}',
      localAuth: { type: "bearer" as const, token: "tok" },
    };
    const exported = buildRequest(source);
    const imported = buildRequest(parseCurl(toCurl(exported)).draft);
    const sorted = (request: typeof exported) => ({
      ...request,
      headers: [...request.headers].sort((a, b) => a.key.localeCompare(b.key)),
    });
    expect(sorted(imported)).toEqual(sorted(exported));
  });
});
