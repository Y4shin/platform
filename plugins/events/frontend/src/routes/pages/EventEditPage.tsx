import { useMutation, useQuery } from '@connectrpc/connect-query';
import {
  Button,
  Card,
  createFormField,
  DateRange,
  Form,
  Input,
  Select,
  Stack,
  Textarea,
} from '@junius/design';
import { rpc } from '@junius/generated/events/rpc';
import { useUser } from '@junius/sdk';
import { Trans, useLingui } from '@lingui/react/macro';
import { useParams } from '@tanstack/react-router';
import { z } from 'zod';

import { toInputValue, toRfc3339 } from '../../domain.js';
import { usePluginNavigate } from '../../nav.js';

const schema = z.object({
  title: z.string().min(1, 'Title is required'),
  description: z.string(),
  location: z.string(),
  schedule: z.object({
    start: z.string().min(1, 'A start date/time is required'),
    end: z.string(),
    allDay: z.boolean(),
  }),
  visibility: z.enum(['private', 'public']),
  owner: z.string(),
});
type FormValues = z.infer<typeof schema>;

const Field = createFormField<FormValues>();

/** Create a new event, or edit an existing one (ownership is immutable, so the
 * owner selector only appears on create). Uses the `@junius/design` forms lib;
 * editing affordances elsewhere are gated by the server's `viewerCanEdit`. */
export function EventEditPage() {
  const { eventId } = useParams({ strict: false }) as { eventId?: string };
  const isEdit = Boolean(eventId);
  const nav = usePluginNavigate();
  const user = useUser();

  const existing = useQuery(rpc.EventService.getEvent, { id: eventId ?? '' }, { enabled: isEdit });
  const create = useMutation(rpc.EventService.createEvent, {
    onSuccess: (res) => res.event && nav('/p/events/$eventId', { eventId: res.event.id }),
  });
  const update = useMutation(rpc.EventService.updateEvent, {
    onSuccess: (res) => res.event && nav('/p/events/$eventId', { eventId: res.event.id }),
  });
  // Hooks must precede any conditional return (React rules-of-hooks).
  const { t } = useLingui();

  if (isEdit && existing.isPending) {
    return (
      <Card>
        <p className="text-fg-2 text-sm">
          <Trans>Loading…</Trans>
        </p>
      </Card>
    );
  }

  const ev = existing.data?.event;
  const defaultValues: FormValues = ev
    ? {
        title: ev.title,
        description: ev.description,
        location: ev.location,
        schedule: {
          start: toInputValue(ev.startsAt, ev.allDay),
          end: toInputValue(ev.endsAt, ev.allDay),
          allDay: ev.allDay,
        },
        visibility: ev.visibility === 'public' ? 'public' : 'private',
        owner: 'user',
      }
    : {
        title: '',
        description: '',
        location: '',
        schedule: { start: '', end: '', allDay: false },
        visibility: 'private',
        owner: 'user',
      };

  const ownerOptions = [
    { value: 'user', label: t`Myself` },
    ...(user?.memberships ?? []).map((m) => ({ value: `group:${m.groupId}`, label: m.groupName })),
  ];

  const onSubmit = (values: FormValues) => {
    const startsAt = toRfc3339(values.schedule.start);
    const endsAt = toRfc3339(values.schedule.end);
    const common = {
      title: values.title,
      description: values.description,
      location: values.location,
      startsAt,
      endsAt,
      allDay: values.schedule.allDay,
      visibility: values.visibility,
    };
    if (isEdit && eventId) {
      update.mutate({ id: eventId, ...common });
      return;
    }
    const isGroup = values.owner.startsWith('group:');
    create.mutate({
      ...common,
      ownerKind: isGroup ? 'group' : 'user',
      ownerId: isGroup ? values.owner.slice('group:'.length) : '',
    });
  };

  return (
    <Stack gap="md">
      <h1 className="font-semibold text-xl">
        {isEdit ? <Trans>Edit event</Trans> : <Trans>New event</Trans>}
      </h1>
      <Card>
        <Form schema={schema} defaultValues={defaultValues} onSubmit={onSubmit}>
          <Stack gap="md">
            <Field name="title" label={t`Title`}>
              {(field) => <Input {...field} placeholder={t`Team offsite`} />}
            </Field>
            <Field name="description" label={t`Description`}>
              {(field) => <Textarea {...field} />}
            </Field>
            <Field name="location" label={t`Location`}>
              {(field) => <Input {...field} placeholder={t`Where`} />}
            </Field>
            <Field name="schedule" label={t`When`}>
              {(field) => <DateRange value={field.value} onChange={field.onChange} />}
            </Field>
            <Field name="visibility" label={t`Visibility`}>
              {(field) => (
                <Select
                  {...field}
                  options={[
                    { value: 'private', label: t`Private` },
                    { value: 'public', label: t`Public (world-readable)` },
                  ]}
                />
              )}
            </Field>
            {!isEdit && (
              <Field name="owner" label={t`Owner`}>
                {(field) => <Select {...field} options={ownerOptions} />}
              </Field>
            )}
            <div>
              <Button type="submit" disabled={create.isPending || update.isPending}>
                {isEdit ? <Trans>Save changes</Trans> : <Trans>Create event</Trans>}
              </Button>
            </div>
          </Stack>
        </Form>
      </Card>
    </Stack>
  );
}
