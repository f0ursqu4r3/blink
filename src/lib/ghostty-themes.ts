import { invoke } from "@tauri-apps/api/core";
import { nativeTransport } from "./transport";

/** Theme files live on disk, so only the desktop app can list them. */
export const canReadGhosttyThemes = nativeTransport;

export const listGhosttyThemes = () => invoke<string[]>("list_ghostty_themes");

export const readGhosttyTheme = (name: string) =>
  invoke<string>("read_ghostty_theme", { name });
