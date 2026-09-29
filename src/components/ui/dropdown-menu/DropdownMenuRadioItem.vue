<script setup lang="ts">
import type {
  DropdownMenuRadioItemEmits,
  DropdownMenuRadioItemProps,
} from "reka-ui";
import type { HTMLAttributes } from "vue";
import { Check } from "lucide-vue-next";
import { reactiveOmit } from "@vueuse/core";
import {
  DropdownMenuItemIndicator,
  DropdownMenuRadioItem,
  useForwardPropsEmits,
} from "reka-ui";
import { cn } from "@/lib/utils";
import { menuItem, menuIndicator } from "../menu-classes";

const props = defineProps<
  DropdownMenuRadioItemProps & { class?: HTMLAttributes["class"] }
>();

const emits = defineEmits<DropdownMenuRadioItemEmits>();

const delegatedProps = reactiveOmit(props, "class");

const forwarded = useForwardPropsEmits(delegatedProps, emits);
</script>

<template>
  <DropdownMenuRadioItem
    data-slot="dropdown-menu-radio-item"
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
  </DropdownMenuRadioItem>
</template>
