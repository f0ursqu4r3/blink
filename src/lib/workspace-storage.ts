import { invoke } from "@tauri-apps/api/core";
import { nativeTransport } from "./transport";
import { WORKSPACE_KEY, validateWorkspace } from "./workspace";

export function readBrowserWorkspace() {
  return localStorage.getItem(WORKSPACE_KEY);
}
export async function readWorkspace() {
  return nativeTransport
    ? invoke<string | null>("load_app_state")
    : readBrowserWorkspace();
}
export async function writeWorkspace(content: string) {
  validateWorkspace(content);
  if (nativeTransport) await invoke("save_app_state", { content });
  else localStorage.setItem(WORKSPACE_KEY, content);
}

/** One disk write at a time. Keep the newest queued snapshot, never an older one. */
export class WorkspaceWriter {
  private pending: string | null = null;
  private running: Promise<void> = Promise.resolve();
  constructor(private write: (content: string) => Promise<void>) {}
  save(content: string): Promise<void> {
    this.pending = content;
    this.running = this.running.catch(() => {}).then(() => this.drain());
    return this.running;
  }
  private async drain() {
    while (this.pending !== null) {
      const next = this.pending;
      this.pending = null;
      try {
        await this.write(next);
      } catch (error) {
        this.pending ??= next;
        throw error;
      }
    }
  }
}
