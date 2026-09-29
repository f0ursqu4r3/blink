<script setup lang="ts">
import type { DropdownMenuItemProps } from "reka-ui";
import type { HTMLAttributes } from "vue";
import { reactiveOmit } from "@vueuse/core";
import { DropdownMenuItem, useForwardProps } from "reka-ui";
import { cn } from "@/lib/utils";
import { menuItem } from "../menu-classes";

const props = withDefaults(
  defineProps<
    DropdownMenuItemProps & {
      class?: HTMLAttributes["class"];
      variant?: "default" | "destructive";
    }
  >(),
  {
    variant: "default",
  },
);

const delegatedProps = reactiveOmit(props, "variant", "class");

const forwardedProps = useForwardProps(delegatedProps);
</script>

<template>
  <DropdownMenuItem
    data-slot="dropdown-menu-item"
    :data-variant="variant"
    v-bind="forwardedProps"
    :class="cn(menuItem, props.class)"
  >
    <slot />
  </DropdownMenuItem>
</template>
