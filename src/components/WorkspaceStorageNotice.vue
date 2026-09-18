<script setup lang="ts">
import { ref } from "vue";
import { Button } from "@/components/ui/button";
defineProps<{ error: string; ready: boolean; exitBlocked: boolean }>();
const emit = defineEmits<{ retry: []; reset: []; quit: [] }>();
const confirmReset = ref(false);
</script>

<template>
  <div v-if="error" class="storage-notice" role="alert">
    <p>{{ error }} <span v-if="ready">Changes are not saved.</span></p>
    <Button variant="secondary" @click="emit('retry')">Retry</Button>
    <template v-if="!ready">
      <Button v-if="!confirmReset" variant="ghost" @click="confirmReset = true"
        >Start fresh</Button
      >
      <template v-else>
        <span>Replace the existing saved workspace?</span>
        <Button
          variant="secondary"
          @click="
            emit('reset');
            confirmReset = false;
          "
          >Replace saved workspace</Button
        >
        <Button variant="ghost" @click="confirmReset = false">Cancel</Button>
      </template>
    </template>
    <Button v-if="exitBlocked" variant="ghost" @click="emit('quit')"
      >Quit without saving</Button
    >
  </div>
</template>

<style scoped>
.storage-notice {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  padding: 8px 16px;
  border-bottom: 1px solid var(--border);
  color: var(--destructive);
  font-size: 12px;
}
p {
  flex: 1;
  margin: 0;
  min-width: 180px;
}
</style>
