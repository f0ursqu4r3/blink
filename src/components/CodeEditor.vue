<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import type { GraphQLSchema } from 'graphql'
import type { CodeEditorHandle, CodeLanguage } from '@/lib/code-editor'
import type { InterpolationContext } from '@/lib/interpolation'
const model = defineModel<string>({ default: '' })
const props = defineProps<{
  language: CodeLanguage
  schema?: GraphQLSchema
  disabled?: boolean
  placeholder?: string
  /** Tokens to color and suggest after `{{`. */
  tokens?: InterpolationContext
  id?: string
  ariaLabelledby?: string
  testId?: string
}>()
const host = ref<HTMLElement>()
let handle: CodeEditorHandle | undefined
let disposed = false

onMounted(async () => {
  // CodeMirror loads on first use to keep startup lean.
  const { createCodeEditor } = await import('@/lib/code-editor')
  if (disposed || !host.value) return
  const attributes: Record<string, string> = {}
  if (props.id) attributes.id = props.id
  if (props.ariaLabelledby) attributes['aria-labelledby'] = props.ariaLabelledby
  if (props.testId) attributes['data-testid'] = props.testId
  handle = createCodeEditor({
    parent: host.value,
    doc: model.value,
    language: props.language,
    schema: props.schema,
    disabled: !!props.disabled,
    placeholder: props.placeholder,
    tokens: props.tokens,
    attributes,
    onChange: (value) => (model.value = value),
  })
})
onBeforeUnmount(() => {
  disposed = true
  handle?.destroy()
})
watch(model, (value) => handle?.setDoc(value))
watch(
  () => [props.language, props.schema] as const,
  ([language, schema]) => handle?.setLanguage(language, schema),
)
watch(
  () => !!props.disabled,
  (disabled) => handle?.setDisabled(disabled),
)
defineExpose({
  /** Move the cursor to `offset` and mark its line until the next edit. */
  markError: (offset: number) => handle?.markError(offset),
})
watch(
  () => props.tokens,
  (tokens) => handle?.setTokens(tokens),
)
watch(
  () => props.placeholder,
  (text) => handle?.setPlaceholder(text),
)
</script>

<template>
  <div
    ref="host"
    class="flex min-h-0 flex-col overflow-hidden"
    :class="{ 'opacity-60': disabled }"
    @contextmenu.stop />
</template>
