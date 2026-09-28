import { Annotation, Compartment, EditorState } from "@codemirror/state";
import {
  EditorView,
  drawSelection,
  keymap,
  placeholder,
} from "@codemirror/view";
import {
  defaultKeymap,
  history,
  historyKeymap,
  indentWithTab,
} from "@codemirror/commands";
import {
  HighlightStyle,
  bracketMatching,
  indentOnInput,
  indentUnit,
  syntaxHighlighting,
} from "@codemirror/language";
import {
  autocompletion,
  closeBrackets,
  closeBracketsKeymap,
  completionKeymap,
} from "@codemirror/autocomplete";
import { json } from "@codemirror/lang-json";
import { graphql } from "cm6-graphql";
import { tags } from "@lezer/highlight";
import type { GraphQLSchema } from "graphql";

export type CodeLanguage = "json" | "graphql";

export type CodeEditorOptions = {
  parent: HTMLElement;
  doc: string;
  language: CodeLanguage;
  schema?: GraphQLSchema;
  disabled: boolean;
  placeholder?: string;
  attributes: Record<string, string>;
  onChange: (value: string) => void;
};

export type CodeEditorHandle = {
  view: EditorView;
  setDoc(value: string): void;
  setLanguage(language: CodeLanguage, schema?: GraphQLSchema): void;
  setDisabled(disabled: boolean): void;
  setPlaceholder(text?: string): void;
  destroy(): void;
};

/** App shortcuts (send, focus URL) that CodeMirror must not consume. */
const reservedKeys = new Set(["Mod-Enter", "Mod-l"]);
const external = Annotation.define<boolean>();

// Mirrors the response CodeView highlight colors.
const highlight = HighlightStyle.define([
  { tag: [tags.propertyName, tags.attributeName], color: "var(--foreground)" },
  { tag: [tags.string, tags.special(tags.string)], color: "var(--success)" },
  {
    tag: [tags.number, tags.bool, tags.null, tags.atom],
    color: "var(--primary)",
  },
  {
    tag: [
      tags.keyword,
      tags.definitionKeyword,
      tags.operatorKeyword,
      tags.typeName,
    ],
    color: "oklch(0.8 0.07 240)",
  },
  { tag: [tags.comment, tags.meta], color: "var(--muted-foreground)" },
  { tag: tags.invalid, color: "var(--destructive)" },
]);

const theme = EditorView.theme(
  {
    "&": {
      flex: "1",
      minHeight: "0",
      fontSize: "0.8125rem",
      color: "var(--foreground)",
      backgroundColor: "transparent",
    },
    "&.cm-focused": { outline: "2px solid var(--ring)", outlineOffset: "-2px" },
    ".cm-scroller": {
      fontFamily: "var(--font-mono)",
      lineHeight: "1.75",
      overflow: "auto",
    },
    ".cm-content": { padding: "1rem 0", caretColor: "var(--foreground)" },
    ".cm-line": { padding: "0 1rem" },
    ".cm-placeholder": { color: "var(--muted-foreground)", opacity: "0.85" },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground": {
      backgroundColor: "oklch(0.82 0.145 85 / 25%)",
    },
    ".cm-matchingBracket": { outline: "1px solid var(--border)" },
    ".cm-tooltip": {
      backgroundColor: "var(--muted)",
      border: "1px solid var(--border)",
      color: "var(--foreground)",
    },
    ".cm-tooltip-autocomplete > ul > li[aria-selected]": {
      backgroundColor: "var(--accent)",
      color: "var(--foreground)",
    },
  },
  { dark: true },
);

const languageExtension = (language: CodeLanguage, schema?: GraphQLSchema) =>
  language === "graphql" ? graphql(schema) : json();

const placeholderExtension = (text?: string) => (text ? placeholder(text) : []);

const editable = (enabled: boolean) => [
  EditorState.readOnly.of(!enabled),
  EditorView.editable.of(enabled),
];

export function createCodeEditor(options: CodeEditorOptions): CodeEditorHandle {
  const languageSlot = new Compartment();
  const editableSlot = new Compartment();
  const placeholderSlot = new Compartment();
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
        indentUnit.of("  "),
        EditorState.tabSize.of(2),
        syntaxHighlighting(highlight),
        theme,
        placeholderSlot.of(placeholderExtension(options.placeholder)),
        keymap.of([
          ...closeBracketsKeymap,
          ...completionKeymap,
          ...historyKeymap,
          ...defaultKeymap.filter((b) => !reservedKeys.has(b.key ?? "")),
          indentWithTab,
        ]),
        EditorView.contentAttributes.of(options.attributes),
        languageSlot.of(languageExtension(options.language, options.schema)),
        editableSlot.of(editable(!options.disabled)),
        EditorView.updateListener.of((update) => {
          if (
            update.docChanged &&
            !update.transactions.some((t) => t.annotation(external))
          )
            options.onChange(update.state.doc.toString());
        }),
      ],
    }),
  });
  return {
    view,
    setDoc(value) {
      if (view.state.doc.toString() === value) return;
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: value },
        annotations: external.of(true),
      });
    },
    setLanguage(language, schema) {
      view.dispatch({
        effects: languageSlot.reconfigure(languageExtension(language, schema)),
      });
    },
    setPlaceholder(text) {
      view.dispatch({
        effects: placeholderSlot.reconfigure(placeholderExtension(text)),
      });
    },
    setDisabled(disabled) {
      view.dispatch({ effects: editableSlot.reconfigure(editable(!disabled)) });
    },
    destroy: () => view.destroy(),
  };
}
