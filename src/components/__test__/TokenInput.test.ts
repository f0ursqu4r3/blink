import { describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import { nextTick } from "vue";
import TokenInput from "../TokenInput.vue";

const tokens = {
  definitions: { endpoint: "users" },
  workspaceDefinitions: { host: "api.test" },
};

function render(modelValue = "") {
  const w = mount(TokenInput, {
    props: {
      modelValue,
      tokens,
      "onUpdate:modelValue": (next: string) => w.setProps({ modelValue: next }),
    },
    attrs: { "data-field": "", class: "px-3" },
    attachTo: document.body,
  });
  return w;
}

async function type(input: HTMLInputElement, value: string) {
  input.value = value;
  input.setSelectionRange(value.length, value.length);
  input.dispatchEvent(new Event("input"));
  await nextTick();
}

function suggestions() {
  return Array.from(
    document.querySelectorAll("[data-token-suggestions] [role=option]"),
  ).map((el) => el.firstElementChild?.textContent?.trim());
}

describe("TokenInput", () => {
  it("passes attributes and classes to the native input", () => {
    const w = render();
    const input = w.get("input");
    expect(input.attributes("data-field")).toBe("");
    expect(input.classes()).toContain("px-3");
    w.unmount();
  });

  it("colors resolved and unresolved references", () => {
    const w = render("https://x/{{endpoint}}/{{missing}}");
    const marks = w
      .findAll("[data-token]")
      .map((el) => [el.text(), el.attributes("data-token")]);
    expect(marks).toEqual([
      ["users", "resolved"],
      ["{{missing}}", "unresolved"],
    ]);
    w.unmount();
  });

  it("shows no backdrop for text without references", () => {
    const w = render("https://x/users");
    expect(w.find("[data-token-backdrop]").exists()).toBe(false);
    w.unmount();
  });

  it("suggests tokens after {{ and inserts the chosen name", async () => {
    let value = "";
    const w = mount(TokenInput, {
      props: {
        modelValue: "",
        tokens,
        "onUpdate:modelValue": (next: string) => {
          value = next;
          void w.setProps({ modelValue: next });
        },
      },
      attachTo: document.body,
    });
    const input = w.get("input").element;

    await type(input, "https://x/{{en");
    expect(suggestions()).toEqual(["endpoint"]);
    expect(
      document.querySelector("[data-token-suggestions] [role=option]")
        ?.textContent,
    ).toContain("users");
    expect(input.getAttribute("aria-expanded")).toBe("true");

    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
    await nextTick();
    expect(value).toBe("https://x/{{endpoint}}");
    expect(suggestions()).toEqual([]);
    w.unmount();
  });

  it("keeps Enter from submitting while suggestions are open", async () => {
    const w = render();
    const input = w.get("input").element;
    await type(input, "{{");
    const event = new KeyboardEvent("keydown", {
      key: "Enter",
      cancelable: true,
    });
    input.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
    w.unmount();
  });

  it("closes suggestions on Escape", async () => {
    const w = render();
    const input = w.get("input").element;
    await type(input, "{{");
    expect(suggestions().length).toBe(3);
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    await nextTick();
    expect(suggestions()).toEqual([]);
    w.unmount();
  });

  it("shows the token value when the pointer is over a token", async () => {
    const w = render("{{endpoint}}");
    const input = w.get("input").element;
    // jsdom has no layout and a plain event has no clientX, so no bounds
    // check fails and the pointer lands in the first span.
    input.dispatchEvent(new Event("pointermove"));
    await nextTick();
    expect(document.querySelector("[data-token-hint]")?.textContent).toContain(
      "endpoint = users",
    );
    input.dispatchEvent(new Event("pointerleave"));
    await nextTick();
    expect(document.querySelector("[data-token-hint]")).toBeNull();
    w.unmount();
  });

  it("shows a defined token as its value and keeps the raw text", async () => {
    let value = "https://x/{{endpoint}}";
    const w = mount(TokenInput, {
      props: {
        modelValue: value,
        tokens,
        "onUpdate:modelValue": (next: string) => {
          value = next;
          void w.setProps({ modelValue: next });
        },
      },
      attachTo: document.body,
    });
    const input = w.get("input").element;
    expect(input.value).toBe("https://x/users");

    // Backspace at the end of the value turns it back into its reference.
    input.setSelectionRange(15, 15);
    input.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Backspace", cancelable: true }),
    );
    await nextTick();
    await nextTick();
    expect(value).toBe("https://x/{{endpoint}");
    expect(input.value).toBe("https://x/{{endpoint}");
    expect(input.selectionStart).toBe(21);

    // Closing the braces shows the value again.
    await type(input, "https://x/{{endpoint}}");
    await nextTick();
    expect(value).toBe("https://x/{{endpoint}}");
    expect(input.value).toBe("https://x/users");
    w.unmount();
  });

  it("maps typing after a shown value to the raw text", async () => {
    let value = "{{endpoint}}";
    const w = mount(TokenInput, {
      props: {
        modelValue: value,
        tokens,
        "onUpdate:modelValue": (next: string) => {
          value = next;
          void w.setProps({ modelValue: next });
        },
      },
      attachTo: document.body,
    });
    const input = w.get("input").element;
    await type(input, "users/1");
    expect(value).toBe("{{endpoint}}/1");
    w.unmount();
  });
});
