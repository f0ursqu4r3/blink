<script lang="ts">
export type GroupAction =
  | 'createRequest'
  | 'createGroup'
  | 'rename'
  | 'settings'
  | 'toggle'
  | 'collapseAll'
  | 'focus'
  | 'unfocus'
  | 'moveSelection'
  | 'delete'
</script>

<script setup lang="ts">
import { computed } from 'vue'
import type { RequestGroup } from '@/lib/groups'
import GroupMenuTree from './GroupMenuTree.vue'
import {
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
} from '@/components/ui/context-menu'
import {
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
} from '@/components/ui/dropdown-menu'

// The group menu, shared by the row's context menu and its ⋯ dropdown.
const props = withDefaults(
  defineProps<{
    group: RequestGroup
    groups: RequestGroup[]
    canMoveSelection: boolean
    /** The Browser shows only this group. */
    focused?: boolean
    kind?: 'context' | 'dropdown'
  }>(),
  { kind: 'context', focused: false },
)
const emit = defineEmits<{
  action: [action: GroupAction]
  moveTo: [parentId: number | null]
}>()
const ui = computed(() =>
  props.kind === 'dropdown'
    ? {
        Item: DropdownMenuItem,
        Separator: DropdownMenuSeparator,
        Sub: DropdownMenuSub,
        SubTrigger: DropdownMenuSubTrigger,
        SubContent: DropdownMenuSubContent,
      }
    : {
        Item: ContextMenuItem,
        Separator: ContextMenuSeparator,
        Sub: ContextMenuSub,
        SubTrigger: ContextMenuSubTrigger,
        SubContent: ContextMenuSubContent,
      },
)
</script>

<template>
  <component :is="ui.Item" @select="emit('action', 'createRequest')">New request</component>
  <component
    :is="ui.Item"
    :aria-label="`Add group inside ${group.name}`"
    @select="emit('action', 'createGroup')">
    New group
  </component>
  <component :is="ui.Separator" />
  <component :is="ui.Item" @select="emit('action', 'rename')">Rename</component>
  <component :is="ui.Item" @select="emit('action', 'settings')">Settings…</component>
  <component :is="ui.Separator" />
  <component :is="ui.Item" data-group-focus @select="emit('action', focused ? 'unfocus' : 'focus')">
    {{ focused ? 'Unfocus' : 'Focus' }}
  </component>
  <component :is="ui.Item" @select="emit('action', 'toggle')">
    {{ group.collapsed ? 'Expand' : 'Collapse' }}
  </component>
  <component :is="ui.Item" @select="emit('action', 'collapseAll')">Collapse all</component>
  <component :is="ui.Separator" />
  <component :is="ui.Item" v-if="canMoveSelection" @select="emit('action', 'moveSelection')">
    Move selection here
  </component>
  <component :is="ui.Sub">
    <component :is="ui.SubTrigger" data-group-move-menu>Move to</component>
    <component :is="ui.SubContent">
      <GroupMenuTree
        :groups="groups"
        root-label="Top level"
        :current-id="group.parentId"
        :exclude-id="group.id"
        :kind="kind"
        @pick="emit('moveTo', $event)" />
    </component>
  </component>
  <component :is="ui.Separator" />
  <component
    :is="ui.Item"
    data-group-delete
    variant="destructive"
    @select="emit('action', 'delete')">
    Delete group
  </component>
</template>
