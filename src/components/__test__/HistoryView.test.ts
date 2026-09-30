import { describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import HistoryView from "../HistoryView.vue";
import type { HistoryEntry } from "@/lib/history";

const entry = (id: number, body: string, status = 200): HistoryEntry => ({
  id,
  sentAt: 1_700_000_000_000 + id * 1000,
  method: "GET",
  url: "https://example.test/items",
  status,
  statusText: "OK",
  durationMs: id,
  sizeBytes: body.length,
  headers: [{ key: "X-Id", value: String(id) }],
  body,
});

describe("HistoryView", () => {
  const history = [
    entry(3, '{"a":3}', 500),
    entry(2, '{"a":2}'),
    entry(1, '{"a":2}'),
  ];
  it("lists sends newest first", () => {
    const view = mount(HistoryView, { props: { history } });
    expect(
      view
        .findAll("[data-history-entry]")
        .map((row) => row.attributes("data-history-entry")),
    ).toEqual(["3", "2", "1"]);
    expect(view.text()).toContain("3 SENDS");
  });
  it("compares a send with the one before it", async () => {
    const view = mount(HistoryView, { props: { history } });
    await view.get('[data-history-entry="3"]').trigger("click");
    const lines = view.findAll(".diff-line");
    expect(
      lines
        .filter((l) => l.attributes("data-kind") === "remove")
        .map((l) => l.text()),
    ).toEqual(expect.arrayContaining([expect.stringContaining('"a": 2')]));
    expect(
      lines
        .filter((l) => l.attributes("data-kind") === "add")
        .map((l) => l.text()),
    ).toEqual(expect.arrayContaining([expect.stringContaining('"a": 3')]));
    await view.get("[data-history-back]").trigger("click");
    expect(view.find("[data-diff]").exists()).toBe(false);
  });
  it("reports an unchanged body", async () => {
    const view = mount(HistoryView, { props: { history } });
    await view.get('[data-history-entry="2"]').trigger("click");
    expect(view.get("[data-diff]").text()).toContain("Body · No changes");
  });
  it("emits clear", async () => {
    const view = mount(HistoryView, { props: { history } });
    await view.get("[data-history-clear]").trigger("click");
    expect(view.emitted("clear")).toHaveLength(1);
  });
});
