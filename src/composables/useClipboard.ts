import { onScopeDispose, ref } from "vue";

export function useClipboard() {
  const copied = ref(false);
  const copyError = ref("");
  let timer: ReturnType<typeof setTimeout> | undefined;
  let alive = true;
  async function copy(text: string) {
    copyError.value = "";
    try {
      if (!navigator.clipboard) throw new Error("Clipboard unavailable");
      await navigator.clipboard.writeText(text);
      if (!alive) return;
      copied.value = true;
      clearTimeout(timer);
      timer = setTimeout(() => {
        copied.value = false;
      }, 1600);
    } catch {
      if (alive)
        copyError.value =
          "Clipboard unavailable. Select and copy the text manually.";
    }
  }
  onScopeDispose(() => {
    alive = false;
    clearTimeout(timer);
  });
  return { copied, copyError, copy };
}
