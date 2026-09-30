<script setup lang="ts">
import type { ContextMenuItemEmits, ContextMenuItemProps } from 'reka-ui'
import type { HTMLAttributes } from 'vue'
import { reactiveOmit } from '@vueuse/core'
import { ContextMenuItem, useForwardPropsEmits } from 'reka-ui'
import { cn } from '@/lib/utils'
import { menuItem } from '../menu-classes'

const props = withDefaults(
  defineProps<
    ContextMenuItemProps & {
      class?: HTMLAttributes['class']
      variant?: 'default' | 'destructive'
    }
  >(),
  {
    variant: 'default',
  },
)
const emits = defineEmits<ContextMenuItemEmits>()

const delegatedProps = reactiveOmit(props, 'variant', 'class')

const forwarded = useForwardPropsEmits(delegatedProps, emits)
</script>

<template>
  <ContextMenuItem
    data-slot="context-menu-item"
    :data-variant="variant"
    v-bind="forwarded"
    :class="cn(menuItem, props.class)">
    <slot />
  </ContextMenuItem>
</template>
