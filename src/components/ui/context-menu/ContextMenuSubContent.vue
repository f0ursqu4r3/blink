<script setup lang="ts">
import { ContextMenuSubContent } from "reka-ui";

defineProps<{
  class?: string;
}>();
</script>

<template>
  <ContextMenuSubContent
    :class="['blink-ctx-sub-content', $props.class]"
    data-surface="context-menu"
  >
    <slot />
  </ContextMenuSubContent>
</template>

<style scoped>
.blink-ctx-sub-content {
  min-width: 10rem;
  overflow: hidden;
  border: 1px solid var(--border);
  border-radius: 4px;
  color: var(--foreground);
  padding: 0.25rem;
  box-shadow:
    0 4px 6px -1px oklch(0 0 0 / 0.4),
    0 2px 4px -2px oklch(0 0 0 / 0.3);
  font-family: var(--font-mono, monospace);
  font-size: 0.75rem;
  z-index: 50;
}
</style>
