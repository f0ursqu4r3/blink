import { afterEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { reactive } from "vue";
import RequestEditor from "../RequestEditor.vue";
import { createDraft, pair, type BodyMode, type Draft } from "@/lib/request";

vi.mock("../CodeEditor.vue", () => import("./code-editor-stub"));
vi.mock("@/lib/request-files", () => ({
  pickRequestFile: vi.fn(),
}));
import { pickRequestFile } from "@/lib/request-files";
const pick = vi.mocked(pickRequestFile);

const wrappers: ReturnType<typeof mount>[] = [];
afterEach(() => {
  wrappers.splice(0).forEach((w) => w.unmount());
  pick.mockReset();
});

function mountEditor(bodyMode: BodyMode, changes: Partial<Draft> = {}) {
  const draft = reactive({
    ...createDraft(),
    method: "POST",
    url: "https://api.test/upload",
    bodyMode,
    ...changes,
  });
  const wrapper = mount(RequestEditor, {
    attachTo: document.body,
    props: { modelValue: draft, tab: "body", busy: false },
  });
  wrappers.push(wrapper);
  return { wrapper, draft };
}

describe("RequestEditor – body modes", () => {
  it("offers form, multipart, and file bodies", () => {
    const { wrapper } = mountEditor("none");
    const values = wrapper
      .findAll("[data-body-actions] select option")
      .map((option) => option.attributes("value"));
    expect(values).toEqual([
      "none",
      "json",
      "text",
      "graphql",
      "form",
      "multipart",
      "file",
    ]);
  });

  it("starts form rows with one blank row and hides file controls", async () => {
    const { wrapper, draft } = mountEditor("form");
    await flushPromises();
    expect(draft.form).toHaveLength(1);
    expect(wrapper.find("[data-form-editor]").exists()).toBe(true);
    expect(wrapper.find("[data-add-file]").exists()).toBe(false);
  });

  it("adds a picked file as a multipart part", async () => {
    pick.mockResolvedValueOnce({
      path: "/data/photo.png",
      name: "photo.png",
      sizeBytes: 7,
    });
    const { wrapper, draft } = mountEditor("multipart", {
      form: [pair("title", "Hi")],
    });
    await wrapper.get("[data-add-file]").trigger("click");
    await flushPromises();
    expect(draft.form?.[1]).toMatchObject({
      key: "photo",
      value: "/data/photo.png",
      file: true,
    });
    expect(wrapper.get("[data-row-file]").text()).toContain("photo.png");
  });

  it("stores a picked file body and shows its size", async () => {
    pick.mockResolvedValueOnce({
      path: "/data/dump.bin",
      name: "dump.bin",
      sizeBytes: 2048,
    });
    const { wrapper, draft } = mountEditor("file");
    await wrapper.get("[data-body-file] button").trigger("click");
    await flushPromises();
    expect(draft.bodyFile).toBe("/data/dump.bin");
    expect(wrapper.get("[data-body-file]").text()).toContain("dump.bin");
    expect(wrapper.get("[data-body-file]").text()).toContain("2.0 KiB");
  });

  it("shows why a file cannot be picked", async () => {
    pick.mockRejectedValueOnce(new Error("Use the desktop app."));
    const { wrapper } = mountEditor("file");
    await wrapper.get("[data-body-file] button").trigger("click");
    await flushPromises();
    expect(wrapper.get("[role='alert']").text()).toBe("Use the desktop app.");
  });
});
