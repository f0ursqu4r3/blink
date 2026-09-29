<script setup lang="ts">
import type {
  DropdownMenuCheckboxItemEmits,
  DropdownMenuCheckboxItemProps,
} from "reka-ui";
import type { HTMLAttributes } from "vue";
import { Check } from "lucide-vue-next";
import { reactiveOmit } from "@vueuse/core";
import {
  DropdownMenuCheckboxItem,
  DropdownMenuItemIndicator,
  useForwardPropsEmits,
} from "reka-ui";
import { cn } from "@/lib/utils";
import { menuItem, menuIndicator } from "../menu-classes";

const props = defineProps<
  DropdownMenuCheckboxItemProps & { class?: HTMLAttributes["class"] }
>();
const emits = defineEmits<DropdownMenuCheckboxItemEmits>();

const delegatedProps = reactiveOmit(props, "class");

const forwarded = useForwardPropsEmits(delegatedProps, emits);
</script>

<template>
  <DropdownMenuCheckboxItem
    data-slot="dropdown-menu-checkbox-item"
    v-bind="forwarded"
    :class="cn(menuItem, props.class)"
  >
    <span :class="menuIndicator">
      <DropdownMenuItemIndicator>
        <slot name="indicator-icon">
          <Check class="size-3" />
        </slot>
      </DropdownMenuItemIndicator>
    </span>
    <slot />
  </DropdownMenuCheckboxItem>
</template>
