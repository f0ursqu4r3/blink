<script setup lang="ts">
import type { DropdownMenuLabelProps } from "reka-ui";
import type { HTMLAttributes } from "vue";
import { reactiveOmit } from "@vueuse/core";
import { DropdownMenuLabel, useForwardProps } from "reka-ui";
import { cn } from "@/lib/utils";
import { menuLabel } from "../menu-classes";

const props = defineProps<
  DropdownMenuLabelProps & { class?: HTMLAttributes["class"] }
>();

const delegatedProps = reactiveOmit(props, "class");
const forwardedProps = useForwardProps(delegatedProps);
</script>

<template>
  <DropdownMenuLabel
    data-slot="dropdown-menu-label"
    v-bind="forwardedProps"
    :class="cn(menuLabel, props.class)"
  >
    <slot />
  </DropdownMenuLabel>
</template>
