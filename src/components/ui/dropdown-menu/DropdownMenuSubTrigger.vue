<script setup lang="ts">
import type { DropdownMenuSubTriggerProps } from 'reka-ui'
import type { HTMLAttributes } from 'vue'
import { ChevronRight } from 'lucide-vue-next'
import { reactiveOmit } from '@vueuse/core'
import { DropdownMenuSubTrigger, useForwardProps } from 'reka-ui'
import { cn } from '@/lib/utils'
import { menuSubTrigger } from '../menu-classes'

const props = defineProps<DropdownMenuSubTriggerProps & { class?: HTMLAttributes['class'] }>()

const delegatedProps = reactiveOmit(props, 'class')
const forwardedProps = useForwardProps(delegatedProps)
</script>

<template>
  <DropdownMenuSubTrigger
    data-slot="dropdown-menu-sub-trigger"
    v-bind="forwardedProps"
    :class="cn(menuSubTrigger, props.class)">
    <slot />
    <ChevronRight class="ml-auto size-3" />
  </DropdownMenuSubTrigger>
</template>
