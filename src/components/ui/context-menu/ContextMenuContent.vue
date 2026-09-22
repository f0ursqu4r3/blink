<script setup lang="ts">
import { ContextMenuPortal, ContextMenuContent } from "reka-ui";

defineProps<{
  class?: string;
}>();
</script>

<template>
  <ContextMenuPortal>
    <ContextMenuContent :class="['blink-ctx-content', $props.class]">
      <slot />
    </ContextMenuContent>
  </ContextMenuPortal>
</template>

<style scoped>
.blink-ctx-content {
  min-width: 10rem;
  overflow: hidden;
  border-radius: 4px;
  border: 1px solid var(--border);
  background: var(--secondary);
  color: var(--foreground);
  padding: 0.25rem;
  box-shadow:
    0 4px 6px -1px oklch(0 0 0 / 0.4),
    0 2px 4px -2px oklch(0 0 0 / 0.3);
  font-family: var(--font-mono, monospace);
  font-size: 0.75rem;
  z-index: 50;
}

@media (prefers-reduced-motion: no-preference) {
  .blink-ctx-content[data-state="open"] {
    animation: ctx-in 120ms ease-out;
  }
  .blink-ctx-content[data-state="closed"] {
    animation: ctx-out 100ms ease-in;
  }
}

@keyframes ctx-in {
  from {
    opacity: 0;
    transform: scale(0.96);
  }
  to {
    opacity: 1;
    transform: scale(1);
  }
}

@keyframes ctx-out {
  from {
    opacity: 1;
    transform: scale(1);
  }
  to {
    opacity: 0;
    transform: scale(0.96);
  }
}
</style>
