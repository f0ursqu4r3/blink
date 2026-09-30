import { isTauri } from "@tauri-apps/api/core";

/** Scale the whole interface. The desktop app zooms the webview. */
export async function applyZoom(zoom: number) {
  if (isTauri()) {
    const { getCurrentWebview } = await import("@tauri-apps/api/webview");
    await getCurrentWebview().setZoom(zoom);
  } else document.documentElement.style.zoom = zoom === 1 ? "" : String(zoom);
}
