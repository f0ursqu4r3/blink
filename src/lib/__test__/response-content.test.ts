import { describe, expect, it } from "vitest";
import { responseLanguage } from "../response-content";

describe("response syntax languages", () => {
  it("maps common HTTP content types to syntax languages", () => {
    expect(responseLanguage("application/problem+json")).toBe("json");
    expect(responseLanguage("application/xml")).toBe("xml");
    expect(responseLanguage("text/html; charset=utf-8")).toBe("xml");
    expect(responseLanguage("text/css")).toBe("css");
    expect(responseLanguage("application/javascript")).toBe("javascript");
    expect(responseLanguage("application/yaml")).toBe("yaml");
    expect(responseLanguage("text/plain")).toBe("plaintext");
  });
});

describe("highlightSource", () => {
  it("highlights code and escapes it", async () => {
    const { highlightSource } = await import("../response-content");
    const html = highlightSource('import requests\nx = "<b>"', "python");
    expect(html).toContain('<span class="hljs-keyword">import</span>');
    expect(html).toContain("&lt;b&gt;");
    expect(html).not.toContain("<b>");
    expect(highlightSource("<x>", "nope")).toBe("&lt;x&gt;");
  });
});
