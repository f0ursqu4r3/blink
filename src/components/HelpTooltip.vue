<script setup lang="ts">
import { ref } from "vue";
import {
  TooltipContent,
  TooltipPortal,
  TooltipProvider,
  TooltipRoot,
  TooltipTrigger,
} from "reka-ui";

const props = withDefaults(
  defineProps<{ text: string; helpOnly?: boolean }>(),
  { helpOnly: true },
);
const open = ref(false);

function openFromClick(event: MouseEvent) {
  if (!props.helpOnly) return;
  // Reka closes a tooltip on trigger click. Re-open after its click handler
  // runs so mouse and touch activation remain dependable.
  event.preventDefault();
  event.stopPropagation();
  const target = event.currentTarget as HTMLElement | null;
  target?.focus();
  queueMicrotask(() => {
    open.value = true;
  });
}
</script>

<template>
  <TooltipProvider :delay-duration="200">
    <TooltipRoot v-model:open="open">
      <TooltipTrigger
        as-child
        @pointerenter="open = true"
        @focusin="open = true"
        @click="openFromClick"
        ><slot
      /></TooltipTrigger>
      <TooltipPortal>
        <TooltipContent
          class="z-[70] max-w-72 rounded border border-border bg-secondary px-2 py-1.5 font-mono text-[11px] text-foreground shadow-sm"
          :side-offset="6"
          @escape-key-down.prevent="open = false"
        >
          {{ text }}
        </TooltipContent>
      </TooltipPortal>
    </TooltipRoot>
  </TooltipProvider>
</template>
