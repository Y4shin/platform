import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { z } from 'zod';

import { Button } from './Button.js';
import { Form } from './Form.js';
import { FormField } from './FormField.js';
import { Input } from './Input.js';
import { Select } from './Select.js';

const schema = z.object({
  title: z.string().min(1, 'Title is required'),
  visibility: z.enum(['private', 'public']),
});
type Values = z.infer<typeof schema>;

function TestForm({ onSubmit }: { onSubmit: (v: Values) => void }) {
  return (
    <Form schema={schema} defaultValues={{ title: '', visibility: 'private' }} onSubmit={onSubmit}>
      <FormField<Values> name="title" label="Title">
        {(field) => <Input {...field} placeholder="Title" />}
      </FormField>
      <FormField<Values> name="visibility" label="Visibility">
        {(field) => (
          <Select
            {...field}
            options={[
              { value: 'private', label: 'Private' },
              { value: 'public', label: 'Public' },
            ]}
          />
        )}
      </FormField>
      <Button type="submit">Save</Button>
    </Form>
  );
}

describe('Form', () => {
  it('submits parsed values when valid', async () => {
    const onSubmit = vi.fn();
    render(<TestForm onSubmit={onSubmit} />);

    fireEvent.change(screen.getByPlaceholderText('Title'), { target: { value: 'Launch' } });
    fireEvent.change(screen.getByLabelText('Visibility'), { target: { value: 'public' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));
    expect(onSubmit.mock.calls[0]?.[0]).toMatchObject({ title: 'Launch', visibility: 'public' });
  });

  it('blocks submit and shows the zod error when invalid', async () => {
    const onSubmit = vi.fn();
    render(<TestForm onSubmit={onSubmit} />);

    // Title left empty → schema fails.
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('Title is required');
    expect(onSubmit).not.toHaveBeenCalled();
  });
});
