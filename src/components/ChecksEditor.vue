<script setup lang="ts">
import { Plus, X } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import HelpTooltip from './HelpTooltip.vue'
import {
  CAPTURE_NAME_RE,
  checkOperators,
  checkSources,
  createAssertion,
  createCapture,
  pathSources,
  unaryOperators,
  type Assertion,
  type Capture,
} from '@/lib/checks'

const assertions = defineModel<Assertion[]>('assertions', {
  default: () => [],
})
const captures = defineModel<Capture[]>('captures', { default: () => [] })
defineProps<{ disabled?: boolean }>()

const captureSources = checkSources.filter((source) => source.id !== 'time' && source.id !== 'size')
const pathPlaceholder = (source: string) => (source === 'header' ? 'Header name' : '.path.to.value')

function updateAssertion(id: number, patch: Partial<Assertion>) {
  assertions.value = assertions.value.map((row) => (row.id === id ? { ...row, ...patch } : row))
}
function updateCapture(id: number, patch: Partial<Capture>) {
  captures.value = captures.value.map((row) => (row.id === id ? { ...row, ...patch } : row))
}
const input =
  'h-8 min-w-0 bg-transparent border-0 px-2 font-mono text-xs rounded-none pointer-coarse:h-11 pointer-coarse:text-base'
const select =
  'h-8 min-w-0 bg-transparent border-0 px-1.5 text-xs rounded-none text-foreground pointer-coarse:h-11'
const cell = 'h-8.5 border-b border-l border-border'
const head =
  'text-muted-foreground text-left text-[0.625rem] uppercase tracking-widest font-medium h-8 px-2 bg-muted border-l border-border'
</script>

<template>
  <div data-checks-editor>
    <section aria-labelledby="assertions-heading">
      <h3
        id="assertions-heading"
        class="flex h-8 items-center justify-between px-3 font-mono text-[0.625rem] tracking-[0.12em] text-muted-foreground">
        ASSERTIONS
        <HelpTooltip
          text="Checked after each send. Results show in the response Tests tab. JSON sources take a jq expression.">
          <button
            type="button"
            class="font-sans text-[0.6875rem] tracking-normal underline decoration-dotted underline-offset-3">
            Help
          </button>
        </HelpTooltip>
      </h3>
      <table class="w-full table-fixed border-collapse" aria-label="Assertions">
        <thead>
          <tr>
            <th class="w-8.5 h-8 bg-muted">
              <span class="sr-only">Enabled</span>
            </th>
            <th scope="col" :class="head" class="w-[28%]">Source</th>
            <th scope="col" :class="head">Path</th>
            <th scope="col" :class="head" class="w-[22%]">Operator</th>
            <th scope="col" :class="head">Expected</th>
            <th class="w-8.5 h-8 bg-muted border-l border-border">
              <span class="sr-only">Remove</span>
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(row, index) in assertions" :key="row.id" data-assertion>
            <td class="h-8.5 border-b border-border text-center">
              <input
                type="checkbox"
                class="accent-primary w-3 h-3"
                :aria-label="`Enable assertion ${index + 1}`"
                :checked="row.enabled"
                :disabled="disabled"
                @change="updateAssertion(row.id, { enabled: !row.enabled })" />
            </td>
            <td :class="cell">
              <select
                :class="select"
                class="w-full"
                :aria-label="`Assertion ${index + 1} source`"
                :value="row.source"
                :disabled="disabled"
                data-assertion-source
                @change="
                  updateAssertion(row.id, {
                    source: ($event.target as HTMLSelectElement).value as Assertion['source'],
                  })
                ">
                <option v-for="source in checkSources" :key="source.id" :value="source.id">
                  {{ source.label }}
                </option>
              </select>
            </td>
            <td :class="cell">
              <input
                v-if="pathSources.includes(row.source)"
                :class="input"
                class="w-full"
                :aria-label="`Assertion ${index + 1} path`"
                :placeholder="pathPlaceholder(row.source)"
                :value="row.path"
                :disabled="disabled"
                spellcheck="false"
                autocomplete="off"
                data-assertion-path
                @input="
                  updateAssertion(row.id, {
                    path: ($event.target as HTMLInputElement).value,
                  })
                " />
            </td>
            <td :class="cell">
              <select
                :class="select"
                class="w-full"
                :aria-label="`Assertion ${index + 1} operator`"
                :value="row.operator"
                :disabled="disabled"
                data-assertion-operator
                @change="
                  updateAssertion(row.id, {
                    operator: ($event.target as HTMLSelectElement).value as Assertion['operator'],
                  })
                ">
                <option v-for="operator in checkOperators" :key="operator.id" :value="operator.id">
                  {{ operator.label }}
                </option>
              </select>
            </td>
            <td :class="cell">
              <input
                v-if="!unaryOperators.includes(row.operator)"
                :class="input"
                class="w-full"
                :aria-label="`Assertion ${index + 1} expected value`"
                placeholder="Value"
                :value="row.expected"
                :disabled="disabled"
                spellcheck="false"
                autocomplete="off"
                data-assertion-expected
                @input="
                  updateAssertion(row.id, {
                    expected: ($event.target as HTMLInputElement).value,
                  })
                " />
            </td>
            <td class="h-8.5 border-b border-l border-border p-0">
              <Button
                variant="ghost"
                class="size-7 shrink-0 p-0"
                :aria-label="`Remove assertion ${index + 1}`"
                :disabled="disabled"
                @click="assertions = assertions.filter((candidate) => candidate.id !== row.id)">
                <X :size="13" aria-hidden="true" />
              </Button>
            </td>
          </tr>
        </tbody>
      </table>
      <Button
        variant="ghost"
        class="m-1.5"
        data-add-assertion
        :disabled="disabled"
        @click="assertions = [...assertions, createAssertion()]">
        <Plus :size="13" aria-hidden="true" />
        Add assertion
      </Button>
    </section>
    <section aria-labelledby="captures-heading" class="mt-2">
      <h3
        id="captures-heading"
        class="flex h-8 items-center justify-between px-3 font-mono text-[0.625rem] tracking-[0.12em] text-muted-foreground">
        CAPTURES
        <HelpTooltip
          text="After each successful send, saves a response value as a workspace token. Use it in other requests as {{name}}.">
          <button
            type="button"
            class="font-sans text-[0.6875rem] tracking-normal underline decoration-dotted underline-offset-3">
            Help
          </button>
        </HelpTooltip>
      </h3>
      <table class="w-full table-fixed border-collapse" aria-label="Captures">
        <thead>
          <tr>
            <th class="w-8.5 h-8 bg-muted">
              <span class="sr-only">Enabled</span>
            </th>
            <th scope="col" :class="head">Token</th>
            <th scope="col" :class="head" class="w-[28%]">Source</th>
            <th scope="col" :class="head">Path</th>
            <th class="w-8.5 h-8 bg-muted border-l border-border">
              <span class="sr-only">Remove</span>
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(row, index) in captures" :key="row.id" data-capture>
            <td class="h-8.5 border-b border-border text-center">
              <input
                type="checkbox"
                class="accent-primary w-3 h-3"
                :aria-label="`Enable capture ${index + 1}`"
                :checked="row.enabled"
                :disabled="disabled"
                @change="updateCapture(row.id, { enabled: !row.enabled })" />
            </td>
            <td :class="cell">
              <input
                :class="[
                  input,
                  {
                    'text-destructive': row.name && !CAPTURE_NAME_RE.test(row.name),
                  },
                ]"
                class="w-full"
                :aria-label="`Capture ${index + 1} token name`"
                :aria-invalid="(row.name && !CAPTURE_NAME_RE.test(row.name)) || undefined"
                :title="
                  row.name && !CAPTURE_NAME_RE.test(row.name)
                    ? 'Start with a letter. Use letters, digits, _, . or -.'
                    : undefined
                "
                placeholder="name"
                :value="row.name"
                :disabled="disabled"
                spellcheck="false"
                autocomplete="off"
                data-capture-name
                @input="
                  updateCapture(row.id, {
                    name: ($event.target as HTMLInputElement).value,
                  })
                " />
            </td>
            <td :class="cell">
              <select
                :class="select"
                class="w-full"
                :aria-label="`Capture ${index + 1} source`"
                :value="row.source"
                :disabled="disabled"
                @change="
                  updateCapture(row.id, {
                    source: ($event.target as HTMLSelectElement).value as Capture['source'],
                  })
                ">
                <option v-for="source in captureSources" :key="source.id" :value="source.id">
                  {{ source.label }}
                </option>
              </select>
            </td>
            <td :class="cell">
              <input
                v-if="pathSources.includes(row.source)"
                :class="input"
                class="w-full"
                :aria-label="`Capture ${index + 1} path`"
                :placeholder="pathPlaceholder(row.source)"
                :value="row.path"
                :disabled="disabled"
                spellcheck="false"
                autocomplete="off"
                data-capture-path
                @input="
                  updateCapture(row.id, {
                    path: ($event.target as HTMLInputElement).value,
                  })
                " />
            </td>
            <td class="h-8.5 border-b border-l border-border p-0">
              <Button
                variant="ghost"
                class="size-7 shrink-0 p-0"
                :aria-label="`Remove capture ${index + 1}`"
                :disabled="disabled"
                @click="captures = captures.filter((candidate) => candidate.id !== row.id)">
                <X :size="13" aria-hidden="true" />
              </Button>
            </td>
          </tr>
        </tbody>
      </table>
      <Button
        variant="ghost"
        class="m-1.5"
        data-add-capture
        :disabled="disabled"
        @click="captures = [...captures, createCapture('', 'json', '.')]">
        <Plus :size="13" aria-hidden="true" />
        Add capture
      </Button>
    </section>
  </div>
</template>
