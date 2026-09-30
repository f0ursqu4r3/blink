<script setup lang="ts">
import type { DropdownMenuSubContentEmits, DropdownMenuSubContentProps } from 'reka-ui'
import type { HTMLAttributes } from 'vue'
import { reactiveOmit } from '@vueuse/core'
import { ContextMenuSubContent, useForwardPropsEmits } from 'reka-ui'
import { cn } from '@/lib/utils'
import { menuContent } from '../menu-classes'

const props = defineProps<DropdownMenuSubContentProps & { class?: HTMLAttributes['class'] }>()
const emits = defineEmits<DropdownMenuSubContentEmits>()

const delegatedProps = reactiveOmit(props, 'class')

const forwarded = useForwardPropsEmits(delegatedProps, emits)
</script>

<template>
  <ContextMenuSubContent
    data-slot="context-menu-sub-content"
    data-surface="context-menu"
    v-bind="forwarded"
    :class="cn(menuContent, props.class)">
    <slot />
  </ContextMenuSubContent>
</template>
