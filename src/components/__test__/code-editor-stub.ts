import { defineComponent, h } from "vue";

/** Textarea stand-in for CodeEditor so RequestEditor tests avoid CodeMirror. */
export default defineComponent({
  name: "CodeEditor",
  props: {
    modelValue: { type: String, default: "" },
    language: { type: String, required: true },
    schema: { type: Object, default: undefined },
    disabled: Boolean,
    placeholder: { type: String, default: undefined },
    id: { type: String, default: undefined },
    ariaLabelledby: { type: String, default: undefined },
    testId: { type: String, default: undefined },
  },
  emits: ["update:modelValue"],
  setup(props, { emit }) {
    return () =>
      h("textarea", {
        id: props.id,
        value: props.modelValue,
        disabled: props.disabled,
        placeholder: props.placeholder,
        "aria-labelledby": props.ariaLabelledby,
        "data-testid": props.testId,
        "data-language": props.language,
        "data-schema": props.schema ? "loaded" : "none",
        onInput: (event: Event) =>
          emit(
            "update:modelValue",
            (event.target as HTMLTextAreaElement).value,
          ),
      });
  },
});
