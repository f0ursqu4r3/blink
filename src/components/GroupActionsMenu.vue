<script setup lang="ts">
import { Ellipsis } from "lucide-vue-next";
import {
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuPortal,
  DropdownMenuRoot,
  DropdownMenuTrigger,
} from "reka-ui";

defineProps<{ name: string; canMove: boolean }>();
const emit = defineEmits<{
  action: [action: "createGroup" | "move" | "rename" | "delete"];
}>();
</script>

<template>
  <DropdownMenuRoot>
    <DropdownMenuTrigger
      class="inline-flex size-5.5 shrink-0 items-center justify-center text-muted-foreground hover:bg-accent hover:text-foreground pointer-coarse:size-8"
      :aria-label="`More actions for ${name}`"
    >
      <Ellipsis :size="14" aria-hidden="true" />
    </DropdownMenuTrigger>
    <DropdownMenuPortal>
      <DropdownMenuContent
        align="end"
        :side-offset="4"
        class="z-50 min-w-40 rounded border border-border bg-secondary p-1 font-mono text-xs text-foreground shadow-sm"
        data-surface="context-menu"
      >
        <DropdownMenuItem
          class="menu-item"
          :aria-label="`Add group inside ${name}`"
          @select="emit('action', 'createGroup')"
          >New child group</DropdownMenuItem
        >
        <DropdownMenuItem
          class="menu-item"
          :disabled="!canMove"
          @select="emit('action', 'move')"
          >Move selection here</DropdownMenuItem
        >
        <DropdownMenuItem class="menu-item" @select="emit('action', 'rename')"
          >Rename</DropdownMenuItem
        >
        <DropdownMenuItem
          class="menu-item text-destructive"
          @select="emit('action', 'delete')"
          >Delete group</DropdownMenuItem
        >
      </DropdownMenuContent>
    </DropdownMenuPortal>
  </DropdownMenuRoot>
</template>

<style scoped>
@reference "../style.css";
.menu-item {
  @apply cursor-default select-none rounded-sm px-2 py-1.5 outline-none data-highlighted:bg-accent data-disabled:pointer-events-none data-disabled:opacity-40 pointer-coarse:py-3;
}
</style>
