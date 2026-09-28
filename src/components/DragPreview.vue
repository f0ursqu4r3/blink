<script setup lang="ts">
import { Folder } from "lucide-vue-next";
import { useDragDrop } from "@/composables/useDragDrop";

const { state } = useDragDrop();
</script>

<template>
  <Teleport to="body">
    <Transition
      leave-active-class="transition-opacity duration-100"
      leave-to-class="opacity-0"
    >
      <div
        v-if="state.preview"
        class="pointer-events-none fixed left-0 top-0 z-50 flex max-w-60 items-center gap-1.5 rounded-md border border-border bg-secondary px-2 py-1 font-mono text-[10px] text-foreground shadow-md"
        :style="{
          transform: `translate(${state.point.x + 8}px, ${state.point.y + 8}px)`,
        }"
        data-drag-preview
      >
        <Folder
          v-if="state.preview.folder"
          :size="12"
          class="shrink-0"
          aria-hidden="true"
        />
        <span
          v-else-if="state.preview.method"
          class="method shrink-0 text-[8px] font-bold"
          :data-method="state.preview.method"
          >{{ state.preview.method }}</span
        >
        <span class="min-w-0 truncate">{{ state.preview.label }}</span>
      </div>
    </Transition>
  </Teleport>
</template>
