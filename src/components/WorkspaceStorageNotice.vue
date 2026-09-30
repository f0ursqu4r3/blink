<script setup lang="ts">
import { ref } from "vue";
import { Button } from "@/components/ui/button";
defineProps<{ error: string; ready: boolean; exitBlocked: boolean }>();
const emit = defineEmits<{ retry: []; reset: []; quit: [] }>();
const confirmReset = ref(false);
</script>

<template>
  <div
    v-if="error"
    class="flex flex-wrap items-center gap-2 px-4 py-2 border-b border-border text-destructive text-[12px]"
    role="alert"
  >
    <p class="flex-1 m-0 min-w-45">
      {{ error }} <span v-if="ready">Changes are not saved.</span>
    </p>
    <Button variant="secondary" @click="emit('retry')">Retry</Button>
    <template v-if="!ready">
      <Button v-if="!confirmReset" variant="ghost" @click="confirmReset = true">
        Start fresh
      </Button>
      <template v-else>
        <span>Replace the existing saved workspace?</span>
        <Button
          variant="secondary"
          @click="
            emit('reset');
            confirmReset = false;
          "
        >
          Replace saved workspace
        </Button>
        <Button variant="ghost" @click="confirmReset = false">Cancel</Button>
      </template>
    </template>
    <Button v-if="exitBlocked" variant="ghost" @click="emit('quit')">
      Quit without saving
    </Button>
  </div>
</template>
