# Smart capture

Type a task with `@space`, `#list`, `+tag`, P0–P3 (`p0` or `!0`), and a date such as `today`, `tomorrow`, a weekday, or `in 3 days`. `!`, `!!`, and `!!!` mean P2, P1, and P0. Adjacent destination tokens work: `buy milk @personal#groceries today`. Quote a name containing spaces: `@"Home life"#"Shopping list"`. The preview shows the interpreted title, destination and metadata. Unknown or ambiguous destination tokens remain in the title. Keep all text literal with the preview button. Settings can turn shortcut or date recognition off and keep quick capture open.

Smart capture sends no task text to an AI provider. Interpretations are committed with the captured task in one idempotent operation and the same payload is retained in the offline queue.

On a task page, **Break down…** in the task context menu explicitly sends its saved title and description plus optional global AI instructions to the configured OpenRouter provider. Suggestions are editable and must be reviewed before they become children of the saved task. Manual subtasks remain available if the provider is disabled. Configure global instructions and capture preferences in General settings.
