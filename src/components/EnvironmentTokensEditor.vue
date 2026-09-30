<script setup lang="ts">
import { Plus, Trash2, X } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuCheckboxItem,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuLabel,
} from '@/components/ui/dropdown-menu'
import {
  environmentColorLabels,
  environmentColors,
  MAX_ENVIRONMENT_NAME,
  MAX_ENVIRONMENTS,
  type EnvironmentColor,
} from '@/lib/environments'

/** One token: a base value and a value per environment ("" inherits). */
export type EnvironmentRow = {
  id: number
  key: string
  base: string
  values: Record<number, string>
}
export type EnvironmentColumn = {
  id: number
  name: string
  color: EnvironmentColor
  protected?: boolean
}

const rows = defineModel<EnvironmentRow[]>('rows', { required: true })
const columns = defineModel<EnvironmentColumn[]>('columns', {
  required: true,
})
const emit = defineEmits<{ addRow: []; addColumn: [] }>()

function setValue(row: EnvironmentRow, columnId: number, value: string) {
  row.values = { ...row.values, [columnId]: value }
}
function patchColumn(id: number, patch: Partial<EnvironmentColumn>) {
  columns.value = columns.value.map((column) =>
    column.id === id ? { ...column, ...patch } : column,
  )
}
function removeColumn(id: number) {
  columns.value = columns.value.filter((column) => column.id !== id)
  for (const row of rows.value) {
    const { [id]: _removed, ...rest } = row.values
    row.values = rest
  }
}
const cell = 'h-8.5 border-b border-l border-border p-0'
const input =
  'h-8.25 w-full min-w-0 bg-transparent border-0 px-2.5 font-mono text-xs rounded-none pointer-coarse:h-11'
</script>

<template>
  <div class="overflow-x-auto" data-environment-tokens>
    <table class="w-full min-w-max border-collapse" aria-label="Tokens and environments">
      <thead>
        <tr
          class="h-8 bg-muted text-left text-[0.625rem] uppercase tracking-widest text-muted-foreground">
          <th scope="col" class="sticky left-0 z-1 w-36 min-w-36 bg-muted px-2.5 font-medium">
            Name
          </th>
          <th scope="col" class="min-w-40 border-l border-border px-2.5 font-medium">Value</th>
          <th
            v-for="column in columns"
            :key="column.id"
            scope="col"
            class="w-36 min-w-36 border-l border-border p-0 font-medium"
            :data-environment-column="column.id">
            <DropdownMenu>
              <DropdownMenuTrigger as-child>
                <button
                  type="button"
                  class="flex h-8 w-full items-center gap-1.5 px-2.5 font-mono text-[0.625rem] font-semibold tracking-widest uppercase hover:bg-accent"
                  :style="{ color: `var(--${column.color})` }"
                  :aria-label="`${column.name} environment options`"
                  data-environment-header>
                  <span class="truncate">{{ column.name || 'Unnamed' }}</span>
                  <span
                    v-if="column.protected"
                    class="text-[0.5625rem] tracking-normal normal-case text-muted-foreground"
                    title="Protected: asks before the first send">
                    protected
                  </span>
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start" class="w-52">
                <div class="px-2 py-1.5">
                  <label class="sr-only" :for="`env-name-${column.id}`">Environment name</label>
                  <input
                    :id="`env-name-${column.id}`"
                    :value="column.name"
                    :maxlength="MAX_ENVIRONMENT_NAME"
                    placeholder="Name"
                    spellcheck="false"
                    autocomplete="off"
                    data-environment-name
                    class="h-7 w-full rounded-sm border border-input bg-background px-2 font-mono text-xs uppercase focus:border-primary"
                    @keydown.stop
                    @input="
                      patchColumn(column.id, {
                        name: ($event.target as HTMLInputElement).value,
                      })
                    " />
                </div>
                <DropdownMenuSeparator />
                <DropdownMenuLabel>Color</DropdownMenuLabel>
                <DropdownMenuRadioGroup
                  :model-value="column.color"
                  @update:model-value="
                    (color) =>
                      patchColumn(column.id, {
                        color: color as EnvironmentColor,
                      })
                  ">
                  <DropdownMenuRadioItem
                    v-for="color in environmentColors"
                    :key="color"
                    :value="color">
                    <span class="size-2 rounded-full" :style="{ background: `var(--${color})` }" />
                    {{ environmentColorLabels[color] }}
                  </DropdownMenuRadioItem>
                </DropdownMenuRadioGroup>
                <DropdownMenuSeparator />
                <DropdownMenuCheckboxItem
                  :model-value="Boolean(column.protected)"
                  data-environment-protected
                  @update:model-value="
                    (value) => patchColumn(column.id, { protected: Boolean(value) })
                  ">
                  Protected
                </DropdownMenuCheckboxItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem
                  variant="destructive"
                  data-environment-delete
                  @select="removeColumn(column.id)">
                  <Trash2 :size="13" aria-hidden="true" />
                  Delete environment
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </th>
          <th class="w-px border-l border-border p-0">
            <Button
              variant="ghost"
              class="size-8 p-0"
              aria-label="Add environment"
              title="Add environment"
              data-add-environment
              :disabled="columns.length >= MAX_ENVIRONMENTS"
              @click="emit('addColumn')">
              <Plus :size="13" aria-hidden="true" />
            </Button>
          </th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="(row, index) in rows" :key="row.id" data-token-row>
          <td class="sticky left-0 z-1 h-8.5 border-b border-border bg-background p-0">
            <input
              v-model="row.key"
              :class="input"
              :aria-label="`Token name ${index + 1}`"
              placeholder="Name"
              spellcheck="false"
              autocomplete="off"
              data-row-name />
          </td>
          <td :class="cell">
            <input
              v-model="row.base"
              :class="input"
              :aria-label="`Token ${index + 1} value`"
              placeholder="Value"
              spellcheck="false"
              autocomplete="off"
              data-row-base />
          </td>
          <td v-for="column in columns" :key="column.id" :class="cell">
            <input
              :value="row.values[column.id] ?? ''"
              :class="input"
              class="placeholder:text-muted-foreground/60"
              :aria-label="`Token ${index + 1} value in ${column.name}`"
              :placeholder="row.base"
              :title="row.values[column.id] ? undefined : 'Empty uses the base value'"
              spellcheck="false"
              autocomplete="off"
              :data-row-environment="column.id"
              @input="setValue(row, column.id, ($event.target as HTMLInputElement).value)" />
          </td>
          <td class="h-8.5 w-px border-b border-l border-border p-0">
            <Button
              variant="ghost"
              class="size-8 p-0"
              :aria-label="`Remove token ${index + 1}`"
              @click="rows = rows.filter((candidate) => candidate !== row)">
              <X :size="13" aria-hidden="true" />
            </Button>
          </td>
        </tr>
      </tbody>
    </table>
    <Button variant="ghost" class="sticky left-0 m-1" data-add-token @click="emit('addRow')">
      <Plus :size="13" aria-hidden="true" />
      Add value
    </Button>
  </div>
</template>
