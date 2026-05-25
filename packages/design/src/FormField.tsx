import type { ReactNode } from 'react';
import {
  type ControllerRenderProps,
  type FieldPath,
  type FieldValues,
  useController,
  useFormContext,
} from 'react-hook-form';

/** The bound field props handed to the render child, plus a stable `id` (wired to
 * the field's `<label>`). Spread onto an `Input`/`Select`/`Textarea`. */
export type FormFieldRender<
  TValues extends FieldValues,
  TName extends FieldPath<TValues> = FieldPath<TValues>,
> = ControllerRenderProps<TValues, TName> & { id: string };

export interface FormFieldProps<
  TValues extends FieldValues,
  TName extends FieldPath<TValues> = FieldPath<TValues>,
> {
  name: TName;
  label?: string;
  /** Helper text shown below the control when there is no error. */
  description?: string;
  children: (field: FormFieldRender<TValues, TName>) => ReactNode;
}

/**
 * A single labelled form control bound (by `name`) to the surrounding [`Form`]'s
 * react-hook-form context. Renders the label, the caller's control (via the
 * render child), and either the helper text or the validation error.
 *
 * `field.value` is typed to the *named* field. For a form whose fields have mixed
 * types, bind the values type once with [`createFormField`] so each `name` infers
 * its own value type at the call site.
 */
export function FormField<
  TValues extends FieldValues,
  TName extends FieldPath<TValues> = FieldPath<TValues>,
>({ name, label, description, children }: FormFieldProps<TValues, TName>) {
  const { control } = useFormContext<TValues>();
  const { field, fieldState } = useController<TValues, TName>({ name, control });
  const id = `field-${name}`;
  const error = fieldState.error?.message;
  return (
    <div className="flex flex-col gap-1">
      {label !== undefined && (
        <label htmlFor={id} className="font-medium text-fg-1 text-sm">
          {label}
        </label>
      )}
      {children({ ...field, id })}
      {error ? (
        <p className="text-danger text-xs" role="alert">
          {error}
        </p>
      ) : (
        description !== undefined && <p className="text-fg-2 text-xs">{description}</p>
      )}
    </div>
  );
}

/**
 * Bind [`FormField`] to a specific form-values type, returning a `Field` component
 * whose `name` is checked and whose render `field.value` is precisely typed per
 * field — the ergonomic way to use it with a heterogeneous schema:
 *
 * ```tsx
 * const Field = createFormField<FormValues>();
 * <Field name="title">{(f) => <Input {...f} />}</Field>
 * ```
 */
export function createFormField<TValues extends FieldValues>() {
  return FormField as <TName extends FieldPath<TValues>>(
    props: FormFieldProps<TValues, TName>,
  ) => ReactNode;
}
