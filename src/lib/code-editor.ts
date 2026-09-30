import { Annotation, Compartment, EditorState, StateEffect, StateField } from '@codemirror/state'
import {
  Decoration,
  EditorView,
  ViewPlugin,
  hoverTooltip,
  drawSelection,
  keymap,
  placeholder,
  type DecorationSet,
  type ViewUpdate,
} from '@codemirror/view'
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
import {
  HighlightStyle,
  bracketMatching,
  indentOnInput,
  indentUnit,
  syntaxHighlighting,
} from '@codemirror/language'
import {
  autocompletion,
  closeBrackets,
  closeBracketsKeymap,
  completionKeymap,
  type CompletionContext,
} from '@codemirror/autocomplete'
import { json } from '@codemirror/lang-json'
import { graphql } from 'cm6-graphql'
import { tags } from '@lezer/highlight'
import type { GraphQLSchema } from 'graphql'
import type { InterpolationContext } from './interpolation'
import { tokenHint, tokenOptions, tokenRanges } from './token-hints'

export type CodeLanguage = 'json' | 'graphql'

export type CodeEditorOptions = {
  parent: HTMLElement
  doc: string
  language: CodeLanguage
  schema?: GraphQLSchema
  disabled: boolean
  placeholder?: string
  tokens?: InterpolationContext
  attributes: Record<string, string>
  onChange: (value: string) => void
}

export type CodeEditorHandle = {
  view: EditorView
  setDoc(value: string): void
  setLanguage(language: CodeLanguage, schema?: GraphQLSchema): void
  setDisabled(disabled: boolean): void
  setPlaceholder(text?: string): void
  setTokens(tokens?: InterpolationContext): void
  markError(offset: number): void
  destroy(): void
}

/** App shortcuts (send, focus URL) that CodeMirror must not consume. */
const reservedKeys = new Set(['Mod-Enter', 'Mod-l'])
const external = Annotation.define<boolean>()

/** Line mark for a format error. Any edit clears it. */
const setErrorLine = StateEffect.define<number>()
const errorLineMark = Decoration.line({ class: 'cm-errorLine' })
const errorLine = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(marks, tr) {
    if (tr.docChanged) marks = Decoration.none
    for (const effect of tr.effects)
      if (effect.is(setErrorLine))
        marks = Decoration.set([errorLineMark.range(tr.state.doc.lineAt(effect.value).from)])
    return marks
  },
  provide: (field) => EditorView.decorations.from(field),
})

// Mirrors the response CodeView highlight colors.
const highlight = HighlightStyle.define([
  { tag: [tags.propertyName, tags.attributeName], color: 'var(--foreground)' },
  { tag: [tags.string, tags.special(tags.string)], color: 'var(--success)' },
  {
    tag: [tags.number, tags.bool, tags.null, tags.atom],
    color: 'var(--warning)',
  },
  {
    tag: [tags.keyword, tags.definitionKeyword, tags.operatorKeyword, tags.typeName],
    color: 'var(--keyword)',
  },
  { tag: [tags.comment, tags.meta], color: 'var(--muted-foreground)' },
  { tag: tags.invalid, color: 'var(--destructive)' },
])

const theme = EditorView.theme(
  {
    '&': {
      flex: '1',
      minHeight: '0',
      fontSize: '0.8125rem',
      color: 'var(--foreground)',
      backgroundColor: 'transparent',
    },
    '&.cm-focused': { outline: 'none' },
    '.cm-scroller': {
      fontFamily: 'var(--font-mono)',
      lineHeight: '1.75',
      overflow: 'auto',
    },
    '.cm-content': { padding: '1rem 0', caretColor: 'var(--foreground)' },
    '.cm-line': { padding: '0 1rem' },
    '.cm-placeholder': { color: 'var(--muted-foreground)', opacity: '0.85' },
    '&.cm-focused .cm-selectionBackground, .cm-selectionBackground': {
      backgroundColor: 'var(--selection)',
    },
    '.cm-matchingBracket': { outline: '1px solid var(--border)' },
    '.cm-errorLine': {
      backgroundColor: 'color-mix(in oklch, var(--destructive) 18%, transparent)',
    },
    '.cm-token': { color: 'var(--keyword)' },
    '.cm-token-env': { color: 'var(--warning)' },
    '.cm-token-unresolved': {
      color: 'var(--destructive)',
      textDecoration: 'underline wavy',
      textUnderlineOffset: '3px',
    },
    '.cm-token-hint': {
      maxWidth: '30rem',
      padding: '0.25rem 0.5rem',
      overflow: 'hidden',
      textOverflow: 'ellipsis',
      whiteSpace: 'nowrap',
      fontFamily: 'var(--font-mono)',
      fontSize: '0.75rem',
    },
    '.cm-completionDetail': {
      color: 'var(--muted-foreground)',
      fontStyle: 'normal',
      marginLeft: '1rem',
    },
    '.cm-tooltip': {
      backgroundColor: 'var(--muted)',
      border: '1px solid var(--border)',
      color: 'var(--foreground)',
    },
    '.cm-tooltip-autocomplete > ul > li[aria-selected]': {
      backgroundColor: 'var(--accent)',
      color: 'var(--foreground)',
    },
  },
  { dark: true },
)

const tokenMarks = {
  resolved: Decoration.mark({ class: 'cm-token' }),
  unresolved: Decoration.mark({ class: 'cm-token cm-token-unresolved' }),
  env: Decoration.mark({ class: 'cm-token cm-token-env' }),
}

/** Color `{{name}}` references and suggest token names after `{{`. */
function tokenExtension(tokens?: InterpolationContext) {
  if (!tokens) return []
  const build = (view: EditorView) =>
    Decoration.set(
      view.visibleRanges.flatMap(({ from, to }) =>
        tokenRanges(view.state.sliceDoc(from, to), tokens).map((range) =>
          tokenMarks[range.token].range(from + range.from, from + range.to),
        ),
      ),
    )
  const marks = ViewPlugin.fromClass(
    class {
      decorations: DecorationSet
      constructor(view: EditorView) {
        this.decorations = build(view)
      }
      update(update: ViewUpdate) {
        if (update.docChanged || update.viewportChanged) this.decorations = build(update.view)
      }
    },
    { decorations: (plugin) => plugin.decorations },
  )
  const options = tokenOptions(tokens).map((option) => ({
    label: option.name,
    detail: option.value,
    info: option.scope === 'local' ? 'Group token' : 'Global token',
    type: 'variable',
  }))
  const hover = hoverTooltip((view, pos) => {
    const line = view.state.doc.lineAt(pos)
    const range = tokenRanges(line.text, tokens).find(
      (r) => line.from + r.from <= pos && pos < line.from + r.to,
    )
    if (!range) return null
    const text = tokenHint(
      {
        text: line.text.slice(range.from, range.to),
        token: range.token,
        name: range.name,
      },
      tokens,
    )
    return {
      pos: line.from + range.from,
      end: line.from + range.to,
      above: true,
      create: () => {
        const dom = document.createElement('div')
        dom.className = 'cm-token-hint'
        dom.textContent = text
        return { dom }
      },
    }
  })
  const complete = (context: CompletionContext) => {
    const match = context.matchBefore(/\{\{[^{}\s]*/)
    if (!match) return null
    const from = match.from + 2
    const closing = context.state.sliceDoc(context.pos, context.pos + 2)
    return {
      from,
      options: options.map((option) => ({
        ...option,
        apply: closing === '}}' ? option.label : `${option.label}}}`,
      })),
      validFor: /^[^{}\s]*$/,
    }
  }
  return [marks, hover, EditorState.languageData.of(() => [{ autocomplete: complete }])]
}

const languageExtension = (language: CodeLanguage, schema?: GraphQLSchema) =>
  language === 'graphql' ? graphql(schema) : json()

const placeholderExtension = (text?: string) => (text ? placeholder(text) : [])

const editable = (enabled: boolean) => [
  EditorState.readOnly.of(!enabled),
  EditorView.editable.of(enabled),
]

export function createCodeEditor(options: CodeEditorOptions): CodeEditorHandle {
  const languageSlot = new Compartment()
  const editableSlot = new Compartment()
  const placeholderSlot = new Compartment()
  const tokenSlot = new Compartment()
  const view = new EditorView({
    parent: options.parent,
    state: EditorState.create({
      doc: options.doc,
      extensions: [
        history(),
        drawSelection(),
        indentOnInput(),
        bracketMatching(),
        closeBrackets(),
        autocompletion(),
        indentUnit.of('  '),
        EditorState.tabSize.of(2),
        syntaxHighlighting(highlight),
        theme,
        errorLine,
        placeholderSlot.of(placeholderExtension(options.placeholder)),
        tokenSlot.of(tokenExtension(options.tokens)),
        keymap.of([
          ...closeBracketsKeymap,
          ...completionKeymap,
          ...historyKeymap,
          ...defaultKeymap.filter((b) => !reservedKeys.has(b.key ?? '')),
          indentWithTab,
        ]),
        EditorView.contentAttributes.of(options.attributes),
        languageSlot.of(languageExtension(options.language, options.schema)),
        editableSlot.of(editable(!options.disabled)),
        EditorView.updateListener.of((update) => {
          if (update.docChanged && !update.transactions.some((t) => t.annotation(external)))
            options.onChange(update.state.doc.toString())
        }),
      ],
    }),
  })
  return {
    view,
    setDoc(value) {
      if (view.state.doc.toString() === value) return
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: value },
        annotations: external.of(true),
      })
    },
    setLanguage(language, schema) {
      view.dispatch({
        effects: languageSlot.reconfigure(languageExtension(language, schema)),
      })
    },
    setPlaceholder(text) {
      view.dispatch({
        effects: placeholderSlot.reconfigure(placeholderExtension(text)),
      })
    },
    setTokens(tokens) {
      view.dispatch({ effects: tokenSlot.reconfigure(tokenExtension(tokens)) })
    },
    markError(offset) {
      const pos = Math.max(0, Math.min(offset, view.state.doc.length))
      view.dispatch({
        selection: { anchor: pos },
        effects: [setErrorLine.of(pos), EditorView.scrollIntoView(pos, { y: 'center' })],
      })
      view.focus()
    },
    setDisabled(disabled) {
      view.dispatch({ effects: editableSlot.reconfigure(editable(!disabled)) })
    },
    destroy: () => view.destroy(),
  }
}
