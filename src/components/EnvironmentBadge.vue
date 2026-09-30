<script setup lang="ts">
import { computed } from "vue";
import { Check, Settings } from "lucide-vue-next";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuLabel,
} from "@/components/ui/dropdown-menu";
import type { RequestGroup } from "@/lib/groups";
import { activeEnvironment } from "@/lib/environments";

const props = defineProps<{ group: RequestGroup }>();
const emit = defineEmits<{
  switch: [environmentId: number | null];
  edit: [];
}>();
const active = computed(() => activeEnvironment(props.group));
</script>

<template>
  <DropdownMenu>
    <DropdownMenuTrigger as-child>
      <button
        type="button"
        data-no-drag
        data-environment-badge
        class="ml-1 shrink-0 rounded-sm px-1 font-mono text-[9px] font-semibold tracking-[0.06em] hover:bg-accent"
        :class="{ 'text-muted-foreground': !active }"
        :style="active ? { color: `var(--${active.color})` } : undefined"
        :aria-label="`Environment for ${group.name}: ${active?.name ?? 'none'}. Switch environment`"
        :title="`Environment: ${active?.name ?? 'none'}`"
        @click.stop
      >
        {{ active?.name ?? "NO ENV" }}
      </button>
    </DropdownMenuTrigger>
    <DropdownMenuContent align="start" class="w-48">
      <DropdownMenuLabel>{{ group.name }} environment</DropdownMenuLabel>
      <DropdownMenuItem
        v-for="environment in group.environments"
        :key="environment.id"
        :data-environment-option="environment.id"
        @select="emit('switch', environment.id)"
      >
        <span
          class="size-2 shrink-0 rounded-full"
          :style="{ background: `var(--${environment.color})` }"
        />
        <span class="flex-1 font-mono">{{ environment.name }}</span>
        <span
          v-if="environment.protected"
          class="text-[10px] text-muted-foreground"
          >protected</span
        >
        <Check
          v-if="environment.id === active?.id"
          :size="13"
          aria-hidden="true"
        />
      </DropdownMenuItem>
      <DropdownMenuItem
        data-environment-option="none"
        @select="emit('switch', null)"
      >
        <span class="size-2 shrink-0" />
        <span class="flex-1">No environment</span>
        <Check v-if="!active" :size="13" aria-hidden="true" />
      </DropdownMenuItem>
      <DropdownMenuSeparator />
      <DropdownMenuItem @select="emit('edit')">
        <Settings :size="13" aria-hidden="true" />
        Edit environments…
      </DropdownMenuItem>
    </DropdownMenuContent>
  </DropdownMenu>
</template>
