import { describe, expect, it, vi } from "vitest";
import { WorkspaceWriter } from "../workspace-storage";

describe("ordered workspace writes", () => {
  it("coalesces changes behind an in-flight write and leaves the latest snapshot on disk", async () => {
    let release!: () => void;
    let disk = "";
    const writes: string[] = [];
    const write = vi.fn(async (content: string) => {
      writes.push(content);
      if (content === "first")
        await new Promise<void>((resolve) => {
          release = resolve;
        });
      disk = content;
    });
    const writer = new WorkspaceWriter(write);
    const first = writer.save("first");
    await vi.waitFor(() => expect(writes).toEqual(["first"]));
    const second = writer.save("second");
    const third = writer.save("latest");
    release();
    await Promise.all([first, second, third]);
    expect(writes).toEqual(["first", "latest"]);
    expect(disk).toBe("latest");
  });
  it("surfaces failed writes and allows a newer snapshot to retry", async () => {
    const write = vi
      .fn()
      .mockRejectedValueOnce(new Error("disk full"))
      .mockResolvedValue(undefined);
    const writer = new WorkspaceWriter(write);
    await expect(writer.save("old")).rejects.toThrow("disk full");
    await writer.save("latest");
    expect(write.mock.calls).toEqual([["old"], ["latest"]]);
  });
});
