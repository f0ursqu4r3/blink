<script setup lang="ts">
import { ContextMenuSeparator } from "reka-ui";
</script>

<template>
  <ContextMenuSeparator class="blink-ctx-separator" />
</template>

<style scoped>
.blink-ctx-separator {
  height: 1px;
  background: var(--border);
  margin: 0.25rem 0;
}
</style>
