<script setup lang="ts">
import { computed } from 'vue'
import type { RequestGroup } from '@/lib/groups'
import {
  ContextMenuCheckboxItem,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
} from '@/components/ui/context-menu'
import {
  DropdownMenuCheckboxItem,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
} from '@/components/ui/dropdown-menu'

// A "Move to" menu that follows the group tree. A group with children opens
// a submenu; its first item moves into the group itself.
defineOptions({ name: 'GroupMenuTree' })
const props = withDefaults(
  defineProps<{
    groups: RequestGroup[]
    rootLabel: string
    /** The current location. Undefined: none. Null: the root. */
    currentId?: number | null
    /** A group to hide with its subtree, so a group cannot move into itself. */
    excludeId?: number
    kind?: 'context' | 'dropdown'
    /** Internal: the parent whose children this level lists. */
    parentId?: number | null
  }>(),
  {
    currentId: undefined,
    excludeId: undefined,
    kind: 'context',
    parentId: undefined,
  },
)
const emit = defineEmits<{ pick: [groupId: number | null] }>()

const ui = computed(() =>
  props.kind === 'dropdown'
    ? {
        Item: DropdownMenuItem,
        Check: DropdownMenuCheckboxItem,
        Separator: DropdownMenuSeparator,
        Sub: DropdownMenuSub,
        SubTrigger: DropdownMenuSubTrigger,
        SubContent: DropdownMenuSubContent,
      }
    : {
        Item: ContextMenuItem,
        Check: ContextMenuCheckboxItem,
        Separator: ContextMenuSeparator,
        Sub: ContextMenuSub,
        SubTrigger: ContextMenuSubTrigger,
        SubContent: ContextMenuSubContent,
      },
)
const nested = computed(() => props.parentId !== undefined)
function children(parentId: number | null) {
  return props.groups.filter((group) => group.parentId === parentId && group.id !== props.excludeId)
}
// The current location is a checked, disabled item.
function itemFor(id: number | null) {
  return props.currentId === id ? ui.value.Check : ui.value.Item
}
function itemProps(id: number | null) {
  return props.currentId === id ? { modelValue: true, disabled: true } : {}
}
</script>

<template>
  <template v-if="!nested">
    <component
      :is="itemFor(null)"
      v-bind="itemProps(null)"
      data-move-target="root"
      @select="emit('pick', null)">
      {{ rootLabel }}
    </component>
    <component :is="ui.Separator" v-if="children(null).length" />
  </template>
  <template v-for="group in children(parentId ?? null)" :key="group.id">
    <component :is="ui.Sub" v-if="children(group.id).length">
      <component :is="ui.SubTrigger" :data-move-target="group.id">
        {{ group.name }}
      </component>
      <component :is="ui.SubContent">
        <component
          :is="itemFor(group.id)"
          v-bind="itemProps(group.id)"
          :data-move-into="group.id"
          @select="emit('pick', group.id)">
          Move into {{ group.name }}
        </component>
        <component :is="ui.Separator" />
        <GroupMenuTree
          :groups="groups"
          :root-label="rootLabel"
          :current-id="currentId"
          :exclude-id="excludeId"
          :kind="kind"
          :parent-id="group.id"
          @pick="emit('pick', $event)" />
      </component>
    </component>
    <component
      :is="itemFor(group.id)"
      v-else
      v-bind="itemProps(group.id)"
      :data-move-target="group.id"
      @select="emit('pick', group.id)">
      {{ group.name }}
    </component>
  </template>
</template>
