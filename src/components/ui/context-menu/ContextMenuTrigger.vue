<script setup lang="ts">
import { ContextMenuTrigger } from "reka-ui";

defineProps<{
  asChild?: boolean;
}>();
</script>

<template>
  <ContextMenuTrigger :as-child="asChild ?? false">
    <slot />
  </ContextMenuTrigger>
</template>
