<script setup lang="ts">
import type { HTMLAttributes } from 'vue'
import { cn } from '@/lib/utils'
import { menuShortcut } from '../menu-classes'

const props = defineProps<{
  class?: HTMLAttributes['class']
}>()
</script>

<template>
  <span data-slot="dropdown-menu-shortcut" :class="cn(menuShortcut, props.class)">
    <slot />
  </span>
</template>
