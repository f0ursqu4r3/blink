<script setup lang="ts">
import type { TreeGuide } from '@/lib/tree-guides'

/** Faint tree lines drawn inside a relative row. */
defineProps<{
  guide: TreeGuide | null | undefined
  level: number
  /** x of the line for a depth. */
  x: (depth: number) => number
  /** x where the elbow's tick ends. */
  end: number
}>()
</script>

<template>
  <span
    v-if="guide"
    class="pointer-events-none absolute inset-y-0 left-0"
    aria-hidden="true"
    data-tree-guides>
    <span
      v-for="depth in guide.through"
      :key="depth"
      class="absolute inset-y-0 w-px bg-muted-foreground/40"
      :style="{ left: `${x(depth)}px` }" />
    <span
      class="absolute top-0 w-px bg-muted-foreground/40"
      :class="guide.elbow === 'mid' ? 'bottom-0' : 'h-1/2'"
      :style="{ left: `${x(level)}px` }"
      :data-elbow="guide.elbow" />
    <span
      class="absolute top-1/2 h-px bg-muted-foreground/40"
      :style="{
        left: `${x(level)}px`,
        width: `${Math.max(2, end - x(level))}px`,
      }" />
  </span>
</template>
