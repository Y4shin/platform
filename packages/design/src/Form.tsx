import { zodResolver } from '@hookform/resolvers/zod';
import type { ReactNode } from 'react';
import {
  type DefaultValues,
  type FieldValues,
  FormProvider,
  type SubmitHandler,
  useForm,
} from 'react-hook-form';
import type { z } from 'zod';

export interface FormProps<TValues extends FieldValues> {
  /** The zod schema that validates the form values (resolver + types). */
  schema: z.ZodType<TValues>;
  defaultValues?: DefaultValues<TValues>;
  /** Called with the parsed, valid values on submit. */
  onSubmit: SubmitHandler<TValues>;
  children: ReactNode;
  className?: string;
}

/**
 * A zod-validated form: wires `react-hook-form` to a zod schema via
 * `@hookform/resolvers` and exposes the form context to nested [`FormField`]s.
 * Validation runs on submit (and re-runs on change once a field has errored).
 */
export function Form<TValues extends FieldValues>({
  schema,
  defaultValues,
  onSubmit,
  children,
  className,
}: FormProps<TValues>) {
  const methods = useForm<TValues>({
    resolver: zodResolver(schema),
    defaultValues,
  });
  return (
    <FormProvider {...methods}>
      <form className={className} onSubmit={methods.handleSubmit(onSubmit)} noValidate>
        {children}
      </form>
    </FormProvider>
  );
}
