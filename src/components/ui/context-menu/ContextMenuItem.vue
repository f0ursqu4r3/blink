<script setup lang="ts">
import { ContextMenuItem } from 'reka-ui';

defineProps<{
  class?: string;
  disabled?: boolean;
}>();

const emit = defineEmits<{
  select: [e: Event];
}>();
</script>

<template>
  <ContextMenuItem
    :class="[
      'flex items-center gap-2 rounded-sm px-2 py-1 cursor-default outline-none select-none text-foreground font-mono text-xs',
      'data-highlighted:bg-accent data-highlighted:text-foreground',
      'focus-visible:outline-2 focus-visible:outline-ring focus-visible:-outline-offset-2',
      'data-disabled:opacity-40 data-disabled:pointer-events-none',
      $props.class,
    ]"
    :disabled="disabled"
    @select="(e: Event) => emit('select', e)"
  >
    <slot />
  </ContextMenuItem>
</template>
