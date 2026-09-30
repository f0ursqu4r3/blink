<script setup lang="ts">
import type { ContextMenuSubTriggerProps } from 'reka-ui'
import type { HTMLAttributes } from 'vue'
import { ChevronRight } from 'lucide-vue-next'
import { reactiveOmit } from '@vueuse/core'
import { ContextMenuSubTrigger, useForwardProps } from 'reka-ui'
import { cn } from '@/lib/utils'
import { menuSubTrigger } from '../menu-classes'

const props = defineProps<ContextMenuSubTriggerProps & { class?: HTMLAttributes['class'] }>()

const delegatedProps = reactiveOmit(props, 'class')

const forwardedProps = useForwardProps(delegatedProps)
</script>

<template>
  <ContextMenuSubTrigger
    data-slot="context-menu-sub-trigger"
    v-bind="forwardedProps"
    :class="cn(menuSubTrigger, props.class)">
    <slot />
    <ChevronRight class="ml-auto size-3" />
  </ContextMenuSubTrigger>
</template>
