<script setup lang="ts">
import { Ellipsis } from "lucide-vue-next";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

defineProps<{ name: string; canMove: boolean }>();
const emit = defineEmits<{
  action: [action: "createGroup" | "move" | "rename" | "delete"];
}>();
</script>

<template>
  <DropdownMenu>
    <DropdownMenuTrigger
      class="inline-flex size-5.5 shrink-0 items-center justify-center text-muted-foreground hover:bg-accent hover:text-foreground pointer-coarse:size-8"
      :aria-label="`More actions for ${name}`"
    >
      <Ellipsis :size="14" aria-hidden="true" />
    </DropdownMenuTrigger>
    <DropdownMenuContent align="end" :side-offset="4">
      <DropdownMenuItem
        :aria-label="`Add group inside ${name}`"
        @select="emit('action', 'createGroup')"
        >New child group</DropdownMenuItem
      >
      <DropdownMenuItem :disabled="!canMove" @select="emit('action', 'move')"
        >Move selection here</DropdownMenuItem
      >
      <DropdownMenuItem @select="emit('action', 'rename')"
        >Rename</DropdownMenuItem
      >
      <DropdownMenuItem variant="destructive" @select="emit('action', 'delete')"
        >Delete group</DropdownMenuItem
      >
    </DropdownMenuContent>
  </DropdownMenu>
</template>
