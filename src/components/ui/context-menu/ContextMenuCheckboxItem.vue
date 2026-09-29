<script setup lang="ts">
import type {
  ContextMenuCheckboxItemEmits,
  ContextMenuCheckboxItemProps,
} from "reka-ui";
import type { HTMLAttributes } from "vue";
import { Check } from "lucide-vue-next";
import { reactiveOmit } from "@vueuse/core";
import {
  ContextMenuCheckboxItem,
  ContextMenuItemIndicator,
  useForwardPropsEmits,
} from "reka-ui";
import { cn } from "@/lib/utils";
import { menuItem, menuIndicator } from "../menu-classes";

const props = defineProps<
  ContextMenuCheckboxItemProps & { class?: HTMLAttributes["class"] }
>();
const emits = defineEmits<ContextMenuCheckboxItemEmits>();

const delegatedProps = reactiveOmit(props, "class");

const forwarded = useForwardPropsEmits(delegatedProps, emits);
</script>

<template>
  <ContextMenuCheckboxItem
    data-slot="context-menu-checkbox-item"
    v-bind="forwarded"
    :class="cn(menuItem, props.class)"
  >
    <span :class="menuIndicator">
      <ContextMenuItemIndicator>
        <slot name="indicator-icon">
          <Check class="size-3" />
        </slot>
      </ContextMenuItemIndicator>
    </span>
    <slot />
  </ContextMenuCheckboxItem>
</template>
