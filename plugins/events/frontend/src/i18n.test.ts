/**
 * Pseudo-locale catalog completeness gate. Lingui's `pseudo` transformation
 * substitutes diacritics into every source string (a→à, e→ē, h→ĥ, …) at
 * compile time; this test asserts each entry has substance and at least one
 * non-ASCII codepoint. A wrapped string that bypassed the macro (or a Lingui
 * pipeline regression) would surface as an entry whose literal parts are
 * pure ASCII — that's the failure mode this catches.
 *
 * The DOM-level "no hardcoded strings" enforcement lives in
 * `e2e/pseudo.spec.ts`; this test is the cheap layer that runs in CI without
 * needing the dev stack.
 */

import { describe, expect, it } from 'vitest';
// Import the compiled catalog directly. Production code goes through
// `loadI18n('pseudo')` (the Lingui Vite plugin rewrites the `.po` import to
// the compiled module), but in vitest's resolver loading the `.js` directly
// is the more reliable path — it sidesteps the `import.meta.glob` indirection.
// @ts-expect-error — Lingui's compiled CJS output is untyped.
import pseudo from '../i18n/pseudo.js';

interface CompiledCatalog {
  messages: Record<string, unknown>;
}

// Strings whose characters are all in the printable-ASCII range. The class
// ` -~` covers U+0020..U+007E and sidesteps biome's "no control characters
// in regex" lint.
const ASCII_ONLY = /^[ -~]*$/;

describe('events pseudo catalog', () => {
  it('emits non-ASCII literal text for every msgid', () => {
    const catalog = pseudo as CompiledCatalog;
    const keys = Object.keys(catalog.messages);
    expect(keys.length, 'pseudo catalog must not be empty').toBeGreaterThan(0);

    const offenders: string[] = [];
    for (const [key, msg] of Object.entries(catalog.messages)) {
      const literals = collectLiterals(msg);
      // Empty literals → message is purely placeholders/plural shells; skip.
      if (literals.trim().length === 0) {
        continue;
      }
      if (ASCII_ONLY.test(literals)) {
        offenders.push(`${key}: ${JSON.stringify(msg)}`);
      }
    }
    expect(
      offenders,
      `pseudo entries with pure-ASCII literals (pseudo transform skipped?):\n${offenders.join('\n')}`,
    ).toEqual([]);
  });
});

/**
 * Walk Lingui's compiled message shape, collecting the literal string pieces
 * (skipping placeholder name arrays + plural-shell control keywords). The
 * compiled format is one of:
 *   "literal text"
 *   ["piece", ["placeholderName"], "more piece"]
 *   [[ "0", "plural", { one: [...], other: [...] }]]
 */
function collectLiterals(msg: unknown): string {
  if (typeof msg === 'string') {
    return msg;
  }
  if (Array.isArray(msg)) {
    // A leaf [name] placeholder reference is a 1-element array of a string —
    // skip those (the name is plain ASCII and isn't displayed).
    if (msg.length === 1 && typeof msg[0] === 'string') {
      return '';
    }
    return msg.map(collectLiterals).join('');
  }
  if (typeof msg === 'object' && msg !== null) {
    // Plural/select shell: values are recursive messages, keys are control
    // words ("one", "other", "=0", …) that we never display.
    return Object.values(msg).map(collectLiterals).join('');
  }
  return '';
}
