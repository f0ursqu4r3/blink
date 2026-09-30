<script setup lang="ts">
import type { ContextMenuRadioItemEmits, ContextMenuRadioItemProps } from 'reka-ui'
import type { HTMLAttributes } from 'vue'
import { Check } from 'lucide-vue-next'
import { reactiveOmit } from '@vueuse/core'
import { ContextMenuItemIndicator, ContextMenuRadioItem, useForwardPropsEmits } from 'reka-ui'
import { cn } from '@/lib/utils'
import { menuItem, menuIndicator } from '../menu-classes'

const props = defineProps<ContextMenuRadioItemProps & { class?: HTMLAttributes['class'] }>()
const emits = defineEmits<ContextMenuRadioItemEmits>()

const delegatedProps = reactiveOmit(props, 'class')

const forwarded = useForwardPropsEmits(delegatedProps, emits)
</script>

<template>
  <ContextMenuRadioItem
    data-slot="context-menu-radio-item"
    v-bind="forwarded"
    :class="cn(menuItem, props.class)">
    <span :class="menuIndicator">
      <ContextMenuItemIndicator>
        <slot name="indicator-icon">
          <Check class="size-3" />
        </slot>
      </ContextMenuItemIndicator>
    </span>
    <slot />
  </ContextMenuRadioItem>
</template>
