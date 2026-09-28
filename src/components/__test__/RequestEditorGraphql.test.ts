import { afterEach, describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import RequestEditor from "../RequestEditor.vue";
import { createDraft } from "@/lib/request";

const wrappers: ReturnType<typeof mount>[] = [];
afterEach(() => wrappers.splice(0).forEach((w) => w.unmount()));

function mountEditor(bodyMode: "json" | "graphql") {
  const draft = { ...createDraft(), method: "POST" as const, bodyMode };
  const wrapper = mount(RequestEditor, {
    attachTo: document.body,
    props: { modelValue: draft, tab: "body", busy: false },
  });
  wrappers.push(wrapper);
  return { wrapper, draft };
}

describe("RequestEditor – GraphQL body", () => {
  it("offers GraphQL in the body mode select", () => {
    const { wrapper } = mountEditor("json");
    const options = wrapper
      .findAll("select[id$='-body-mode'] option")
      .map((o) => o.attributes("value"));
    expect(options).toContain("graphql");
  });
  it("shows a variables editor only in GraphQL mode", async () => {
    const { wrapper, draft } = mountEditor("json");
    expect(wrapper.find("textarea[id$='-variables']").exists()).toBe(false);
    await wrapper.setProps({ modelValue: { ...draft, bodyMode: "graphql" } });
    const variables = wrapper.find("textarea[id$='-variables']");
    expect(variables.exists()).toBe(true);
    expect(wrapper.find("label[for$='-body']").text()).toBe("GraphQL query");
    await variables.setValue('{"id":"1"}');
    expect(wrapper.props("modelValue").variables).toBe('{"id":"1"}');
  });
  it("formats the GraphQL query and variables", async () => {
    const { wrapper } = mountEditor("graphql");
    await wrapper.setProps({
      modelValue: {
        ...wrapper.props("modelValue"),
        body: "{viewer{id}}",
        variables: '{"a":1}',
      },
    });
    await wrapper.find("[data-body-actions] button").trigger("click");
    await vi.waitFor(() =>
      expect(wrapper.props("modelValue").body).toBe(
        "{\n  viewer {\n    id\n  }\n}",
      ),
    );
    expect(wrapper.props("modelValue").variables).toBe('{\n  "a": 1\n}');
  });
  it("keeps the body unchanged when GraphQL is invalid", async () => {
    const { wrapper } = mountEditor("graphql");
    await wrapper.setProps({
      modelValue: { ...wrapper.props("modelValue"), body: "query {" },
    });
    await wrapper.find("[data-body-actions] button").trigger("click");
    await vi.waitFor(() =>
      expect(wrapper.find("[role='alert']").text()).toContain(
        "Invalid GraphQL",
      ),
    );
    expect(wrapper.props("modelValue").body).toBe("query {");
  });
});
