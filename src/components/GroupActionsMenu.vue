<script setup lang="ts">
import { Ellipsis } from 'lucide-vue-next'
import type { RequestGroup } from '@/lib/groups'
import GroupMenuItems, { type GroupAction } from './GroupMenuItems.vue'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'

defineProps<{
  group: RequestGroup
  groups: RequestGroup[]
  canMoveSelection: boolean
  focused?: boolean
}>()
const emit = defineEmits<{
  action: [action: GroupAction]
  moveTo: [parentId: number | null]
}>()
</script>

<template>
  <DropdownMenu>
    <DropdownMenuTrigger
      class="inline-flex size-5.5 shrink-0 items-center justify-center text-muted-foreground hover:bg-accent hover:text-foreground pointer-coarse:size-8"
      :aria-label="`More actions for ${group.name}`">
      <Ellipsis :size="14" aria-hidden="true" />
    </DropdownMenuTrigger>
    <DropdownMenuContent align="end" :side-offset="4">
      <GroupMenuItems
        kind="dropdown"
        :group="group"
        :groups="groups"
        :can-move-selection="canMoveSelection"
        :focused="focused"
        @action="emit('action', $event)"
        @move-to="emit('moveTo', $event)" />
    </DropdownMenuContent>
  </DropdownMenu>
</template>
