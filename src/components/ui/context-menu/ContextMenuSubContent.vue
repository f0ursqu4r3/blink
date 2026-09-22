<script setup lang="ts">
import { ContextMenuSubContent } from "reka-ui";

defineProps<{
  class?: string;
}>();
</script>

<template>
  <ContextMenuSubContent
    :class="[
      'min-w-40 overflow-hidden rounded border border-border bg-secondary text-foreground p-1 z-50 font-mono text-xs',
      'shadow-[0_4px_6px_-1px_oklch(0_0_0/0.4),0_2px_4px_-2px_oklch(0_0_0/0.3)]',
      $props.class,
    ]"
    data-surface="context-menu"
  >
    <slot />
  </ContextMenuSubContent>
</template>
