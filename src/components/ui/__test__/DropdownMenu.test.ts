import { describe, expect, it } from "vitest";
import { enableAutoUnmount, mount } from "@vue/test-utils";
import { afterEach } from "vitest";
import { defineComponent } from "vue";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
} from "../dropdown-menu";

enableAutoUnmount(afterEach);

describe("DropdownMenu", () => {
  it("uses the shared menu surface", async () => {
    const w = mount(
      defineComponent({
        components: {
          DropdownMenu,
          DropdownMenuTrigger,
          DropdownMenuContent,
          DropdownMenuItem,
        },
        template: `<DropdownMenu><DropdownMenuTrigger data-trigger>Open</DropdownMenuTrigger>
          <DropdownMenuContent><DropdownMenuItem>One</DropdownMenuItem></DropdownMenuContent></DropdownMenu>`,
      }),
      { attachTo: document.body },
    );
    await w.get("[data-trigger]").trigger("keydown", { key: "Enter" });
    const surface = document.body.querySelector(
      '[data-surface="context-menu"]',
    );
    expect(surface?.className).toContain("bg-popover");
    expect(surface?.className).toContain("font-sans");
  });
});
