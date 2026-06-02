# Forms

Non-trivial forms use the zod-validated `Form` / `FormField` wrappers from
`@junius/design`, layered over react-hook-form. One zod schema drives both
validation **and** the inferred value type, so your form fields are type-checked
against the data they edit.

## The shape of a form

```tsx
import { z } from 'zod';
import { Form, createFormField, Input, DateRange } from '@junius/design';

const schema = z.object({
  title: z.string().min(1, 'Title is required'),
  schedule: z.object({ start: z.string(), end: z.string(), allDay: z.boolean() }),
});
type FormValues = z.infer<typeof schema>;

const Field = createFormField<FormValues>();   // per-field typed `field.value`

export function EventEditPage() {
  return (
    <Form schema={schema} defaultValues={{ title: '', schedule: { /* … */ } }} onSubmit={handleSubmit}>
      <Field name="title" label="Title">
        {(field) => <Input {...field} />}
      </Field>
      <Field name="schedule" label="When">
        {(f) => <DateRange value={f.value} onChange={f.onChange} />}
      </Field>
    </Form>
  );
}
```

See the real version in
`plugins/events/frontend/src/routes/pages/EventEditPage.tsx`.

## Use `createFormField<T>()`, not bare `FormField`

`createFormField<FormValues>()` produces a `Field` component bound to your
schema's type, so each `name` infers its **own** `field.value` type. With a bare
`FormField` on a heterogeneous schema you lose that per-field inference. Always
create the typed field factory.

## Available inputs

`@junius/design` ships the common inputs:

| Component | For |
| --- | --- |
| `Input` | single-line text |
| `Textarea` | multi-line text |
| `Select` | single choice |
| `DateTimeInput` | a date + time |
| `DateRange` | a start → end range with an all-day toggle |

## A dependency note

A plugin authoring zod schemas needs **`zod` as its own dependency** in
`frontend/package.json` — it isn't transitively provided. Add it and
`pnpm install`.

## Localized validation messages

Validation strings are user-facing, so they go through i18n. Build the schema
**inside** the component (memoized on the `t` function) so the messages render in
the active locale:

```tsx
import { useLingui } from '@lingui/react/macro';
import { useMemo } from 'react';

function useEventSchema() {
  const { t } = useLingui();
  return useMemo(
    () => z.object({ title: z.string().min(1, t`Title is required`) }),
    [t],
  );
}
```

The i18n mechanics are in [Internationalization](../capabilities/i18n.md).

Next: sharing components with other plugins.
