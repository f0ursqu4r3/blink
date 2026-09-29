<script setup lang="ts">
import type { ContextMenuLabelProps } from "reka-ui";
import type { HTMLAttributes } from "vue";
import { reactiveOmit } from "@vueuse/core";
import { ContextMenuLabel } from "reka-ui";
import { cn } from "@/lib/utils";
import { menuLabel } from "../menu-classes";

const props = defineProps<
  ContextMenuLabelProps & { class?: HTMLAttributes["class"] }
>();

const delegatedProps = reactiveOmit(props, "class");
</script>

<template>
  <ContextMenuLabel
    data-slot="context-menu-label"
    v-bind="delegatedProps"
    :class="cn(menuLabel, props.class)"
  >
    <slot />
  </ContextMenuLabel>
</template>
