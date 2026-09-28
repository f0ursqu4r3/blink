import { afterEach, describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { nextTick } from "vue";
import RequestEditor from "../RequestEditor.vue";
import { createDraft } from "@/lib/request";

vi.mock("../CodeEditor.vue", () => import("./code-editor-stub"));

const wrappers: ReturnType<typeof mount>[] = [];
afterEach(() => wrappers.splice(0).forEach((w) => w.unmount()));

function makeDraft(overrides?: object) {
  return { ...createDraft(), ...overrides };
}

function mountEditor(draftOverrides?: object, propsOverrides?: object) {
  const draft = makeDraft(draftOverrides);
  const wrapper = mount(RequestEditor, {
    attachTo: document.body,
    props: {
      modelValue: draft,
      tab: "auth",
      busy: false,
      ...propsOverrides,
    },
  });
  wrappers.push(wrapper);
  return wrapper;
}

/** Find the Auth tab trigger button by its text */
function getAuthTab(wrapper: ReturnType<typeof mount>) {
  return wrapper
    .findAll(".tab-trigger")
    .find((b) => b.text().startsWith("Auth"))!;
}

describe("RequestEditor – Inherit auth option", () => {
  it("shows Inherit option in auth select", async () => {
    const wrapper = mountEditor();

    const select = wrapper.find("select[id$='-auth-type']");
    expect(select.exists()).toBe(true);
    const options = select.findAll("option");
    const values = options.map((o) => o.element.value);
    expect(values).toContain("inherit");
  });

  it("shows Inherit as selected when draft.localAuth is undefined", async () => {
    const wrapper = mountEditor({ localAuth: undefined });

    const select = wrapper.find<HTMLSelectElement>("select[id$='-auth-type']");
    expect(select.element.value).toBe("inherit");
  });

  it("shows bearer as selected when draft.localAuth is bearer", async () => {
    const wrapper = mountEditor({
      localAuth: { type: "bearer", token: "tok" },
    });

    const select = wrapper.find<HTMLSelectElement>("select[id$='-auth-type']");
    expect(select.element.value).toBe("bearer");
  });

  it("shows Effective note without token when in inherit mode with bearer effectiveAuth", async () => {
    const wrapper = mountEditor(
      { localAuth: undefined },
      { effectiveAuth: { type: "bearer", token: "super-secret" } },
    );

    const note = wrapper.find("[data-testid='effective-auth-note']");
    expect(note.exists()).toBe(true);
    expect(note.text()).toContain("bearer");
    // must NOT show the token value
    expect(wrapper.html()).not.toContain("super-secret");
  });

  it("auth tab badge counts inherited active auth", async () => {
    const wrapper = mountEditor(
      { localAuth: undefined },
      { effectiveAuth: { type: "bearer", token: "tok" } },
    );

    const authTabTrigger = getAuthTab(wrapper);
    expect(authTabTrigger).toBeDefined();
    const badge = authTabTrigger.find(".tab-count");
    expect(badge.exists()).toBe(true);
    expect(badge.text()).toBe("1");
  });

  it("auth tab badge is 0 when inherited effective auth is none", async () => {
    const wrapper = mountEditor(
      { localAuth: undefined },
      { effectiveAuth: { type: "none" } },
    );

    const authTabTrigger = getAuthTab(wrapper);
    expect(authTabTrigger).toBeDefined();
    const badge = authTabTrigger.find(".tab-count");
    expect(badge.exists()).toBe(false);
  });

  it("setting Bearer from Inherit sets draft.localAuth via emitted update", async () => {
    const draft = makeDraft({ localAuth: undefined });
    const wrapper = mount(RequestEditor, {
      attachTo: document.body,
      props: { modelValue: draft, tab: "auth", busy: false },
    });
    wrappers.push(wrapper);

    const select = wrapper.find<HTMLSelectElement>("select[id$='-auth-type']");
    await select.setValue("bearer");
    await select.trigger("change");
    await nextTick();

    const emitted = wrapper.emitted("update:modelValue");
    expect(emitted).toBeTruthy();
    const last = (emitted as unknown[][])[emitted!.length - 1][0] as Record<
      string,
      unknown
    >;
    expect((last.localAuth as { type: string }).type).toBe("bearer");
  });

  it("opens body actions from the non-editable body toolbar", async () => {
    const wrapper = mountEditor(
      { bodyMode: "json", body: '{"a":1}' },
      { tab: "body" },
    );

    await wrapper.find("[data-body-actions]").trigger("contextmenu");

    expect(document.body.textContent).toContain("Format JSON");
    expect(document.body.textContent).toContain("Clear body");
  });

  it("keeps the native context menu for body text input", () => {
    const wrapper = mountEditor(
      { bodyMode: "text", body: "hello" },
      { tab: "body" },
    );
    const textarea = wrapper.find("textarea");
    const event = new MouseEvent("contextmenu", {
      bubbles: true,
      cancelable: true,
    });

    textarea.element.dispatchEvent(event);

    expect(event.defaultPrevented).toBe(false);
  });
});
