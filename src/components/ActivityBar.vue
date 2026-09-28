<script setup lang="ts">
import { Files, Settings } from "lucide-vue-next";

defineProps<{ browserOpen: boolean }>();
const emit = defineEmits<{
  toggleBrowser: [];
  openSettings: [event: MouseEvent];
}>();
</script>

<template>
  <nav
    class="flex w-11 shrink-0 flex-col items-center gap-1 py-1"
    aria-label="Activity bar"
    data-activity-bar
  >
    <button
      type="button"
      class="activity-button"
      data-activity="browser"
      aria-label="Request browser"
      title="Request browser"
      aria-controls="request-browser"
      :aria-pressed="browserOpen"
      @click="emit('toggleBrowser')"
    >
      <Files :size="18" aria-hidden="true" />
    </button>
    <button
      type="button"
      class="activity-button mt-auto"
      data-activity="settings"
      aria-label="Application settings"
      title="Application settings · Cmd/Ctrl+,"
      @click="emit('openSettings', $event)"
    >
      <Settings :size="18" aria-hidden="true" />
    </button>
  </nav>
</template>

<style scoped>
@reference "../style.css";
.activity-button {
  @apply relative flex size-9 items-center justify-center rounded text-muted-foreground hover:bg-accent hover:text-foreground;
}
.activity-button[aria-pressed="true"] {
  @apply text-foreground;
}
/* Active marker on the outer edge, as in VS Code. */
.activity-button[aria-pressed="true"]::before {
  position: absolute;
  inset: 8px auto 8px -4px;
  width: 2px;
  content: "";
  background: var(--primary);
}
</style>
