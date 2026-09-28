<script setup lang="ts">
import { ContextMenuPortal, ContextMenuContent } from "reka-ui";

defineProps<{
  class?: string;
}>();
</script>

<template>
  <ContextMenuPortal>
    <ContextMenuContent
      :class="[
        'ctx-anim',
        'min-w-40 overflow-hidden rounded-lg border border-border bg-secondary text-foreground p-1 z-50 font-mono text-xs',
        'shadow-[0_4px_6px_-1px_oklch(0_0_0/0.4),0_2px_4px_-2px_oklch(0_0_0/0.3)]',
        $props.class,
      ]"
      data-surface="context-menu"
    >
      <slot />
    </ContextMenuContent>
  </ContextMenuPortal>
</template>

<style scoped>
/* Enter/exit keyframe animations cannot be expressed as Tailwind utilities */
@media (prefers-reduced-motion: no-preference) {
  .ctx-anim[data-state="open"] {
    animation: ctx-in 120ms ease-out;
  }
  .ctx-anim[data-state="closed"] {
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
