<script setup lang="ts">
import { ContextMenuItem } from "reka-ui";

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
    :class="['blink-ctx-item', $props.class]"
    :disabled="disabled"
    @select="(e: Event) => emit('select', e)"
  >
    <slot />
  </ContextMenuItem>
</template>

<style scoped>
.blink-ctx-item {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  border-radius: 2px;
  padding: 0.25rem 0.5rem;
  cursor: default;
  outline: none;
  user-select: none;
  color: var(--foreground);
  font-family: var(--font-mono, monospace);
  font-size: 0.75rem;
}

.blink-ctx-item[data-highlighted] {
  background: var(--accent);
  color: var(--foreground);
}

.blink-ctx-item:focus-visible {
  outline: 2px solid var(--ring);
  outline-offset: -2px;
}

.blink-ctx-item[data-disabled] {
  opacity: 0.4;
  pointer-events: none;
  cursor: default;
}
</style>
