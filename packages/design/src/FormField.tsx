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
export type FormFieldRender<TValues extends FieldValues> = ControllerRenderProps<TValues> & {
  id: string;
};

export interface FormFieldProps<TValues extends FieldValues> {
  name: FieldPath<TValues>;
  label?: string;
  /** Helper text shown below the control when there is no error. */
  description?: string;
  children: (field: FormFieldRender<TValues>) => ReactNode;
}

/**
 * A single labelled form control bound (by `name`) to the surrounding [`Form`]'s
 * react-hook-form context. Renders the label, the caller's control (via the
 * render child), and either the helper text or the validation error.
 */
export function FormField<TValues extends FieldValues>({
  name,
  label,
  description,
  children,
}: FormFieldProps<TValues>) {
  const { control } = useFormContext<TValues>();
  const { field, fieldState } = useController({ name, control });
  const id = `field-${name}`;
  const error = fieldState.error?.message;
  return (
    <div className="flex flex-col gap-1">
      {label !== undefined && (
        <label htmlFor={id} className="text-sm font-medium text-fg-1">
          {label}
        </label>
      )}
      {children({ ...field, id })}
      {error ? (
        <p className="text-xs text-danger" role="alert">
          {error}
        </p>
      ) : (
        description !== undefined && <p className="text-xs text-fg-2">{description}</p>
      )}
    </div>
  );
}
