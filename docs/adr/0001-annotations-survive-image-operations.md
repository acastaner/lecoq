# Annotations survive Image operations

Crop, rotate, flip and resize transform the Annotations along with the Image instead of Flattening them first. People often annotate first and crop or rotate afterwards, and they expect to keep adjusting what they drew. Concretely:

- **Crop** clips Annotations and removes those entirely outside; undoing the crop restores them.
- **Rotate and flip** transform shapes, lines and arrows, but text and Step counters stay upright.
- **Resize** scales everything, including line thickness and font size.

Decided in [Annotation and undo semantics](https://github.com/acastaner/lecoq/issues/7).

## Considered Options

- **Flatten before any Image operation.** Simpler and more robust, because Image operations would touch only pixels. Rejected because annotating before cropping is common, and losing the ability to edit would make that workflow worse.

## Consequences

- Every Annotation type needs a geometric transform for crop offsets, 90° rotations, flips and scaling. Text and Step counters have their own rule: their anchor moves but they don't rotate.
- Highlight and obfuscate recompute live from what lies beneath them, so they re-render correctly after any Image operation.
- The History stores both the pre-operation pixels and the pre-operation Annotation geometry for each Image operation.
