import { randomUUID } from 'node:crypto';
import { GenericContainer, type StartedTestContainer, Wait } from 'testcontainers';

const LABEL = 'junius-e2e-run';

export interface StackHandles {
  /** Unique label value identifying every container of this run. */
  runId: string;
  /** `postgres://platform_migrator:dev-only@host:port/platform` */
  databaseUrl: string;
  /** `amqp://guest:guest@host:port/%2f` */
  amqpUrl: string;
  /** S3 (MinIO) connection. */
  s3Endpoint: string;
  s3AccessKey: string;
  s3SecretKey: string;
  /** SMTP (mailpit). */
  smtpHost: string;
  smtpPort: number;
  /** mailpit HTTP API for inspecting captured messages. */
  mailpitHttpUrl: string;
  /** Internal container handles for stopStack(). */
  containers: StartedTestContainer[];
}

/**
 * Bring up an ephemeral, isolated stack for one Playwright run: Postgres,
 * RabbitMQ, MinIO, mailpit. **Authentik is deliberately omitted** — the
 * `loginAs` fixture seeds sessions directly, so no OIDC round-trip is needed
 * for the standard suite.
 *
 * Every container carries a `junius-e2e-run=<uuid>` label so a follow-up
 * teardown (or a `docker rm -f --filter label=...`) can sweep them
 * deterministically even after a crash.
 */
export async function startStack(): Promise<StackHandles> {
  const runId = randomUUID();
  const labels = { [LABEL]: runId };

  const [postgres, rabbitmq, minio, mailpit] = await Promise.all([
    new GenericContainer('postgres:17')
      .withEnvironment({
        POSTGRES_DB: 'platform',
        POSTGRES_USER: 'platform_migrator',
        POSTGRES_PASSWORD: 'dev-only',
      })
      .withExposedPorts(5432)
      .withLabels(labels)
      .withWaitStrategy(Wait.forLogMessage(/database system is ready to accept connections/, 2))
      .start(),
    new GenericContainer('rabbitmq:3-management')
      .withExposedPorts(5672)
      .withLabels(labels)
      .withWaitStrategy(Wait.forLogMessage(/Server startup complete/))
      .start(),
    new GenericContainer('minio/minio')
      .withEnvironment({
        MINIO_ROOT_USER: 'minioadmin',
        MINIO_ROOT_PASSWORD: 'minioadmin',
      })
      .withCommand(['server', '/data'])
      .withExposedPorts(9000)
      .withLabels(labels)
      .withWaitStrategy(Wait.forLogMessage(/API:/))
      .start(),
    new GenericContainer('axllent/mailpit')
      .withExposedPorts(1025, 8025)
      .withLabels(labels)
      .withWaitStrategy(Wait.forLogMessage(/accessible via/))
      .start(),
  ]);

  const pgHost = postgres.getHost();
  const pgPort = postgres.getMappedPort(5432);
  const mqHost = rabbitmq.getHost();
  const mqPort = rabbitmq.getMappedPort(5672);
  const s3Host = minio.getHost();
  const s3Port = minio.getMappedPort(9000);
  const mpHost = mailpit.getHost();
  const mpSmtp = mailpit.getMappedPort(1025);
  const mpHttp = mailpit.getMappedPort(8025);

  return {
    runId,
    databaseUrl: `postgres://platform_migrator:dev-only@${pgHost}:${pgPort}/platform`,
    amqpUrl: `amqp://guest:guest@${mqHost}:${mqPort}/%2f`,
    s3Endpoint: `http://${s3Host}:${s3Port}`,
    s3AccessKey: 'minioadmin',
    s3SecretKey: 'minioadmin',
    smtpHost: mpHost,
    smtpPort: mpSmtp,
    mailpitHttpUrl: `http://${mpHost}:${mpHttp}`,
    containers: [postgres, rabbitmq, minio, mailpit],
  };
}

export async function stopStack(handles: StackHandles): Promise<void> {
  await Promise.all(
    handles.containers.map((c) =>
      c.stop({ timeout: 10_000 }).catch((e: unknown) => {
        // Best-effort: log and continue so one stuck container can't block teardown.
        console.error(`stopStack: failed to stop ${c.getId()}: ${String(e)}`);
      }),
    ),
  );
}
